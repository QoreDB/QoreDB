// SPDX-License-Identifier: BUSL-1.1

//! OIDC (OpenID Connect) Authorization Code + PKCE flow.
//!
//! Implemented directly over `reqwest` (rustls) + `jsonwebtoken` JWKS validation
//! rather than a full RP crate, to keep the dependency surface rustls-only
//! (Docker-friendly) and the id_token verification on a vetted library. The IdP
//! configuration comes from the environment (it is server config, not a user
//! credential); SSO is enabled only when all four variables are set.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const PENDING_TTL: Duration = Duration::from_secs(600);

pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OidcConfig {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            issuer: non_empty("QORE_OIDC_ISSUER")?,
            client_id: non_empty("QORE_OIDC_CLIENT_ID")?,
            client_secret: non_empty("QORE_OIDC_CLIENT_SECRET")?,
            redirect_uri: non_empty("QORE_OIDC_REDIRECT_URI")?,
        })
    }
}

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[derive(Deserialize)]
struct Discovery {
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    issuer: String,
}

struct Pending {
    verifier: String,
    nonce: String,
    created: Instant,
}

pub struct OidcProvider {
    config: OidcConfig,
    http: reqwest::Client,
    meta: Discovery,
    pending: Mutex<HashMap<String, Pending>>,
}

impl OidcProvider {
    pub async fn discover(config: OidcConfig) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| e.to_string())?;
        let url = format!(
            "{}/.well-known/openid-configuration",
            config.issuer.trim_end_matches('/')
        );
        let meta: Discovery = http
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self {
            config,
            http,
            meta,
            pending: Mutex::new(HashMap::new()),
        })
    }

    /// Builds the IdP authorization URL and stores the PKCE verifier + nonce
    /// keyed by the CSRF `state` for the callback to consume.
    pub fn start(&self) -> Result<String, String> {
        let verifier = random_token(32);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = random_token(24);
        let nonce = random_token(24);

        {
            let mut pending = self.pending.lock().map_err(|_| "lock poisoned")?;
            let now = Instant::now();
            pending.retain(|_, p| now.duration_since(p.created) < PENDING_TTL);
            pending.insert(
                state.clone(),
                Pending {
                    verifier,
                    nonce: nonce.clone(),
                    created: now,
                },
            );
        }

        let url = reqwest::Url::parse_with_params(
            &self.meta.authorization_endpoint,
            &[
                ("response_type", "code"),
                ("client_id", self.config.client_id.as_str()),
                ("redirect_uri", self.config.redirect_uri.as_str()),
                ("scope", "openid email profile"),
                ("state", state.as_str()),
                ("nonce", nonce.as_str()),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(url.to_string())
    }

    /// Exchanges the authorization code, validates the id_token against the
    /// IdP JWKS (signature, issuer, audience, expiry) and the stored nonce, and
    /// returns the verified email.
    pub async fn callback(&self, code: &str, state: &str) -> Result<String, String> {
        let pending = {
            let mut p = self.pending.lock().map_err(|_| "lock poisoned")?;
            p.remove(state).ok_or("unknown or expired login state")?
        };
        if Instant::now().duration_since(pending.created) > PENDING_TTL {
            return Err("login attempt expired".to_string());
        }

        #[derive(Deserialize)]
        struct TokenResp {
            id_token: String,
        }
        let token: TokenResp = self
            .http
            .post(&self.meta.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.config.redirect_uri.as_str()),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("code_verifier", pending.verifier.as_str()),
            ])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        let jwks: JwkSet = self
            .http
            .get(&self.meta.jwks_uri)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        let header = decode_header(&token.id_token).map_err(|e| e.to_string())?;
        let kid = header.kid.ok_or("id_token missing kid")?;
        let jwk = jwks.find(&kid).ok_or("no matching JWKS key")?;
        let key = DecodingKey::from_jwk(jwk).map_err(|e| e.to_string())?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[self.meta.issuer.as_str()]);
        validation.set_audience(&[self.config.client_id.as_str()]);

        #[derive(Deserialize)]
        struct IdClaims {
            email: Option<String>,
            nonce: Option<String>,
        }
        let data =
            decode::<IdClaims>(&token.id_token, &key, &validation).map_err(|e| e.to_string())?;

        if data.claims.nonce.as_deref() != Some(pending.nonce.as_str()) {
            return Err("nonce mismatch".to_string());
        }
        data.claims
            .email
            .filter(|e| !e.is_empty())
            .ok_or_else(|| "id_token has no email claim".to_string())
    }
}

pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}
