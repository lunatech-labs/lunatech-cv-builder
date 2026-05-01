use anyhow::Result;
use cv_builder::{AnthropicConfig, AppState, Db, KeycloakConfig, app, auth};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://cvbuilder:cvbuilder@localhost:5433/cvbuilder".into());
    let db = Db::connect(&db_url).await?;

    let anthropic = AnthropicConfig::from_env_and_skill()?;
    if anthropic.is_some() {
        tracing::info!("ANTHROPIC_API_KEY found — POST /api/cvs/{{id}}/reviews enabled");
    } else {
        tracing::info!(
            "ANTHROPIC_API_KEY not set — POST /api/cvs/{{id}}/reviews will return 503"
        );
    }

    let keycloak = KeycloakConfig::from_env();
    let authorizer = match &keycloak {
        Some(cfg) => {
            tracing::info!(
                "Keycloak configured — issuer={} client_id={} (protected /api/* routes)",
                cfg.issuer(),
                cfg.client_id
            );
            Some(auth::Authorizer::from_keycloak(cfg).await?)
        }
        None => {
            tracing::warn!(
                "KEYCLOAK_URL/REALM/CLIENT_ID not all set — running unauthenticated (dev mode)"
            );
            None
        }
    };

    let state = AppState {
        db,
        anthropic,
        keycloak,
        batch_review: Arc::new(RwLock::new(None)),
    };

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{}", addr);
    axum::serve(listener, app(state, authorizer, "frontend")).await?;
    Ok(())
}
