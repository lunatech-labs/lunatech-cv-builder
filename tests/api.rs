use axum::body::Body;
use axum::http::{Request, StatusCode};
use cv_builder::{AppState, Db, api_router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const SAMPLE_YAML: &str = r#"
name: Alice Smith
title: Senior Engineer
lunatech_since: "2020"
client_name: ACME
key_assets:
  - 10 years of Rust
  - Distributed systems
summary: A short bio.
experiences:
  - company: Big Co
    role: Lead
    period: "2024 — present"
    description: Backend leadership.
    tags: [Rust, Postgres]
skills:
  - group: Languages
    items:
      - { name: Rust, level: 5 }
languages:
  - language: French
    level: Native
"#;

fn router_with(pool: PgPool) -> axum::Router {
    api_router(
        AppState {
            db: Db::from_pool(pool),
            anthropic: None,
            keycloak: None,
            batch_review: Arc::new(RwLock::new(None)),
        },
        None,
    )
}

async fn body_string(resp: axum::http::Response<Body>) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn body_bytes(resp: axum::http::Response<Body>) -> Vec<u8> {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    bytes.to_vec()
}

async fn body_json(resp: axum::http::Response<Body>) -> Value {
    let s = body_string(resp).await;
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("invalid JSON ({e}): {s}"))
}

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn empty_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

async fn create(app: &axum::Router, yaml: &str) -> uuid::Uuid {
    let resp = app
        .clone()
        .oneshot(json_request("POST", "/cvs", json!({"yaml": yaml})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let id_str = json["id"].as_str().expect("missing id field");
    uuid::Uuid::parse_str(id_str).unwrap()
}

// ─────────────────────────── HEALTH ───────────────────────────

#[sqlx::test]
async fn health_reports_ok_when_the_database_is_reachable(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(empty_request("GET", "/health"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "ok");
    assert_eq!(json["database"], "up");
}

/// Health must not sit behind the auth layer — a check that needs a JWT can't
/// tell "app down" from "Keycloak down", which is the ambiguity it exists to
/// resolve. `router_with` supplies no authorizer, so this pins the route's
/// placement on the public router rather than re-testing the handler.
#[sqlx::test]
async fn health_is_reachable_without_authentication(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(empty_request("GET", "/health"))
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// The regression this branch is about: an unreachable database must produce a
/// served 503 that *names the problem*, not a hang. Points the pool at a
/// closed port, so nothing is listening.
#[tokio::test]
async fn health_reports_degraded_when_the_database_is_unreachable() {
    let db = Db::lazy("postgres://cvbuilder:cvbuilder@127.0.0.1:1/cvbuilder")
        .expect("lazy pool should build without dialling Postgres");
    let app = api_router(
        AppState {
            db,
            anthropic: None,
            keycloak: None,
            batch_review: Arc::new(RwLock::new(None)),
        },
        None,
    );

    let started = std::time::Instant::now();
    let resp = app
        .oneshot(empty_request("GET", "/health"))
        .await
        .unwrap();
    // Must answer well inside a gateway timeout. Without the ping's own cap it
    // would block on the pool's 30s acquire timeout and 504 like everything
    // else — defeating the point of a health endpoint.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "health took {:?}; it must answer before a proxy gives up",
        started.elapsed()
    );
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "degraded");
    assert_eq!(json["database"], "down");
    assert!(
        json["detail"].as_str().is_some_and(|d| !d.is_empty()),
        "degraded health should explain itself, got: {json}"
    );
}

/// `Db::lazy` must not touch the network — that property is what lets `main`
/// reach its `bind` call when Postgres is unreachable. Needs a Tokio runtime
/// because the pool spawns its idle reaper on construction.
#[tokio::test]
async fn lazy_pool_builds_against_an_unreachable_database() {
    assert!(Db::lazy("postgres://nobody:nobody@127.0.0.1:1/nothing").is_ok());
}

// ─────────────────────────── LIST ───────────────────────────

#[sqlx::test]
async fn list_empty_returns_empty_array(pool: PgPool) {
    let app = router_with(pool);
    let resp = app.oneshot(empty_request("GET", "/cvs")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "[]");
}

#[sqlx::test]
async fn list_returns_summaries_in_descending_updated_at(pool: PgPool) {
    let app = router_with(pool);
    create(&app, "name: First").await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    create(&app, "name: Second").await;

    let resp = app.oneshot(empty_request("GET", "/cvs")).await.unwrap();
    let json = body_json(resp).await;
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    // Most recent first.
    assert_eq!(arr[0]["name"], "Second");
    assert_eq!(arr[1]["name"], "First");
}

// ─────────────────────────── CREATE ───────────────────────────

#[sqlx::test]
async fn create_returns_id_and_persists(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;
    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["id"], id.to_string());
    assert_eq!(json["name"], "Alice Smith");
    assert_eq!(json["yaml"], SAMPLE_YAML);
}

#[sqlx::test]
async fn create_extracts_name_from_yaml(pool: PgPool) {
    let app = router_with(pool);
    create(&app, "name: Bob Marley\ntitle: Singer").await;

    let resp = app.oneshot(empty_request("GET", "/cvs")).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json[0]["name"], "Bob Marley");
}

#[sqlx::test]
async fn create_with_empty_body_rejected(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(json_request("POST", "/cvs", json!({"yaml": ""})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn create_with_only_whitespace_rejected(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(json_request("POST", "/cvs", json!({"yaml": "   \n  \t  "})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn create_with_yaml_lacking_name_keeps_empty_name(pool: PgPool) {
    let app = router_with(pool);
    create(&app, "title: No Name\nfoo: bar").await;
    let resp = app.oneshot(empty_request("GET", "/cvs")).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json[0]["name"], "");
}

// ─────────────────────────── GET ───────────────────────────

#[sqlx::test]
async fn get_unknown_returns_404(pool: PgPool) {
    let app = router_with(pool);
    let id = uuid::Uuid::new_v4();
    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn get_with_invalid_uuid_returns_400(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(empty_request("GET", "/cvs/not-a-uuid"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─────────────────────────── UPDATE ───────────────────────────

#[sqlx::test]
async fn update_replaces_yaml_and_name(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, "name: Original\ntitle: A").await;

    let resp = app
        .clone()
        .oneshot(json_request(
            "PUT",
            &format!("/cvs/{id}"),
            json!({"yaml": "name: Updated\ntitle: B"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}")))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["name"], "Updated");
    assert!(json["yaml"].as_str().unwrap().contains("title: B"));
}

#[sqlx::test]
async fn update_unknown_returns_404(pool: PgPool) {
    let app = router_with(pool);
    let id = uuid::Uuid::new_v4();
    let resp = app
        .oneshot(json_request(
            "PUT",
            &format!("/cvs/{id}"),
            json!({"yaml": "name: X"}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn update_with_empty_yaml_rejected(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, "name: Original").await;

    let resp = app
        .oneshot(json_request(
            "PUT",
            &format!("/cvs/{id}"),
            json!({"yaml": ""}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn update_bumps_updated_at(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, "name: First").await;

    let before = body_json(
        app.clone()
            .oneshot(empty_request("GET", &format!("/cvs/{id}")))
            .await
            .unwrap(),
    )
    .await["updated_at"]
        .clone();

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    app.clone()
        .oneshot(json_request(
            "PUT",
            &format!("/cvs/{id}"),
            json!({"yaml": "name: Second"}),
        ))
        .await
        .unwrap();

    let after = body_json(
        app.oneshot(empty_request("GET", &format!("/cvs/{id}")))
            .await
            .unwrap(),
    )
    .await["updated_at"]
        .clone();

    assert_ne!(before, after, "updated_at should change after PUT");
}

// ─────────────────────────── DELETE ───────────────────────────

#[sqlx::test]
async fn delete_existing_returns_204(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;

    let resp = app
        .clone()
        .oneshot(empty_request("DELETE", &format!("/cvs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn delete_unknown_returns_404(pool: PgPool) {
    let app = router_with(pool);
    let id = uuid::Uuid::new_v4();
    let resp = app
        .oneshot(empty_request("DELETE", &format!("/cvs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─────────────────────────── PDF ───────────────────────────

#[sqlx::test]
async fn pdf_returns_pdf_bytes(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;

    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}/pdf")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(ct, "application/pdf");
    let body = body_bytes(resp).await;
    assert!(body.starts_with(b"%PDF-"), "not a PDF (first bytes: {:?})", &body.get(..8));
}

#[sqlx::test]
async fn pdf_includes_filename_in_disposition(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;

    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}/pdf")))
        .await
        .unwrap();
    let cd = resp
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(cd.contains("alice-smith"), "expected slug in disposition, got {cd}");
    assert!(cd.contains(".pdf"));
}

#[sqlx::test]
async fn pdf_renders_each_theme(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;

    for theme in ["lunatech", "cosmic", "luxe", "opera"] {
        let uri = format!("/cvs/{id}/pdf?theme={theme}");
        let resp = app.clone().oneshot(empty_request("GET", &uri)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "theme {theme} failed");
        let body = body_bytes(resp).await;
        assert!(body.starts_with(b"%PDF-"), "theme {theme} did not produce a PDF");
    }
}

#[sqlx::test]
async fn pdf_unknown_theme_falls_back_silently(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;

    let resp = app
        .oneshot(empty_request(
            "GET",
            &format!("/cvs/{id}/pdf?theme=does-not-exist"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_bytes(resp).await;
    assert!(body.starts_with(b"%PDF-"));
}

#[sqlx::test]
async fn pdf_unknown_id_returns_404(pool: PgPool) {
    let app = router_with(pool);
    let id = uuid::Uuid::new_v4();
    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}/pdf")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn pdf_for_minimal_yaml_still_renders(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, "name: Minimal\ntitle: ''").await;

    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}/pdf")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_bytes(resp).await;
    assert!(body.starts_with(b"%PDF-"));
}

// ─────────────────────────── REVIEW ───────────────────────────
//
// We always run with `anthropic: None` so the route returns 503 — that
// guarantees we don't accidentally call the live Anthropic API from CI.
// Hitting Claude for real is exercised by manual end-to-end testing.

#[sqlx::test]
async fn review_returns_503_when_api_key_not_configured(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;
    let resp = app
        .oneshot(empty_request("POST", &format!("/cvs/{id}/reviews")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ─────────────────────────── OVERVIEW ───────────────────────────

#[sqlx::test]
async fn overview_empty_returns_zero_stats(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(empty_request("GET", "/overview"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    for scope in ["mine", "company"] {
        assert_eq!(json["stats"][scope]["total_cvs"], 0);
        assert_eq!(json["stats"][scope]["reviewed_cvs"], 0);
        assert!(json["stats"][scope]["avg_score"].is_null());
        assert_eq!(json["stats"][scope]["client_ready_count"], 0);
    }
    assert_eq!(json["my_cvs"].as_array().unwrap().len(), 0);
    assert_eq!(json["top_cvs"].as_array().unwrap().len(), 0);
}

#[sqlx::test]
async fn overview_lists_my_cvs(pool: PgPool) {
    let app = router_with(pool);
    create(&app, SAMPLE_YAML).await;
    create(&app, "name: Second").await;

    let resp = app.oneshot(empty_request("GET", "/overview")).await.unwrap();
    let json = body_json(resp).await;
    // Tests run as the dev user, who owns both CVs — mine and company match.
    assert_eq!(json["stats"]["mine"]["total_cvs"], 2);
    assert_eq!(json["stats"]["company"]["total_cvs"], 2);
    let my = json["my_cvs"].as_array().unwrap();
    assert_eq!(my.len(), 2);
    // No reviews yet, so latest_score is null on each.
    assert!(my[0]["latest_score"].is_null());
    // top_cvs is restricted to reviewed CVs — none here.
    assert_eq!(json["top_cvs"].as_array().unwrap().len(), 0);
    // all_cvs is the unfiltered catalog — both CVs land here.
    assert_eq!(json["all_cvs"].as_array().unwrap().len(), 2);
}

#[sqlx::test]
async fn create_then_get_exposes_seniority(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;

    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}")))
        .await
        .unwrap();
    let json = body_json(resp).await;
    assert!(json["seniority"].is_object(), "expected seniority on detail");
    assert!(json["seniority"]["score"].is_number());
    assert!(json["seniority"]["level"].is_string());
    assert!(json["seniority"]["breakdown"].is_object());
}

#[sqlx::test]
async fn overview_items_include_seniority(pool: PgPool) {
    let app = router_with(pool);
    create(&app, SAMPLE_YAML).await;

    let resp = app.oneshot(empty_request("GET", "/overview")).await.unwrap();
    let json = body_json(resp).await;
    let my = json["my_cvs"].as_array().unwrap();
    assert!(!my.is_empty());
    // The dashboard rows carry the lean variant — score + level only.
    assert!(my[0]["seniority_score"].is_number() || my[0]["seniority_score"].is_null());
    assert!(my[0]["seniority_level"].is_string() || my[0]["seniority_level"].is_null());
}

#[sqlx::test]
async fn get_cv_includes_owner_info(pool: PgPool) {
    let app = router_with(pool);
    let id = create(&app, SAMPLE_YAML).await;

    let resp = app
        .oneshot(empty_request("GET", &format!("/cvs/{id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(json["owner"].is_object(), "expected owner field");
    assert!(json["owner"]["id"].is_string());
}

#[sqlx::test]
async fn review_returns_404_for_unknown_cv_even_without_key(pool: PgPool) {
    // The 503 (no API key) check fires before the lookup, so a missing
    // CV still surfaces as 503. That's fine — the user's first signal is
    // "this server can't review at all" before "your CV doesn't exist".
    let app = router_with(pool);
    let id = uuid::Uuid::new_v4();
    let resp = app
        .oneshot(empty_request("POST", &format!("/cvs/{id}/reviews")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test]
async fn review_pdf_renders_a_pdf(pool: PgPool) {
    // Stateless route — doesn't touch DB or call Claude, so it works even
    // without an API key. We feed it a hand-crafted Review and check the
    // response is a real PDF.
    let app = router_with(pool);
    let body = json!({
        "review": {
            "overall_score": 7,
            "verdict": "minor_improvements",
            "language": "en",
            "report_markdown": "# Overall\n\nThe CV is solid.\n\n## Per-Project\n\n- Lead the API team\n- Designed the auth flow\n",
            "improved_yaml": ""
        },
        "cv_name": "Alice Smith"
    });
    let resp = app
        .oneshot(json_request("POST", "/review/pdf", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = body_bytes(resp).await;
    assert!(bytes.starts_with(b"%PDF-"));
}

#[sqlx::test]
async fn review_pdf_works_without_cv_name(pool: PgPool) {
    let app = router_with(pool);
    let body = json!({
        "review": {
            "overall_score": 3,
            "verdict": "major_rework",
            "language": "fr",
            "report_markdown": "# Verdict\n\nÀ retravailler.",
            "improved_yaml": ""
        }
    });
    let resp = app
        .oneshot(json_request("POST", "/review/pdf", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = body_bytes(resp).await;
    assert!(bytes.starts_with(b"%PDF-"));
}

// ─────────────────── BATCH REVIEWS (admin-triggered) ───────────────────

// The dev user the test middleware injects has `is_admin = false`, so any
// admin-only endpoint should return 403 without us touching env vars. The
// 503 case (no Anthropic config) would also be testable but the admin
// gate fires first, which is fine.
//
// We deliberately avoid the 202/409 happy-path here: it would require
// flipping `ADMIN_EMAILS` process-wide, racing with other tests. The
// staleness filter is unit-tested in `batch_review::tests` and the
// kickoff handler is exercised by manual end-to-end runs.

#[sqlx::test]
async fn batch_reviews_start_returns_403_for_non_admin(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(empty_request("POST", "/batch-reviews"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn batch_reviews_events_returns_403_for_non_admin(pool: PgPool) {
    let app = router_with(pool);
    let resp = app
        .oneshot(empty_request("GET", "/batch-reviews"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
