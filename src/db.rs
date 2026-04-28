use crate::cv_reviewer::Review;
use crate::users::User;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
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
        Ok(Self { pool })
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
        let rows = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
            "SELECT id, name, updated_at FROM cvs WHERE user_id = $1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, updated_at)| CvSummary { id, name, updated_at })
            .collect())
    }

    pub async fn create(&self, user_id: Uuid, yaml: &str, name: &str) -> Result<Uuid> {
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO cvs (user_id, name, yaml) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(user_id)
        .bind(name)
        .bind(yaml)
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

    pub async fn update(&self, user_id: Uuid, id: Uuid, yaml: &str, name: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE cvs SET yaml = $1, name = $2, updated_at = NOW()
             WHERE id = $3 AND user_id = $4",
        )
        .bind(yaml)
        .bind(name)
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, user_id: Uuid, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM cvs WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
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

    /// Returns the most recent review for a CV (scoped to the calling user
    /// so we never expose another user's review even if the cv_id leaks),
    /// alongside the timestamp the review was run.
    pub async fn latest_review(
        &self,
        cv_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<(Review, DateTime<Utc>)>> {
        let row: Option<(serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
            "SELECT payload, created_at FROM reviews
             WHERE cv_id = $1 AND user_id = $2
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(cv_id)
        .bind(user_id)
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
}
