use crate::AppState;
use crate::auth::KeycloakConfig;
use crate::cv_reviewer;
use crate::db::{CvRecord, CvSummary, Db};
use crate::pdf;
use crate::review_pdf;
use crate::users::User;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::Json;
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
    let id = db.create(user.id, &body.yaml, &name).await.map_err(err500)?;
    Ok(Json(CreateResponse { id }))
}

/// `GET /api/cvs/{id}` enriched with the latest persisted review (if any)
/// so the frontend's score badge is populated on first load — saves a
/// second round-trip.
#[derive(Serialize)]
pub struct CvWithReview {
    #[serde(flatten)]
    pub cv: CvRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_review: Option<cv_reviewer::Review>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_review_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_cv(
    State(db): State<Db>,
    Extension(user): Extension<User>,
    Path(id): Path<Uuid>,
) -> Result<Json<CvWithReview>, ApiError> {
    let rec = db
        .get(user.id, id)
        .await
        .map_err(err500)?
        .ok_or((StatusCode::NOT_FOUND, "cv not found".into()))?;
    let (latest_review, latest_review_at) = match db
        .latest_review(id, user.id)
        .await
        .map_err(err500)?
    {
        Some((r, ts)) => (Some(r), Some(ts)),
        None => (None, None),
    };
    Ok(Json(CvWithReview {
        cv: rec,
        latest_review,
        latest_review_at,
    }))
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
    let name = extract_name(&body.yaml);
    let updated = db
        .update(user.id, id, &body.yaml, &name)
        .await
        .map_err(err500)?;
    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "cv not found".into()))
    }
}

pub async fn delete_cv(
    State(db): State<Db>,
    Extension(user): Extension<User>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = db.delete(user.id, id).await.map_err(err500)?;
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
    Extension(user): Extension<User>,
    Path(id): Path<Uuid>,
    Query(q): Query<PdfQuery>,
) -> Result<Response, ApiError> {
    let rec = db
        .get(user.id, id)
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
/// resulting review, and returns it. The CV must belong to the calling
/// user — anything else returns 404 (we never leak other users' CVs).
/// Returns 503 if `ANTHROPIC_API_KEY` isn't set.
pub async fn review_cv(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(cv_id): Path<Uuid>,
) -> Result<Json<cv_reviewer::Review>, ApiError> {
    let cfg = state.anthropic.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "ANTHROPIC_API_KEY is not set on the server".to_string(),
    ))?;

    let rec = state
        .db
        .get(user.id, cv_id)
        .await
        .map_err(err500)?
        .ok_or((StatusCode::NOT_FOUND, "cv not found".into()))?;

    let review = cv_reviewer::review(cfg, &rec.yaml).await.map_err(err500)?;

    state
        .db
        .save_review(cv_id, user.id, &review, &rec.yaml)
        .await
        .map_err(err500)?;
    Ok(Json(review))
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

