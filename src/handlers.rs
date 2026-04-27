use crate::db::{CvRecord, CvSummary, Db};
use crate::pdf;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

type ApiError = (StatusCode, String);

fn err500<E: std::fmt::Display>(e: E) -> ApiError {
    tracing::error!("internal error: {}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
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

pub async fn list_cvs(State(db): State<Db>) -> Result<Json<Vec<CvSummary>>, ApiError> {
    let rows = db.list().await.map_err(err500)?;
    Ok(Json(rows))
}

pub async fn create_cv(
    State(db): State<Db>,
    Json(body): Json<YamlBody>,
) -> Result<Json<CreateResponse>, ApiError> {
    if body.yaml.trim().is_empty() {
        return Err(err400("yaml is empty"));
    }
    let name = extract_name(&body.yaml);
    let id = db.create(&body.yaml, &name).await.map_err(err500)?;
    Ok(Json(CreateResponse { id }))
}

pub async fn get_cv(
    State(db): State<Db>,
    Path(id): Path<Uuid>,
) -> Result<Json<CvRecord>, ApiError> {
    let rec = db
        .get(id)
        .await
        .map_err(err500)?
        .ok_or((StatusCode::NOT_FOUND, "cv not found".into()))?;
    Ok(Json(rec))
}

pub async fn update_cv(
    State(db): State<Db>,
    Path(id): Path<Uuid>,
    Json(body): Json<YamlBody>,
) -> Result<StatusCode, ApiError> {
    if body.yaml.trim().is_empty() {
        return Err(err400("yaml is empty"));
    }
    let name = extract_name(&body.yaml);
    let updated = db.update(id, &body.yaml, &name).await.map_err(err500)?;
    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "cv not found".into()))
    }
}

pub async fn delete_cv(
    State(db): State<Db>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = db.delete(id).await.map_err(err500)?;
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
        .get(id)
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

