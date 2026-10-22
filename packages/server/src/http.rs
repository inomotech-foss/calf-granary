//! HTTP routes.

mod health;
mod objects;
mod observe;
mod server;
mod token;

use std::borrow::Cow;
use std::sync::Arc;

use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::{Router, middleware};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use deadpool_postgres::Pool;

use self::observe::RequestMetrics;
pub(crate) use self::server::serve;
use crate::access::AccessLog;
use crate::cache::Caches;
use crate::content::Content;
use crate::exchange::Exchange;
use crate::token::TokenKey;

/// Builds the router for all routes.
///
/// Every route except the probes is traced and measured.
pub(crate) fn router(
    pool: Pool,
    caches: Arc<Caches>,
    content: Arc<Content>,
    access: Arc<AccessLog>,
    key: TokenKey,
    exchange: Arc<Exchange>,
    public_url: Option<String>,
) -> Router {
    let observed = Router::new()
        .merge(objects::router(
            Arc::clone(&caches),
            content,
            pool.clone(),
            access,
            key,
        ))
        .merge(token::router(exchange, caches, public_url))
        .route_layer(middleware::from_fn_with_state(
            RequestMetrics::new(),
            observe::observe,
        ));
    Router::new().merge(health::router(pool)).merge(observed)
}

/// Returns the bearer token from the `Authorization` header.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let (scheme, token) = headers.get(AUTHORIZATION)?.to_str().ok()?.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}

/// Returns the granary token from the `Authorization` header.
///
/// Besides bearer tokens, this accepts the token as the password of basic
/// authentication, which is what Nix sends from a netrc file.
fn credential(headers: &HeaderMap) -> Option<Cow<'_, str>> {
    if let Some(token) = bearer(headers) {
        return Some(Cow::Borrowed(token));
    }
    let (scheme, encoded) = headers.get(AUTHORIZATION)?.to_str().ok()?.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = String::from_utf8(STANDARD.decode(encoded.trim()).ok()?).ok()?;
    let start = decoded.find(':')?.checked_add(1)?;
    let mut password = decoded;
    password.drain(..start);
    Some(Cow::Owned(password))
}
