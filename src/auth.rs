// Keycloak OIDC authentication.
//
// At startup we read the three KEYCLOAK_* env vars; if any of them is missing
// the backend logs a warning and runs unauthenticated (same fallback shape as
// ANTHROPIC_API_KEY). When configured we fetch the realm's JWKS once at boot
// and validate every Bearer token on protected routes against it. Token
// rotation: Keycloak's signing keys change rarely; if they do, restart the
// server. (We can add periodic refresh later if it becomes a problem.)

use anyhow::{Context, Result, anyhow};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize)]
pub struct KeycloakConfig {
    pub url: String,
    pub realm: String,
    pub client_id: String,
}

impl KeycloakConfig {
    /// All three vars must be set; if any is missing or empty we treat it as
    /// "auth disabled" so local devs without Keycloak access can still work.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("KEYCLOAK_URL").ok()?;
        let realm = std::env::var("KEYCLOAK_REALM").ok()?;
        let client_id = std::env::var("KEYCLOAK_CLIENT_ID").ok()?;
        if url.trim().is_empty() || realm.trim().is_empty() || client_id.trim().is_empty() {
            return None;
        }
        Some(Self {
            url: url.trim_end_matches('/').to_string(),
            realm,
            client_id,
        })
    }

    pub fn issuer(&self) -> String {
        format!("{}/realms/{}", self.url, self.realm)
    }

    fn jwks_url(&self) -> String {
        format!("{}/protocol/openid-connect/certs", self.issuer())
    }
}

/// Subset of Keycloak's standard JWT claims that we care about.
#[derive(Clone, Debug, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub preferred_username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
}

/// Validates Bearer tokens against a Keycloak realm's JWKS.
#[derive(Clone)]
pub struct Authorizer {
    inner: Arc<AuthorizerInner>,
}

struct AuthorizerInner {
    jwks: JwkSet,
    issuer: String,
}

impl Authorizer {
    pub async fn from_keycloak(cfg: &KeycloakConfig) -> Result<Self> {
        let jwks: JwkSet = reqwest::get(cfg.jwks_url())
            .await
            .with_context(|| format!("fetching JWKS at {}", cfg.jwks_url()))?
            .error_for_status()
            .context("Keycloak JWKS endpoint returned non-2xx")?
            .json()
            .await
            .context("decoding JWKS JSON")?;
        Ok(Self {
            inner: Arc::new(AuthorizerInner {
                jwks,
                issuer: cfg.issuer(),
            }),
        })
    }

    fn validate(&self, token: &str) -> Result<Claims> {
        let header = decode_header(token).context("decoding JWT header")?;
        let kid = header
            .kid
            .ok_or_else(|| anyhow!("JWT has no `kid`; cannot match a signing key"))?;
        let jwk = self
            .inner
            .jwks
            .find(&kid)
            .ok_or_else(|| anyhow!("`kid` {kid} not present in Keycloak JWKS"))?;
        let key = DecodingKey::from_jwk(jwk).context("building decoding key from JWK")?;

        // Algorithm comes from the JWK; for Keycloak this is RS256 by default.
        let alg = header.alg;
        let mut validation = Validation::new(alg);
        validation.set_issuer(&[&self.inner.issuer]);
        validation.set_audience(&["account"]);

        let data = decode::<Claims>(token, &key, &validation).context("verifying JWT")?;
        Ok(data.claims)
    }
}

/// Tower middleware that requires a valid Bearer token. On success the parsed
/// `Claims` are inserted into the request's extensions for handlers that want
/// them; handlers that don't care just proceed.
pub async fn require_auth(
    State(authz): State<Authorizer>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = bearer_token(req.headers()).ok_or_else(|| {
        tracing::debug!("rejected: missing or malformed Authorization header");
        StatusCode::UNAUTHORIZED
    })?;
    let claims = authz.validate(token).map_err(|e| {
        tracing::debug!("rejected: JWT validation failed: {e:#}");
        StatusCode::UNAUTHORIZED
    })?;
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
