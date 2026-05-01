pub mod auth;
pub mod batch_review;
pub mod cv_reviewer;
pub mod db;
pub mod handlers;
pub mod pdf;
pub mod review_pdf;
pub mod seniority;
pub mod users;

pub use auth::KeycloakConfig;
pub use batch_review::SharedBatchState;
pub use cv_reviewer::AnthropicConfig;
pub use db::Db;

use axum::Router;
use axum::extract::FromRef;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// State shared across all routes. Both `anthropic` and `keycloak` are
/// optional — when either isn't configured we degrade gracefully instead of
/// refusing to boot, so devs without those credentials can still work.
/// `FromRef<AppState> for Db` lets existing handlers keep using `State<Db>`.
///
/// `batch_review` carries the in-memory state of the latest admin-triggered
/// "Review all CVs" job. `None` until the first run, then `Some(job)`
/// updated in place by the worker. Lost on server restart by design — see
/// `batch_review.rs`.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub anthropic: Option<AnthropicConfig>,
    pub keycloak: Option<KeycloakConfig>,
    pub batch_review: SharedBatchState,
}

impl FromRef<AppState> for Db {
    fn from_ref(s: &AppState) -> Db {
        s.db.clone()
    }
}

/// Builds the full `/api/*` router. `/api/config` is always public so the
/// frontend can read it before keycloak-js boots; everything else gets the
/// user resolver (always-on) and, when an authorizer is supplied, the JWT
/// layer in front of it.
pub fn api_router(state: AppState, authorizer: Option<auth::Authorizer>) -> Router {
    let public = Router::new()
        .route("/config", get(handlers::get_config))
        .with_state(state.clone());

    let mut protected = Router::new()
        .route("/overview", get(handlers::get_overview))
        .route("/cvs", get(handlers::list_cvs).post(handlers::create_cv))
        .route(
            "/cvs/{id}",
            get(handlers::get_cv)
                .put(handlers::update_cv)
                .delete(handlers::delete_cv),
        )
        .route("/cvs/{id}/pdf", get(handlers::pdf_cv))
        .route("/cvs/{id}/reviews", post(handlers::review_cv))
        .route(
            "/batch-reviews",
            post(handlers::batch_reviews_start).get(handlers::batch_reviews_events),
        )
        .route("/review/pdf", post(handlers::review_pdf_handler))
        .with_state(state.clone());

    // Always-on: turns the JWT claims (when the auth layer ran) or the dev
    // fallback into a `User` extension that handlers extract.
    protected = protected.layer(from_fn_with_state(state, users::resolve_user));

    // When auth is on, validate the JWT *before* the user resolver runs.
    if let Some(authz) = authorizer {
        protected = protected.layer(from_fn_with_state(authz, auth::require_auth));
    }

    public.merge(protected)
}

pub fn app(
    state: AppState,
    authorizer: Option<auth::Authorizer>,
    frontend_dir: &str,
) -> Router {
    Router::new()
        .nest("/api", api_router(state, authorizer))
        .nest_service("/assets", ServeDir::new("assets"))
        .fallback_service(
            ServeDir::new(frontend_dir)
                .not_found_service(ServeFile::new(format!("{frontend_dir}/index.html"))),
        )
        .layer(TraceLayer::new_for_http())
}
