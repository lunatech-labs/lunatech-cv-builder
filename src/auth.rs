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

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    // Regression guard for the jsonwebtoken 10 upgrade: the crate no longer
    // enables a crypto backend by default, and the missing-provider error only
    // surfaces at sign/verify time (not at compile time). A token round-trip
    // exercises that path, so this fails loudly if the `rust_crypto` (or
    // `aws_lc_rs`) feature is ever dropped from Cargo.toml again.
    #[test]
    fn jwt_round_trip_has_a_crypto_provider() {
        let secret = b"regression-test-secret";
        let claims = serde_json::json!({
            "sub": "user-123",
            "email": "user@example.com",
            "exp": 9_999_999_999u64,
        });
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret),
        )
        .expect("encoding a JWT must not panic on a missing crypto provider");

        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_aud = false;
        let data = decode::<Claims>(&token, &DecodingKey::from_secret(secret), &validation)
            .expect("decoding a JWT must not panic on a missing crypto provider");

        assert_eq!(data.claims.sub, "user-123");
    }

    // Exercises the real Keycloak algorithm (RS256) end to end: sign with an RSA
    // private key, verify with the public key. This proves the chosen crypto
    // backend actually performs RSA verification — the HS256 test above only
    // covers HMAC, which is a different code path inside the provider.
    #[test]
    fn rs256_sign_and_verify_round_trip() {
        let priv_pem = include_bytes!("../tests/fixtures/jwt_test_rsa_priv.pem");
        let pub_pem = include_bytes!("../tests/fixtures/jwt_test_rsa_pub.pem");

        let issuer = "https://keycloak.example.com/realms/lunatech";
        let claims = serde_json::json!({
            "sub": "abc-123",
            "name": "Ada Lovelace",
            "email": "ada@example.com",
            "iss": issuer,
            "aud": "account",
            "exp": 9_999_999_999u64,
        });
        let token = encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &EncodingKey::from_rsa_pem(priv_pem).expect("loading RSA private key"),
        )
        .expect("signing an RS256 JWT must not panic on a missing crypto provider");

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[issuer]);
        validation.set_audience(&["account"]);
        let data = decode::<Claims>(
            &token,
            &DecodingKey::from_rsa_pem(pub_pem).expect("loading RSA public key"),
            &validation,
        )
        .expect("verifying an RS256 JWT must succeed with the rust_crypto backend");

        assert_eq!(data.claims.sub, "abc-123");
        assert_eq!(data.claims.name.as_deref(), Some("Ada Lovelace"));
    }
}
