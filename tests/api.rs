use axum::body::Body;
use axum::http::{Request, StatusCode};
use cv_builder::{Db, api_router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
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
    api_router(Db { pool })
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

    for theme in ["cosmic", "luxe", "opera"] {
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
