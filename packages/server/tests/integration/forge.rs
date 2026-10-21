//! A forge that issues OIDC ID tokens to jobs, like GitHub or Forgejo.

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::routing::get;
use axum::{Json, Router};
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, get_current_timestamp};
use serde_json::json;
use tokio::net::TcpListener;

/// The token that jobs present to request ID tokens.
pub(crate) const RUNNER_TOKEN: &str = "runner-token";

/// The key ID of the forge's key.
const KEY_ID: &str = "forge";

/// A forge on a local port.
pub(crate) struct Forge {
    /// The issuer URL.
    url: String,
}

impl Forge {
    /// Starts serving discovery, keys and ID tokens for a pull request job
    /// of `inomotech/test`.
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
        let issuer = url.clone();
        let mint = async move |headers: HeaderMap, uri: Uri| {
            let authorized = headers
                .get("authorization")
                .is_some_and(|value| value == &format!("Bearer {RUNNER_TOKEN}"));
            if !authorized {
                return Err(StatusCode::UNAUTHORIZED);
            }
            let audience = uri
                .query()
                .unwrap_or_default()
                .split('&')
                .find_map(|pair| pair.strip_prefix("audience="))
                .unwrap_or_default()
                .to_owned();
            let claims = json!({
                "iss": issuer,
                "aud": audience,
                "sub": "repo:inomotech/test:pull_request",
                "repository_owner": "inomotech",
                "ref": "refs/pull/1/merge",
                "exp": get_current_timestamp().saturating_add(600),
            });
            let mut header = Header::new(Algorithm::ES256);
            header.kid = Some(KEY_ID.to_owned());
            let token = encode(&header, &claims, &key).unwrap();
            Ok(Json(json!({ "value": token })))
        };
        let app = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(async move || Json(discovery)),
            )
            .route("/jwks", get(async move || Json(keys)))
            .route("/token", get(mint));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url }
    }

    /// The issuer URL.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }
}
