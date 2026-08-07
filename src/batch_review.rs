// Background job that runs the cv-reviewer skill against many CVs at once.
//
// Triggered by an admin from the overview page. The job state lives in
// memory only (a single `Arc<RwLock<Option<BatchReviewJob>>>` on the
// AppState) — losing it on restart is acceptable because the only
// consequence is the admin re-running the batch. Per-CV reviews that
// finished before the crash are already persisted in the `reviews` table.
//
// Concurrency: capped at MAX_CONCURRENT Claude calls in flight. Anthropic
// has token-per-minute and request-per-minute limits and a runaway batch
// would also starve normal one-off reviews from non-admin users.
//
// SSE feed: every state change broadcasts a `BatchEvent` carrying the
// current `Snapshot` so a freshly-connected subscriber can sync from a
// single message.
//
// The `force` flag controls the staleness filter — see `select_work_list`.

use crate::cv_reviewer::{self, AnthropicConfig};
use crate::db::{CvOverviewItem, Db};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore, broadcast};
use uuid::Uuid;

/// Cap on concurrent Anthropic calls — keeps us under rate limits and
/// leaves headroom for one-off reviews triggered by other users.
const MAX_CONCURRENT: usize = 3;

/// Per-CV ticker we share over SSE — the frontend renders a "now reviewing
/// X" line from the `current` list and a recent-completions tail from the
/// `failed` plus latest `succeeded` events.
#[derive(Clone, Debug, Serialize)]
pub struct CvLabel {
    pub cv_id: Uuid,
    pub cv_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BatchFailure {
    pub cv_id: Uuid,
    pub cv_name: String,
    pub reason: String,
}

/// Snapshot of the job's overall state at a single point in time. Rides on
/// every event so a late SSE subscriber catches up without us having to
/// replay history.
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub job_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub started_by: Uuid,
    pub force: bool,
    pub total: usize,
    pub done: usize,
    pub succeeded: usize,
    pub failed: Vec<BatchFailure>,
    pub current: Vec<CvLabel>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Discriminated event shape sent to SSE subscribers. Each event type also
/// carries the full snapshot so a client that connects mid-batch can
/// initialise from any single message.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchEvent {
    /// Sent once per CV when its review starts. Lets the modal render
    /// "now reviewing: …" before the (slow) Claude call returns.
    Started { cv: CvLabel, snapshot: Snapshot },
    /// Sent once per CV when its review finishes (success or failure).
    Progress {
        cv: CvLabel,
        ok: bool,
        reason: Option<String>,
        snapshot: Snapshot,
    },
    /// Final event when the worker finishes the work list. Closes the SSE
    /// stream from the server side.
    Done { snapshot: Snapshot },
}

/// Mutable state for one in-flight batch. Wrapped in `RwLock` on the
/// AppState so handlers + worker can update it concurrently.
pub struct BatchReviewJob {
    pub job_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub started_by: Uuid,
    pub force: bool,
    pub total: usize,
    pub done: usize,
    pub succeeded: usize,
    pub failed: Vec<BatchFailure>,
    pub current: Vec<CvLabel>,
    pub completed_at: Option<DateTime<Utc>>,
    pub events: broadcast::Sender<BatchEvent>,
}

impl BatchReviewJob {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            job_id: self.job_id,
            started_at: self.started_at,
            started_by: self.started_by,
            force: self.force,
            total: self.total,
            done: self.done,
            succeeded: self.succeeded,
            failed: self.failed.clone(),
            current: self.current.clone(),
            completed_at: self.completed_at,
        }
    }

    pub fn is_running(&self) -> bool {
        self.completed_at.is_none()
    }
}

pub type SharedBatchState = Arc<RwLock<Option<BatchReviewJob>>>;

/// Filter the CV catalog down to the work list. Without `force`, we skip
/// CVs whose latest review is newer than the CV's `updated_at` — they
/// haven't been edited since their last review and re-running Claude
/// would just spend tokens on the same answer. With `force`, we include
/// every CV (used after a SKILL.md tweak when the rubric changed).
pub fn select_work_list(catalog: Vec<CvOverviewItem>, force: bool) -> Vec<CvOverviewItem> {
    if force {
        return catalog;
    }
    catalog
        .into_iter()
        .filter(|cv| match cv.latest_review_at {
            None => true,
            Some(reviewed_at) => reviewed_at < cv.updated_at,
        })
        .collect()
}

/// Drives the batch through the work list: spawn a per-CV future under a
/// semaphore, broadcast `Started` / `Progress` events, persist successes
/// via `db.save_review`. The worker is detached — the kickoff handler
/// returns 202 immediately. State is shared with SSE subscribers via
/// `state` and `events`.
pub async fn run(
    state: SharedBatchState,
    db: Db,
    cfg: AnthropicConfig,
    work: Vec<CvOverviewItem>,
    started_by: Uuid,
) {
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut tasks = Vec::with_capacity(work.len());

    for cv in work {
        let permit = match sem.clone().acquire_owned().await {
            Ok(p) => p,
            // The semaphore is never closed in normal operation; if it is,
            // we just stop dispatching new work and let the already-spawned
            // tasks finish.
            Err(_) => break,
        };
        let state_for_task = state.clone();
        let db_for_task = db.clone();
        let cfg_for_task = cfg.clone();

        tasks.push(tokio::spawn(async move {
            // The permit drops at the end of this task, releasing the slot
            // for the next CV.
            let _permit = permit;
            let label = CvLabel {
                cv_id: cv.id,
                cv_name: cv.name.clone(),
            };

            // Mark "started" for the modal's "currently reviewing" line.
            broadcast_started(&state_for_task, &label).await;

            // Re-fetch the CV's YAML — `all_cvs_with_review` doesn't carry
            // it, and we want the freshest version anyway in case it was
            // edited after the work list was built.
            let yaml = match db_for_task.get_any(cv.id).await {
                Ok(Some(record)) => record.yaml,
                Ok(None) => {
                    finish_one(
                        &state_for_task,
                        &label,
                        Err("CV no longer exists".to_string()),
                    )
                    .await;
                    return;
                }
                Err(e) => {
                    finish_one(&state_for_task, &label, Err(format!("db.get_any: {e}"))).await;
                    return;
                }
            };

            let result = cv_reviewer::review(&cfg_for_task, &yaml).await;
            let outcome = match result {
                Ok(review) => {
                    if let Err(e) = db_for_task
                        .save_review(cv.id, started_by, &review, &yaml)
                        .await
                    {
                        Err(format!("save_review: {e}"))
                    } else {
                        Ok(())
                    }
                }
                // `{:#}` keeps the source chain; plain `to_string()` would
                // report only the outermost context.
                Err(e) => Err(format!("{e:#}")),
            };
            finish_one(&state_for_task, &label, outcome).await;
        }));
    }

    // Wait for every task — drives the semaphore to drain and lets us emit
    // a final `Done` event after the last CV.
    for t in tasks {
        let _ = t.await;
    }

    let snapshot = {
        let mut guard = state.write().await;
        if let Some(job) = guard.as_mut() {
            job.completed_at = Some(Utc::now());
            let snap = job.snapshot();
            let _ = job.events.send(BatchEvent::Done {
                snapshot: snap.clone(),
            });
            Some(snap)
        } else {
            None
        }
    };

    if let Some(s) = snapshot {
        tracing::info!(
            job_id = %s.job_id,
            total = s.total,
            succeeded = s.succeeded,
            failed = s.failed.len(),
            "batch review completed"
        );
    }
}

async fn broadcast_started(state: &SharedBatchState, label: &CvLabel) {
    let mut guard = state.write().await;
    if let Some(job) = guard.as_mut() {
        job.current.push(label.clone());
        let snapshot = job.snapshot();
        let _ = job.events.send(BatchEvent::Started {
            cv: label.clone(),
            snapshot,
        });
    }
}

async fn finish_one(state: &SharedBatchState, label: &CvLabel, outcome: Result<(), String>) {
    let mut guard = state.write().await;
    let Some(job) = guard.as_mut() else {
        return;
    };
    job.current.retain(|c| c.cv_id != label.cv_id);
    job.done += 1;
    let (ok, reason) = match outcome {
        Ok(()) => {
            job.succeeded += 1;
            (true, None)
        }
        Err(e) => {
            job.failed.push(BatchFailure {
                cv_id: label.cv_id,
                cv_name: label.cv_name.clone(),
                reason: e.clone(),
            });
            (false, Some(e))
        }
    };
    let snapshot = job.snapshot();
    let _ = job.events.send(BatchEvent::Progress {
        cv: label.clone(),
        ok,
        reason,
        snapshot,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cv(reviewed_at: Option<DateTime<Utc>>, updated_at: DateTime<Utc>) -> CvOverviewItem {
        CvOverviewItem {
            id: Uuid::new_v4(),
            name: "Test".into(),
            updated_at,
            owner_id: Uuid::nil(),
            owner_name: None,
            label: None,
            latest_score: None,
            latest_verdict: None,
            latest_review_at: reviewed_at,
            seniority_score: None,
            seniority_level: None,
        }
    }

    #[test]
    fn select_includes_unreviewed() {
        let items = vec![cv(None, Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap())];
        let out = select_work_list(items, false);
        assert_eq!(out.len(), 1, "unreviewed CV should be in the work list");
    }

    #[test]
    fn select_excludes_already_reviewed_after_last_edit() {
        let updated = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let reviewed = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
        let items = vec![cv(Some(reviewed), updated)];
        let out = select_work_list(items, false);
        assert!(
            out.is_empty(),
            "review newer than edit should drop the CV from the work list"
        );
    }

    #[test]
    fn select_includes_stale_review() {
        let updated = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        let reviewed = Utc.with_ymd_and_hms(2026, 1, 15, 0, 0, 0).unwrap();
        let items = vec![cv(Some(reviewed), updated)];
        let out = select_work_list(items, false);
        assert_eq!(
            out.len(),
            1,
            "edit newer than review should re-include the CV"
        );
    }

    #[test]
    fn force_includes_everything() {
        let updated = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let reviewed = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
        let items = vec![
            cv(None, updated),
            cv(Some(reviewed), updated),
            cv(Some(updated), updated),
        ];
        let out = select_work_list(items, true);
        assert_eq!(out.len(), 3, "force=true should keep every CV");
    }
}
