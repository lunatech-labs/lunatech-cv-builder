// User identity and ownership.
//
// A `User` is whoever owns a CV. In production `User` is derived from the
// Keycloak `sub` claim that the auth middleware put on the request (so
// the user is whoever logged in). In dev mode (no Keycloak configured) we
// fall back to a fixed "dev local" user — the nil UUID — so every CV
// created on a dev machine ends up under that single shared identity and
// stays visible across restarts.
//
// `resolve_user` is the middleware that materialises the `User`, upserts
// the row in the `users` table (so we have email/name to display later if
// needed), and stashes the value as a request extension. Handlers then
// pull it out via `Extension<User>` and pass `user.id` to the DB layer,
// which scopes every query.

use crate::AppState;
use crate::auth::Claims;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use std::str::FromStr;
use uuid::Uuid;

/// Sentinel id for the unauthenticated dev user. Matches the row seeded by
/// migration `0004_users.sql`.
pub const DEV_USER_ID: Uuid = Uuid::nil();

/// Admin allow-list, read from the `ADMIN_EMAILS` env var (comma-separated,
/// case-insensitive). Any user whose Keycloak email matches gets the
/// `is_admin` flag and can edit / delete / review any CV regardless of
/// ownership. Empty / missing env var = no admins.
///
/// We re-parse the env on every call rather than caching: the cost is
/// negligible (a short CSV split) and it lets `cargo test` toggle the
/// allow-list per test without a global cache to invalidate.
fn is_admin_email(email: &str) -> bool {
    let needle = email.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    std::env::var("ADMIN_EMAILS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .any(|a| !a.is_empty() && a == needle)
}

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: Option<String>,
    pub name: Option<String>,
    pub is_admin: bool,
}

impl User {
    fn dev() -> Self {
        Self {
            id: DEV_USER_ID,
            email: Some("dev@local".into()),
            name: Some("Dev User".into()),
            // Local dev runs everything as a single fixed user; we don't
            // grant admin to that synthetic identity by default.
            is_admin: false,
        }
    }

    fn from_claims(claims: &Claims) -> Result<Self, String> {
        let id = Uuid::from_str(&claims.sub)
            .map_err(|e| format!("Keycloak `sub` is not a UUID: {e}"))?;
        let email = claims.email.clone();
        let is_admin = email.as_deref().map(is_admin_email).unwrap_or(false);
        Ok(Self {
            id,
            email,
            name: claims
                .name
                .clone()
                .or_else(|| claims.preferred_username.clone()),
            is_admin,
        })
    }
}

pub async fn resolve_user(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = match req.extensions().get::<Claims>().cloned() {
        Some(claims) => match User::from_claims(&claims) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("rejecting request: {e}");
                return Err(StatusCode::UNAUTHORIZED);
            }
        },
        None => User::dev(),
    };

    // Best-effort upsert — losing this row briefly doesn't matter, the
    // request can still proceed because the dev user is seeded by migration
    // and any authenticated user gets created on their first successful
    // upsert. We just log if it fails.
    if let Err(e) = state.db.upsert_user(&user).await {
        tracing::warn!("user upsert failed for {}: {e:#}", user.id);
    }

    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}
