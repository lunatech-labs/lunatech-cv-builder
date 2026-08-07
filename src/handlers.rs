use crate::AppState;
use crate::auth::KeycloakConfig;
use crate::batch_review::{self, BatchEvent, BatchReviewJob};
use crate::cv_reviewer;
use crate::db::{CvOverviewItem, CvSummary, CvWithOwner, Db, OverviewStats};
use crate::pdf;
use crate::review_pdf;
use crate::users::User;
use axum::Json;
use axum::body::Body;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use uuid::Uuid;

type ApiError = (StatusCode, String);

// `{:#}` renders an anyhow error together with its source chain
// ("outer context: inner context: root cause"). Plain `{}` prints only the
// outermost context, which hides the actual failure — e.g. a Review parse
// error arrives as "parsing Review JSON from Claude" with serde's message
// (and the column it choked on) silently dropped.
fn err500<E: std::fmt::Display>(e: E) -> ApiError {
    tracing::error!("internal error: {:#}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
}

fn err400(msg: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

#[derive(Deserialize)]
pub struct YamlBody {
    pub yaml: String,
}

#[derive(Serialize)]
pub struct CreateResponse {
    pub id: Uuid,
}

fn extract_name(yaml: &str) -> String {
    let parsed: serde_yaml::Result<serde_yaml::Value> = serde_yaml::from_str(yaml);
    parsed
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .unwrap_or_default()
}

/// Optional `label:` field on the YAML — the client this version of the CV
/// is tailored for (e.g., "Disney"). Whitespace-only values are coerced to
/// `None` so the dashboard chip doesn't render an empty pill.
fn extract_label(yaml: &str) -> Option<String> {
    let parsed: serde_yaml::Result<serde_yaml::Value> = serde_yaml::from_str(yaml);
    parsed
        .ok()
        .and_then(|v| v.get("label").and_then(|n| n.as_str()).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
}

pub async fn list_cvs(
    State(db): State<Db>,
    Extension(user): Extension<User>,
) -> Result<Json<Vec<CvSummary>>, ApiError> {
    let rows = db.list(user.id).await.map_err(err500)?;
    Ok(Json(rows))
}

pub async fn create_cv(
    State(db): State<Db>,
    Extension(user): Extension<User>,
    Json(body): Json<YamlBody>,
) -> Result<Json<CreateResponse>, ApiError> {
    if body.yaml.trim().is_empty() {
        return Err(err400("yaml is empty"));
    }
    let name = extract_name(&body.yaml);
    let label = extract_label(&body.yaml);
    let id = db
        .create(user.id, &body.yaml, &name, label.as_deref())
        .await
        .map_err(err500)?;
    Ok(Json(CreateResponse { id }))
}

/// `GET /api/cvs/{id}` — readable by any authenticated user (Lunatech-internal
/// trust model: every Lunatech recruiter can browse every consultant's CV).
/// The `owner` field lets the frontend detect non-owned CVs and switch the
/// editor into a read-only mode. Write operations (PUT / DELETE / reviews)
/// stay strictly owner-scoped at the DB level.
#[derive(Serialize)]
pub struct CvDetail {
    #[serde(flatten)]
    pub cv: CvWithOwner,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_review: Option<cv_reviewer::Review>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_review_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_cv(
    State(db): State<Db>,
    Path(id): Path<Uuid>,
) -> Result<Json<CvDetail>, ApiError> {
    let cv = db
        .get_any(id)
        .await
        .map_err(err500)?
        .ok_or((StatusCode::NOT_FOUND, "cv not found".into()))?;
    let (latest_review, latest_review_at) = match db.latest_review(id).await.map_err(err500)? {
        Some((r, ts)) => (Some(r), Some(ts)),
        None => (None, None),
    };
    Ok(Json(CvDetail {
        cv,
        latest_review,
        latest_review_at,
    }))
}

/// Fetches the CV and returns 404 if it doesn't exist, 403 if the caller is
/// neither the owner nor an admin. Used as a guard at the top of every
/// mutating handler so the actual SQL stays simple.
async fn require_write_access(
    db: &Db,
    user: &User,
    id: Uuid,
) -> Result<(), ApiError> {
    let cv = db
        .get_any(id)
        .await
        .map_err(err500)?
        .ok_or((StatusCode::NOT_FOUND, "cv not found".into()))?;
    if !user.is_admin && cv.owner.id != user.id {
        return Err((StatusCode::FORBIDDEN, "not your CV".into()));
    }
    Ok(())
}

pub async fn update_cv(
    State(db): State<Db>,
    Extension(user): Extension<User>,
    Path(id): Path<Uuid>,
    Json(body): Json<YamlBody>,
) -> Result<StatusCode, ApiError> {
    if body.yaml.trim().is_empty() {
        return Err(err400("yaml is empty"));
    }
    require_write_access(&db, &user, id).await?;
    let name = extract_name(&body.yaml);
    let label = extract_label(&body.yaml);
    let updated = db
        .update_any(id, &body.yaml, &name, label.as_deref())
        .await
        .map_err(err500)?;
    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        // Vanishingly unlikely race: row went away between the auth fetch
        // and the update. Surface as 404 — the caller will refresh.
        Err((StatusCode::NOT_FOUND, "cv not found".into()))
    }
}

pub async fn delete_cv(
    State(db): State<Db>,
    Extension(user): Extension<User>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_write_access(&db, &user, id).await?;
    let deleted = db.delete_any(id).await.map_err(err500)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "cv not found".into()))
    }
}

#[derive(Deserialize)]
pub struct PdfQuery {
    pub theme: Option<String>,
}

pub async fn pdf_cv(
    State(db): State<Db>,
    Path(id): Path<Uuid>,
    Query(q): Query<PdfQuery>,
) -> Result<Response, ApiError> {
    let rec = db
        .get_any(id)
        .await
        .map_err(err500)?
        .ok_or((StatusCode::NOT_FOUND, "cv not found".into()))?;
    let theme = q.theme.as_deref().unwrap_or("cosmic");
    let bytes = pdf::render(&rec.yaml, theme).map_err(err500)?;
    let base = if rec.name.trim().is_empty() {
        "cv".to_string()
    } else {
        rec.name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    };
    let filename = format!("cv-{}.pdf", base);
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", filename),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Public bootstrap config the frontend needs to wire up keycloak-js. Always
/// returns 200 — the frontend decides what to do based on which fields are
/// populated. Never includes secrets (no API keys, only public OIDC ids).
#[derive(Serialize)]
pub struct PublicConfig {
    pub keycloak: Option<KeycloakConfig>,
    pub anthropic_enabled: bool,
}

pub async fn get_config(State(state): State<AppState>) -> Json<PublicConfig> {
    Json(PublicConfig {
        keycloak: state.keycloak.clone(),
        anthropic_enabled: state.anthropic.is_some(),
    })
}

/// Landing-page payload — bundles everything the Overview view needs in one
/// round-trip: the caller's identity, platform-wide stats, the caller's CVs
/// (for the editing entry points), the cross-user ranking, and the flat
/// catalog of every CV (so recruiters can browse + download from anyone).
#[derive(Serialize)]
pub struct OverviewPayload {
    pub me: User,
    pub stats: OverviewStats,
    pub my_cvs: Vec<CvOverviewItem>,
    pub top_cvs: Vec<CvOverviewItem>,
    pub all_cvs: Vec<CvOverviewItem>,
}

const TOP_CVS_LIMIT: i64 = 10;

pub async fn get_overview(
    State(db): State<Db>,
    Extension(user): Extension<User>,
) -> Result<Json<OverviewPayload>, ApiError> {
    let stats = db.overview_stats(user.id).await.map_err(err500)?;
    let my_cvs = db.my_cvs_with_review(user.id).await.map_err(err500)?;
    let top_cvs = db.top_cvs(TOP_CVS_LIMIT).await.map_err(err500)?;
    let all_cvs = db.all_cvs_with_review().await.map_err(err500)?;

    Ok(Json(OverviewPayload {
        me: user,
        stats,
        my_cvs,
        top_cvs,
        all_cvs,
    }))
}

/// Body for `POST /api/review/pdf` — the review object the frontend already
/// has in memory plus an optional CV name for the PDF title.
#[derive(Deserialize)]
pub struct ReviewPdfBody {
    pub review: cv_reviewer::Review,
    #[serde(default)]
    pub cv_name: Option<String>,
}

/// Renders a review (returned earlier from `POST /api/review`) as a Lunatech-
/// branded PDF and streams it back. Stateless — doesn't touch the DB or call
/// Claude. Useful when the recruiter wants to share the report with the
/// consultant or attach it to a ticket.
pub async fn review_pdf_handler(
    Json(body): Json<ReviewPdfBody>,
) -> Result<Response, ApiError> {
    let bytes = review_pdf::render(&body.review, body.cv_name.as_deref()).map_err(err500)?;
    let base = body
        .cv_name
        .as_deref()
        .map(|n| n.trim())
        .filter(|n| !n.is_empty())
        .map(|n| {
            n.to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .trim_matches('-')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "review".to_string());
    let filename = format!("cv-review-{}.pdf", base);
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Runs Claude with the cv-reviewer skill on a saved CV, persists the
/// resulting review, and returns it. The caller must own the CV (or be an
/// admin); a non-owning, non-admin caller gets 403. Returns 503 if
/// `ANTHROPIC_API_KEY` isn't set.
///
/// The Anthropic call routinely takes 5-15 minutes for a long CV, which
/// blows past the idle-timeout of any reverse proxy in front of axum
/// (CleverCloud's default is 5 min → 504 Gateway Timeout). To keep the
/// proxy happy we stream the response back to the browser: a single
/// space byte every 15s while the call is in flight, then the JSON
/// payload at the end. JSON allows arbitrary whitespace before the
/// opening `{`, so `await res.json()` on the client just works.
///
/// Errors that happen *after* headers are sent (Anthropic call failure,
/// DB save failure) can't change the status code anymore — we encode
/// them as `{"error": "..."}` in the JSON body, and the frontend checks
/// for that field. Pre-stream errors (auth, ownership, missing config)
/// still return their proper status codes.
pub async fn review_cv(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(cv_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let cfg = state.anthropic.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "ANTHROPIC_API_KEY is not set on the server".to_string(),
    ))?;

    let cv = state
        .db
        .get_any(cv_id)
        .await
        .map_err(err500)?
        .ok_or((StatusCode::NOT_FOUND, "cv not found".into()))?;
    if !user.is_admin && cv.owner.id != user.id {
        return Err((StatusCode::FORBIDDEN, "not your CV".into()));
    }

    let cfg = cfg.clone();
    let yaml = cv.yaml.clone();
    let db = state.db.clone();
    let user_id = user.id;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);

    tokio::spawn(async move {
        let review_fut = cv_reviewer::review(&cfg, &yaml);
        tokio::pin!(review_fut);

        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        // Skip the first immediate tick — we don't need a heartbeat before
        // any work has started.
        heartbeat.tick().await;

        let result = loop {
            tokio::select! {
                biased;
                r = &mut review_fut => break r,
                _ = heartbeat.tick() => {
                    if tx.send(Ok(Bytes::from_static(b" "))).await.is_err() {
                        // Client disconnected — drop the in-flight call.
                        return;
                    }
                }
            }
        };

        let payload = match result {
            Ok(review) => {
                // The review is attributed to the caller (whoever ran it),
                // not the owner — that's accurate provenance even when an
                // admin is reviewing someone else's CV.
                if let Err(e) = db.save_review(cv_id, user_id, &review, &yaml).await {
                    tracing::error!("save_review failed: {e}");
                    serde_json::json!({ "error": format!("save_review: {e}") })
                } else {
                    serde_json::to_value(&review).unwrap_or_else(|e| {
                        serde_json::json!({ "error": format!("serialise review: {e}") })
                    })
                }
            }
            Err(e) => {
                // `{:#}` keeps the source chain. This is the path the review
                // button actually takes, so it is the one that has to carry
                // the real cause to the browser.
                tracing::error!("cv_reviewer::review failed: {:#}", e);
                serde_json::json!({ "error": format!("{e:#}") })
            }
        };

        let bytes = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
        let _ = tx.send(Ok(Bytes::from(bytes))).await;
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let body = Body::from_stream(stream);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body)
        .map_err(err500)
}

// ─────────────────────────── Batch reviews (admin) ───────────────────────────

#[derive(Deserialize)]
pub struct BatchReviewQuery {
    #[serde(default)]
    pub force: bool,
}

#[derive(Serialize)]
pub struct BatchReviewStarted {
    pub job_id: Uuid,
    pub total: usize,
    pub started_at: chrono::DateTime<Utc>,
    pub force: bool,
}

/// Admin-only kickoff for the "review every CV" batch. Returns 202 with a
/// snapshot of the brand-new job; the heavy lifting runs in a detached
/// `tokio::spawn`. Concurrent runs are rejected with 409 — admins can
/// follow the existing job via the SSE stream instead.
pub async fn batch_reviews_start(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(q): Query<BatchReviewQuery>,
) -> Result<(StatusCode, Json<BatchReviewStarted>), ApiError> {
    if !user.is_admin {
        return Err((StatusCode::FORBIDDEN, "admin only".into()));
    }
    let cfg = state.anthropic.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "ANTHROPIC_API_KEY is not set on the server".to_string(),
    ))?;

    // Single-flight: refuse if the previous job is still running. A finished
    // job is left in place so a fresh subscriber can still pick up its
    // final snapshot — only when the next batch starts do we replace it.
    {
        let guard = state.batch_review.read().await;
        if let Some(job) = guard.as_ref() {
            if job.is_running() {
                return Err((
                    StatusCode::CONFLICT,
                    "a batch review is already running".into(),
                ));
            }
        }
    }

    let catalog = state.db.all_cvs_with_review().await.map_err(err500)?;
    let work = batch_review::select_work_list(catalog, q.force);
    let total = work.len();
    let job_id = Uuid::new_v4();
    let started_at = Utc::now();
    let (tx, _) = tokio::sync::broadcast::channel::<BatchEvent>(64);

    let job = BatchReviewJob {
        job_id,
        started_at,
        started_by: user.id,
        force: q.force,
        total,
        done: 0,
        succeeded: 0,
        failed: Vec::new(),
        current: Vec::new(),
        completed_at: None,
        events: tx,
    };

    {
        let mut guard = state.batch_review.write().await;
        *guard = Some(job);
    }

    // If the staleness filter trimmed the work list to zero we still want a
    // clean lifecycle: spawn the worker, it has nothing to do, and emits a
    // Done event so the SSE modal shows "0/0 done" and closes cleanly.
    let shared = state.batch_review.clone();
    let db = state.db.clone();
    let cfg = cfg.clone();
    let started_by = user.id;
    tokio::spawn(async move {
        batch_review::run(shared, db, cfg, work, started_by).await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(BatchReviewStarted {
            job_id,
            total,
            started_at,
            force: q.force,
        }),
    ))
}

/// Admin-only SSE stream of batch-review events. On connect, sends one
/// snapshot event so a late subscriber syncs from a single message; then
/// forwards every broadcast event until `Done` arrives, then closes. If
/// no job has been started since boot, sends a synthetic idle "done"
/// event and closes immediately.
pub async fn batch_reviews_events(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if !user.is_admin {
        return Err((StatusCode::FORBIDDEN, "admin only".into()));
    }

    // Snapshot + a broadcast subscription, both produced under one lock so
    // the snapshot is consistent with the events that follow it.
    let (initial_event, mut rx, terminal) = {
        let guard = state.batch_review.read().await;
        match guard.as_ref() {
            Some(job) => {
                let snapshot = job.snapshot();
                let initial = Event::default()
                    .event(if job.is_running() { "snapshot" } else { "done" })
                    .json_data(&snapshot)
                    .map_err(err500)?;
                let rx = job.events.subscribe();
                let terminal = !job.is_running();
                (initial, rx, terminal)
            }
            None => {
                // No job has ever run — send an "idle" event and close.
                let initial = Event::default().event("idle").data("{}");
                // A receiver that immediately closes — we'll drop it below.
                let (_tx, rx) = tokio::sync::broadcast::channel::<BatchEvent>(1);
                (initial, rx, true)
            }
        }
    };

    let (out_tx, out_rx) =
        tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);

    // Push the snapshot first so any subscriber sees current state on
    // arrival, even if no broadcast events follow.
    let _ = out_tx.send(Ok(initial_event)).await;

    if !terminal {
        tokio::spawn(async move {
            // Forward every broadcast event, mapping the discriminant to the
            // SSE event name so the frontend can `switch (event.event)`.
            // Lagged subscribers (slow clients) just resync from the next
            // event's snapshot — that's why we always include one.
            loop {
                let evt = match rx.recv().await {
                    Ok(e) => e,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                let (name, done) = match &evt {
                    BatchEvent::Started { .. } => ("started", false),
                    BatchEvent::Progress { .. } => ("progress", false),
                    BatchEvent::Done { .. } => ("done", true),
                };
                let sse_event = match Event::default().event(name).json_data(&evt) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::error!("encoding SSE event: {e}");
                        continue;
                    }
                };
                if out_tx.send(Ok(sse_event)).await.is_err() {
                    break;
                }
                if done {
                    break;
                }
            }
        });
    }

    let stream = tokio_stream::wrappers::ReceiverStream::new(out_rx);
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_name_simple() {
        assert_eq!(extract_name("name: Alice\ntitle: Engineer"), "Alice");
    }

    #[test]
    fn extract_name_quoted() {
        assert_eq!(extract_name("name: \"Alice O'Brien\""), "Alice O'Brien");
    }

    #[test]
    fn extract_name_missing() {
        assert_eq!(extract_name("title: Engineer\nfoo: bar"), "");
    }

    #[test]
    fn extract_name_invalid_yaml_returns_empty() {
        assert_eq!(extract_name("name: : :\n  - broken"), "");
    }

    #[test]
    fn extract_name_non_string_returns_empty() {
        // If `name` is a number or list, we cannot use it as a display name.
        assert_eq!(extract_name("name: 42"), "");
        assert_eq!(extract_name("name:\n  - a\n  - b"), "");
    }

    #[test]
    fn extract_name_empty_yaml_returns_empty() {
        assert_eq!(extract_name(""), "");
        assert_eq!(extract_name("   "), "");
    }
}

