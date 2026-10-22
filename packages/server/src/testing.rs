//! Helpers for tests.

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use axum::routing::get;
use axum::{Json, Router};
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, get_current_timestamp};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// The key ID of every fake issuer's key.
const KEY_ID: &str = "test";

/// An OIDC issuer on a local port that signs tokens with a fresh key.
pub(crate) struct FakeIssuer {
    /// The issuer URL.
    url: String,
    /// The signing key.
    key: EncodingKey,
    /// The server task.
    server: JoinHandle<()>,
}

impl FakeIssuer {
    /// Starts serving the discovery document and the key set.
    pub(crate) async fn start() -> Self {
        let pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .unwrap();
        let key = EncodingKey::from_ec_der(pkcs8.as_ref());
        let mut jwk = Jwk::from_encoding_key(&key, Algorithm::ES256).unwrap();
        jwk.common.key_id = Some(KEY_ID.to_owned());

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let discovery = json!({ "issuer": url, "jwks_uri": format!("{url}/jwks") });
        let keys = json!({ "keys": [jwk] });
        let app = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(async move || Json(discovery)),
            )
            .route("/jwks", get(async move || Json(keys)));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url, key, server }
    }

    /// Stops serving. Open connections stay until the client closes them.
    pub(crate) fn stop(&self) {
        self.server.abort();
    }

    /// The issuer URL.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Signs a token. The claims override defaults for `iss`, `sub` and
    /// `exp`.
    pub(crate) fn token(&self, claims: &Value) -> String {
        let mut all = json!({
            "iss": self.url,
            "sub": "repo:inomotech/test:ref:refs/heads/main",
            "exp": get_current_timestamp().saturating_add(600),
        });
        all.as_object_mut()
            .unwrap()
            .extend(claims.as_object().unwrap().clone());
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(KEY_ID.to_owned());
        encode(&header, &all, &self.key).unwrap()
    }
}
