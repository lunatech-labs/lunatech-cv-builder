use anyhow::Result;
use cv_builder::{AnthropicConfig, AppState, Db, KeycloakConfig, app, auth};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

/// Ceiling on the migration retry backoff. Caps how long a recovered database
/// can sit unnoticed while still keeping the log quiet during a long outage.
const MAX_MIGRATE_BACKOFF: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://cvbuilder:cvbuilder@localhost:5433/cvbuilder".into());
    // Lazy: builds the pool without dialling Postgres, so an unreachable
    // database can't stop us reaching the `bind` below. Migrations run in the
    // background once we're listening — see the spawn after binding.
    let db = Db::lazy(&db_url)?;

    let anthropic = AnthropicConfig::from_env_and_skill()?;
    if anthropic.is_some() {
        tracing::info!("ANTHROPIC_API_KEY found — POST /api/cvs/{{id}}/reviews enabled");
    } else {
        tracing::info!(
            "ANTHROPIC_API_KEY not set — POST /api/cvs/{{id}}/reviews will return 503"
        );
    }

    let keycloak = KeycloakConfig::from_env();
    // Gate the dev fixture seed on the same two conditions as before — no
    // Keycloak *and* an explicit `DEV_SEED_FIXTURES=1`. Decided here, applied
    // in the background task below, because seeding needs a migrated schema.
    let seed_fixtures =
        keycloak.is_none() && std::env::var("DEV_SEED_FIXTURES").as_deref() == Ok("1");
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
            if !seed_fixtures {
                tracing::info!(
                    "DEV_SEED_FIXTURES not set — skipping fixture seed (set =1 to enable)"
                );
            }
            None
        }
    };

    let state = AppState {
        db,
        anthropic,
        keycloak,
        batch_review: Arc::new(RwLock::new(None)),
    };

    // Bind *before* touching the database. The listener existing is what
    // stops the platform's proxy from 504-ing every route (including the
    // static frontend and `/api/config`) while Postgres is unreachable.
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{}", addr);

    // Migrations run behind the listener and retry indefinitely. A database
    // that is slow to start, or briefly unreachable, now resolves itself
    // instead of killing the process — and every attempt is logged, so the
    // failure is visible from the logs and from `/api/health` rather than
    // being a silent hang.
    let migrate_db = state.db.clone();
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            match migrate_db.migrate().await {
                Ok(()) => {
                    tracing::info!("database ready — migrations applied");
                    if seed_fixtures && let Err(e) = migrate_db
                        .seed_fixtures_if_empty("assets/fixtures")
                        .await
                    {
                        tracing::error!("fixture seeding failed: {e:#}");
                    }
                    return;
                }
                Err(e) => {
                    tracing::error!(
                        "database not ready ({e:#}) — retrying in {}s; \
                         /api/health reports the current state",
                        backoff.as_secs()
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_MIGRATE_BACKOFF);
                }
            }
        }
    });

    axum::serve(listener, app(state, authorizer, "frontend")).await?;
    Ok(())
}
