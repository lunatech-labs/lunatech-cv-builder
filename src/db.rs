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

    pub async fn list(&self) -> Result<Vec<CvSummary>> {
        let rows = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
            "SELECT id, name, updated_at FROM cvs ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, updated_at)| CvSummary { id, name, updated_at })
            .collect())
    }

    pub async fn create(&self, yaml: &str, name: &str) -> Result<Uuid> {
        let row: (Uuid,) = sqlx::query_as("INSERT INTO cvs (name, yaml) VALUES ($1, $2) RETURNING id")
            .bind(name)
            .bind(yaml)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<CvRecord>> {
        let row = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
            "SELECT id, name, yaml, updated_at FROM cvs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, name, yaml, updated_at)| CvRecord {
            id,
            name,
            yaml,
            updated_at,
        }))
    }

    pub async fn update(&self, id: Uuid, yaml: &str, name: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE cvs SET yaml = $1, name = $2, updated_at = NOW() WHERE id = $3",
        )
        .bind(yaml)
        .bind(name)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM cvs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
