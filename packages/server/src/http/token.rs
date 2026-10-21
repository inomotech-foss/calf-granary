//! Exchanging ID tokens for granary tokens.

use core::error::Error;
use std::borrow::Cow;
use std::sync::Arc;

use axum::extract::State;
use axum::http::header::HOST;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use super::bearer;
use crate::cache::{Cache, CacheName, Caches, Protocol};
use crate::exchange::{Exchange, ExchangeError};
use crate::oidc::VerifyError;

impl ExchangeError {
    /// The status to respond with.
    const fn status(&self) -> StatusCode {
        match self {
            Self::Malformed(_)
            | Self::UnknownIssuer(_)
            | Self::Verify(
                VerifyError::Algorithm(_)
                | VerifyError::Invalid(_)
                | VerifyError::Key(_)
                | VerifyError::MissingKeyId
                | VerifyError::Rejected
                | VerifyError::UnknownKey(_),
            ) => StatusCode::UNAUTHORIZED,
            Self::NoTier { sub: _ } => StatusCode::FORBIDDEN,
            Self::Verify(VerifyError::Fetch { url: _, source: _ }) => StatusCode::BAD_GATEWAY,
            Self::Sign(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Tokens {
    /// The URL that clients reach the server at, without a trailing slash.
    fn base_url(&self, headers: &HeaderMap) -> Cow<'_, str> {
        if let Some(url) = self.public_url.as_deref() {
            return Cow::Borrowed(url.trim_end_matches('/'));
        }
        let mut url = request_url(headers);
        let len = url.trim_end_matches('/').len();
        url.truncate(len);
        Cow::Owned(url)
    }
}

/// Builds the router for the token exchange.
/// What clients learn about the caches besides their grants.
#[derive(Clone, Debug, Default)]
pub(crate) struct CacheDirectory {
    /// The URL that clients reach the server at, if configured.
    pub(crate) public_url: Option<String>,
    /// The caches that jobs without an ID token read from.
    pub(crate) anonymous: Vec<CacheName>,
}

/// Shared state of the token route.
#[derive(Clone, Debug)]
struct Tokens {
    /// Exchanges ID tokens.
    exchange: Arc<Exchange>,
    /// Describes the granted caches.
    caches: Arc<Caches>,
    /// The URL that clients reach the server at, if configured.
    public_url: Option<Arc<str>>,
    /// The caches that jobs without an ID token read from.
    anonymous: Arc<[CacheName]>,
}

/// A granary token and the caches it grants.
#[derive(Debug, Serialize)]
struct TokenResponse<'a> {
    /// The granary token.
    token: String,
    /// When the token expires, in seconds since the Unix epoch.
    expires_at: u64,
    /// The tier that the ID token mapped to.
    tier: String,
    /// The caches that the token may write to and read through.
    caches: Vec<CacheInfo<'a>>,
}

/// The caches that jobs without an ID token read from.
#[derive(Debug, Serialize)]
struct AnonymousResponse<'a> {
    /// The caches.
    caches: Vec<CacheInfo<'a>>,
}

/// How a client uses a granted cache.
#[derive(Debug, Serialize)]
struct CacheInfo<'a> {
    /// The cache name.
    name: &'a str,
    /// The protocol, `files`, `nix` or `hashserv`.
    protocol: &'static str,
    /// What the cache is for, such as `sstate` or `downloads`.
    role: &'a str,
    /// The URL that clients use.
    url: String,
    /// The public key of a Nix cache, for `trusted-public-keys`.
    #[serde(skip_serializing_if = "Option::is_none")]
    public_key: Option<&'a str>,
}

/// Builds the router for the token exchange.
pub(super) fn router(
    exchange: Arc<Exchange>,
    caches: Arc<Caches>,
    directory: CacheDirectory,
) -> Router {
    Router::new()
        .route("/token", post(issue))
        .route("/_api/caches", get(anonymous_caches))
        .with_state(Tokens {
            exchange,
            caches,
            public_url: directory.public_url.map(Into::into),
            anonymous: directory.anonymous.into(),
        })
}

/// Describes the caches that jobs without an ID token read from.
async fn anonymous_caches(State(state): State<Tokens>, headers: HeaderMap) -> Response {
    let base = state.base_url(&headers);
    let caches = state
        .anonymous
        .iter()
        .filter_map(|name| state.caches.get(name.as_ref()))
        .map(|cache| describe(cache, &base))
        .collect();
    Json(AnonymousResponse { caches }).into_response()
}

/// Exchanges the ID token in the `Authorization` header.
async fn issue(State(state): State<Tokens>, headers: HeaderMap) -> Response {
    let Some(id_token) = bearer(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.exchange.exchange(id_token).await {
        Ok(issued) => {
            let base = state.base_url(&headers);
            let caches = issued
                .caches
                .iter()
                .filter_map(|name| state.caches.get(name.as_ref()))
                .map(|cache| describe(cache, &base))
                .collect();
            Json(TokenResponse {
                token: issued.token,
                expires_at: issued.expires_at,
                tier: issued.tier,
                caches,
            })
            .into_response()
        }
        Err(err) => {
            let status = err.status();
            if status.is_server_error() {
                tracing::error!(error = &err as &dyn Error, "failed to exchange a token");
            } else {
                tracing::info!(error = &err as &dyn Error, "rejected a token exchange");
            }
            status.into_response()
        }
    }
}

/// Describes a cache for clients.
fn describe<'a>(cache: &'a Cache, base: &str) -> CacheInfo<'a> {
    let name = cache.name().as_ref();
    let (protocol, url, public_key) = match cache.protocol() {
        Protocol::Files => ("files", format!("{base}/{name}"), None),
        Protocol::Nix(nix) => ("nix", format!("{base}/{name}"), Some(nix.key.public_key())),
        Protocol::Hashserv => {
            let (scheme, rest) = base
                .strip_prefix("https://")
                .map(|rest| ("wss://", rest))
                .or_else(|| base.strip_prefix("http://").map(|rest| ("ws://", rest)))
                .unwrap_or(("", base));
            ("hashserv", format!("{scheme}{rest}/_hashserv/{name}"), None)
        }
    };
    CacheInfo {
        name,
        protocol,
        role: cache.role(),
        url,
        public_key,
    }
}

/// The URL that the request reached the server at, from the `Host` and
/// `X-Forwarded-Proto` headers.
fn request_url(headers: &HeaderMap) -> String {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("localhost");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .filter(|scheme| matches!(*scheme, "http" | "https"))
        .unwrap_or("http");
    format!("{scheme}://{host}")
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;
    use crate::cache::NixCache;
    use crate::nix::SigningKey;
    use crate::nix::signing_tests::TEST_KEY;

    fn cache(name: &str, protocol: Protocol, role: Option<&str>) -> Cache {
        Cache::new(
            CacheName::try_from(name.to_owned()).unwrap(),
            None,
            false,
            None,
            protocol,
            role.map(ToOwned::to_owned),
            None,
        )
    }

    #[test]
    fn describes_caches() {
        let files = cache("sstate-pr", Protocol::Files, Some("sstate"));
        let files = describe(&files, "https://g.example");
        assert_eq!(files.url, "https://g.example/sstate-pr", "files URL");
        assert_eq!(files.role, "sstate", "configured role");

        let nix = cache(
            "nix-pr",
            Protocol::Nix(NixCache::new(SigningKey::load_text(TEST_KEY).unwrap(), 41)),
            None,
        );
        let nix = describe(&nix, "https://g.example");
        assert_eq!(nix.role, "nix", "the role defaults to the protocol");
        assert!(
            nix.public_key.is_some_and(|key| key.starts_with("test-1:")),
            "Nix caches name their key"
        );

        let hashserv_cache = cache("hash-pr", Protocol::Hashserv, None);
        let hashserv = describe(&hashserv_cache, "https://g.example");
        assert_eq!(
            hashserv.url, "wss://g.example/_hashserv/hash-pr",
            "secure WebSocket"
        );
        let local = describe(&hashserv_cache, "http://localhost:8080");
        assert_eq!(
            local.url, "ws://localhost:8080/_hashserv/hash-pr",
            "plain WebSocket"
        );
    }

    #[test]
    fn derives_the_url_from_the_request() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("g.example"));
        assert_eq!(
            request_url(&headers),
            "http://g.example",
            "plain by default"
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        assert_eq!(request_url(&headers), "https://g.example", "behind a proxy");
    }
}
