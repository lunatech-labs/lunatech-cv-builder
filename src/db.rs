use crate::users::User;
use anyhow::Result;
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
}
