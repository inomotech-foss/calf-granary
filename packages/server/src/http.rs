//! HTTP routes.

mod health;
mod objects;
mod token;

use std::sync::Arc;

use axum::Router;
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use deadpool_postgres::Pool;

use crate::access::AccessLog;
use crate::cache::Caches;
use crate::exchange::Exchange;
use crate::token::TokenKey;

/// Builds the router for all routes.
pub(crate) fn router(
    pool: Pool,
    caches: Arc<Caches>,
    access: Arc<AccessLog>,
    key: TokenKey,
    exchange: Arc<Exchange>,
) -> Router {
    Router::new()
        .merge(health::router(pool))
        .merge(objects::router(caches, access, key))
        .merge(token::router(exchange))
}

/// Returns the bearer token from the `Authorization` header.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let (scheme, token) = headers.get(AUTHORIZATION)?.to_str().ok()?.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}
