pub mod db;
pub mod handlers;
pub mod pdf;

pub use db::Db;

use axum::Router;
use axum::routing::get;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub fn api_router(db: Db) -> Router {
    Router::new()
        .route("/cvs", get(handlers::list_cvs).post(handlers::create_cv))
        .route(
            "/cvs/{id}",
            get(handlers::get_cv)
                .put(handlers::update_cv)
                .delete(handlers::delete_cv),
        )
        .route("/cvs/{id}/pdf", get(handlers::pdf_cv))
        .with_state(db)
}

pub fn app(db: Db, frontend_dir: &str) -> Router {
    Router::new()
        .nest("/api", api_router(db))
        .fallback_service(
            ServeDir::new(frontend_dir)
                .not_found_service(ServeFile::new(format!("{frontend_dir}/index.html"))),
        )
        .layer(TraceLayer::new_for_http())
}
