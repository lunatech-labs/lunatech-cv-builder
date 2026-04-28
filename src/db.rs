use crate::cv_reviewer::Review;
use crate::seniority;
use crate::users::User;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value as Json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[derive(Clone)]
pub struct Db {
    pub pool: PgPool,
}

#[derive(Serialize)]
pub struct CvSummary {
    pub id: Uuid,
    pub name: String,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Serialize)]
pub struct CvRecord {
    pub id: Uuid,
    pub name: String,
    pub yaml: String,
    pub updated_at: DateTime<Utc>,
}

impl Db {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let db = Self { pool };
        // One-shot backfill: any CV missing the seniority snapshot gets one
        // at boot. Idempotent — re-running is a no-op once the column is
        // populated. Cheap (< a few ms per CV) for our scale.
        db.backfill_seniority().await?;
        Ok(db)
    }

    pub async fn backfill_seniority(&self) -> Result<usize> {
        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, yaml FROM cvs WHERE seniority IS NULL")
                .fetch_all(&self.pool)
                .await?;
        let n = rows.len();
        for (id, yaml) in rows {
            let report = seniority::score_yaml(&yaml);
            self.persist_seniority(id, &report).await?;
        }
        if n > 0 {
            tracing::info!("seniority: backfilled {n} CV(s)");
        }
        Ok(n)
    }

    async fn persist_seniority(&self, cv_id: Uuid, report: &seniority::Report) -> Result<()> {
        let payload = serde_json::to_value(report).context("serialising Seniority report")?;
        sqlx::query(
            "UPDATE cvs SET seniority = $1, seniority_score = $2, seniority_level = $3
             WHERE id = $4",
        )
        .bind(payload)
        .bind(report.score as i16)
        .bind(&report.level)
        .bind(cv_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Idempotent — refresh email/name on every login, bump last_seen_at.
    pub async fn upsert_user(&self, u: &User) -> Result<()> {
        sqlx::query(
            "INSERT INTO users (id, email, name) VALUES ($1, $2, $3)
             ON CONFLICT (id) DO UPDATE SET
               email = EXCLUDED.email,
               name = EXCLUDED.name,
               last_seen_at = NOW()",
        )
        .bind(u.id)
        .bind(u.email.as_deref())
        .bind(u.name.as_deref())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ────────── CV operations — all scoped to the calling user ──────────
    //
    // Every query takes the owner's user_id and filters on it. A CV that
    // belongs to a different user is invisible (404 on get/update/delete)
    // — we never leak it via cross-user lookups.

    pub async fn list(&self, user_id: Uuid) -> Result<Vec<CvSummary>> {
        let rows = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>, Option<String>)>(
            "SELECT id, name, updated_at, label
             FROM cvs WHERE user_id = $1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, updated_at, label)| CvSummary {
                id,
                name,
                updated_at,
                label,
            })
            .collect())
    }

    pub async fn create(
        &self,
        user_id: Uuid,
        yaml: &str,
        name: &str,
        label: Option<&str>,
    ) -> Result<Uuid> {
        let report = seniority::score_yaml(yaml);
        let payload = serde_json::to_value(&report).context("serialising Seniority report")?;
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO cvs (user_id, name, yaml, label, seniority, seniority_score, seniority_level)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(user_id)
        .bind(name)
        .bind(yaml)
        .bind(label)
        .bind(payload)
        .bind(report.score as i16)
        .bind(&report.level)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn get(&self, user_id: Uuid, id: Uuid) -> Result<Option<CvRecord>> {
        let row = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
            "SELECT id, name, yaml, updated_at FROM cvs WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, name, yaml, updated_at)| CvRecord {
            id,
            name,
            yaml,
            updated_at,
        }))
    }

    // ────────── Mutating ops — must run after a handler-level guard ──────
    //
    // The SQL is unscoped: callers must NOT invoke these without first
    // verifying that the caller is the owner or an admin (see
    // `require_write_access` in handlers.rs).

    pub async fn update_any(
        &self,
        id: Uuid,
        yaml: &str,
        name: &str,
        label: Option<&str>,
    ) -> Result<bool> {
        let report = seniority::score_yaml(yaml);
        let payload = serde_json::to_value(&report).context("serialising Seniority report")?;
        let result = sqlx::query(
            "UPDATE cvs
             SET yaml = $1, name = $2, label = $3,
                 seniority = $4, seniority_score = $5, seniority_level = $6,
                 updated_at = NOW()
             WHERE id = $7",
        )
        .bind(yaml)
        .bind(name)
        .bind(label)
        .bind(payload)
        .bind(report.score as i16)
        .bind(&report.level)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_any(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM cvs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ────────── Reviews ──────────

    /// Records a fresh review for a CV alongside the YAML it was run against.
    /// Caller is responsible for verifying the CV belongs to `user_id` first
    /// (typically by having just called `get(user_id, cv_id)`).
    pub async fn save_review(
        &self,
        cv_id: Uuid,
        user_id: Uuid,
        review: &Review,
        yaml_snapshot: &str,
    ) -> Result<Uuid> {
        let payload = serde_json::to_value(review).context("serialising Review for storage")?;
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO reviews (cv_id, user_id, overall_score, verdict, language, payload, yaml_snapshot)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(cv_id)
        .bind(user_id)
        .bind(i16::from(review.overall_score))
        .bind(&review.verdict)
        .bind(&review.language)
        .bind(payload)
        .bind(yaml_snapshot)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Returns the most recent review for a CV regardless of who ran it.
    /// All authenticated users can see each other's reviews on the Overview
    /// + read-only-CV surfaces; the *write* side (running a new review)
    /// stays owner-only and is enforced in the handler.
    pub async fn latest_review(
        &self,
        cv_id: Uuid,
    ) -> Result<Option<(Review, DateTime<Utc>)>> {
        let row: Option<(serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
            "SELECT payload, created_at FROM reviews
             WHERE cv_id = $1
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(cv_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some((payload, ts)) => {
                let review: Review = serde_json::from_value(payload)
                    .context("deserialising stored Review payload")?;
                Ok(Some((review, ts)))
            }
            None => Ok(None),
        }
    }

    // ────────── Cross-user reads (Overview + read-only viewer) ──────────

    /// Reads any CV regardless of owner — used by the read-only viewer and
    /// the PDF download. Returns the record plus the owner's identity so
    /// the frontend can show "by Alice Smith" and detect non-owned CVs.
    pub async fn get_any(&self, id: Uuid) -> Result<Option<CvWithOwner>> {
        let row: Option<(
            Uuid,
            String,
            String,
            DateTime<Utc>,
            Option<String>,
            Uuid,
            Option<String>,
            Option<String>,
            Option<Json>,
        )> = sqlx::query_as(
            "SELECT c.id, c.name, c.yaml, c.updated_at, c.label,
                    u.id, u.name, u.email,
                    c.seniority
             FROM cvs c JOIN users u ON u.id = c.user_id
             WHERE c.id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(
            |(id, name, yaml, updated_at, label, owner_id, owner_name, owner_email, seniority)| {
                CvWithOwner {
                    id,
                    name,
                    yaml,
                    updated_at,
                    owner: Owner {
                        id: owner_id,
                        name: owner_name,
                        email: owner_email,
                    },
                    label,
                    seniority: seniority.and_then(|v| serde_json::from_value(v).ok()),
                }
            },
        ))
    }

    /// Computes the same 4-tuple of stats both for the calling user (`mine`)
    /// and for the entire workspace (`company`) in a single round-trip — the
    /// CTE for `latest` is shared, only the WHERE on the per-user side
    /// differs.
    pub async fn overview_stats(&self, user_id: Uuid) -> Result<OverviewStats> {
        let row: (
            i64,
            i64,
            Option<f64>,
            i64,
            i64,
            i64,
            Option<f64>,
            i64,
        ) = sqlx::query_as(
            "WITH all_latest AS (
                 SELECT DISTINCT ON (r.cv_id)
                     r.cv_id, r.overall_score, r.verdict, c.user_id
                 FROM reviews r
                 JOIN cvs c ON c.id = r.cv_id
                 ORDER BY r.cv_id, r.created_at DESC
             )
             SELECT
                 -- Mine
                 (SELECT COUNT(*) FROM cvs WHERE user_id = $1)::bigint,
                 (SELECT COUNT(*) FROM all_latest WHERE user_id = $1)::bigint,
                 (SELECT AVG(overall_score)::float8 FROM all_latest WHERE user_id = $1),
                 (SELECT COUNT(*) FROM all_latest
                    WHERE user_id = $1 AND verdict = 'client_ready')::bigint,
                 -- Company-wide
                 (SELECT COUNT(*) FROM cvs)::bigint,
                 (SELECT COUNT(*) FROM all_latest)::bigint,
                 (SELECT AVG(overall_score)::float8 FROM all_latest),
                 (SELECT COUNT(*) FROM all_latest WHERE verdict = 'client_ready')::bigint",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(OverviewStats {
            mine: ScopedStats {
                total_cvs: row.0 as u32,
                reviewed_cvs: row.1 as u32,
                avg_score: row.2,
                client_ready_count: row.3 as u32,
            },
            company: ScopedStats {
                total_cvs: row.4 as u32,
                reviewed_cvs: row.5 as u32,
                avg_score: row.6,
                client_ready_count: row.7 as u32,
            },
        })
    }

    /// All CVs owned by a user, with the score of their latest review (any
    /// reviewer; in practice the owner since only owners can run reviews).
    pub async fn my_cvs_with_review(&self, user_id: Uuid) -> Result<Vec<CvOverviewItem>> {
        sqlx::query_as::<_, CvOverviewItem>(
            "SELECT
                c.id, c.name, c.updated_at,
                u.id   AS owner_id,
                u.name AS owner_name,
                c.label,
                r.overall_score AS latest_score,
                r.verdict       AS latest_verdict,
                r.created_at    AS latest_review_at,
                c.seniority_score,
                c.seniority_level
             FROM cvs c
             JOIN users u ON u.id = c.user_id
             LEFT JOIN LATERAL (
                 SELECT overall_score, verdict, created_at
                 FROM reviews
                 WHERE cv_id = c.id
                 ORDER BY created_at DESC
                 LIMIT 1
             ) r ON TRUE
             WHERE c.user_id = $1
             ORDER BY c.updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("loading my_cvs_with_review")
    }

    /// Every CV across the platform, sorted by recency. Used by the
    /// Overview's "All CVs" listing — recruiters need a flat catalog they
    /// can browse and download from regardless of review status.
    pub async fn all_cvs_with_review(&self) -> Result<Vec<CvOverviewItem>> {
        sqlx::query_as::<_, CvOverviewItem>(
            "SELECT
                c.id, c.name, c.updated_at,
                u.id   AS owner_id,
                u.name AS owner_name,
                c.label,
                r.overall_score AS latest_score,
                r.verdict       AS latest_verdict,
                r.created_at    AS latest_review_at,
                c.seniority_score,
                c.seniority_level
             FROM cvs c
             JOIN users u ON u.id = c.user_id
             LEFT JOIN LATERAL (
                 SELECT overall_score, verdict, created_at
                 FROM reviews
                 WHERE cv_id = c.id
                 ORDER BY created_at DESC
                 LIMIT 1
             ) r ON TRUE
             ORDER BY c.updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("loading all_cvs_with_review")
    }

    /// Top N CVs across the platform, ranked by their latest review's score
    /// (descending). CVs that have never been reviewed are excluded — there's
    /// nothing to rank them by. Ties are broken by recency.
    pub async fn top_cvs(&self, limit: i64) -> Result<Vec<CvOverviewItem>> {
        sqlx::query_as::<_, CvOverviewItem>(
            "SELECT
                c.id, c.name, c.updated_at,
                u.id   AS owner_id,
                u.name AS owner_name,
                c.label,
                r.overall_score AS latest_score,
                r.verdict       AS latest_verdict,
                r.created_at    AS latest_review_at,
                c.seniority_score,
                c.seniority_level
             FROM cvs c
             JOIN users u ON u.id = c.user_id
             JOIN LATERAL (
                 SELECT overall_score, verdict, created_at
                 FROM reviews
                 WHERE cv_id = c.id
                 ORDER BY created_at DESC
                 LIMIT 1
             ) r ON TRUE
             ORDER BY r.overall_score DESC, r.created_at DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("loading top_cvs")
    }
}

#[derive(Serialize)]
pub struct Owner {
    pub id: Uuid,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Serialize)]
pub struct CvWithOwner {
    pub id: Uuid,
    pub name: String,
    pub yaml: String,
    pub updated_at: DateTime<Utc>,
    pub owner: Owner,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seniority: Option<seniority::Report>,
}

#[derive(Serialize)]
pub struct OverviewStats {
    pub mine: ScopedStats,
    pub company: ScopedStats,
}

#[derive(Serialize)]
pub struct ScopedStats {
    pub total_cvs: u32,
    pub reviewed_cvs: u32,
    pub avg_score: Option<f64>,
    pub client_ready_count: u32,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CvOverviewItem {
    pub id: Uuid,
    pub name: String,
    pub updated_at: DateTime<Utc>,
    pub owner_id: Uuid,
    pub owner_name: Option<String>,
    pub label: Option<String>,
    pub latest_score: Option<i16>,
    pub latest_verdict: Option<String>,
    pub latest_review_at: Option<DateTime<Utc>>,
    pub seniority_score: Option<i16>,
    pub seniority_level: Option<String>,
}

