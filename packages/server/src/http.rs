//! HTTP routes.

mod health;
mod objects;
mod observe;
mod token;

use std::sync::Arc;

use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::{Router, middleware};
use deadpool_postgres::Pool;

use self::observe::RequestMetrics;
use crate::access::AccessLog;
use crate::cache::Caches;
use crate::exchange::Exchange;
use crate::token::TokenKey;

/// Builds the router for all routes.
///
/// Every route except the probes is traced and measured.
pub(crate) fn router(
    pool: Pool,
    caches: Arc<Caches>,
    access: Arc<AccessLog>,
    key: TokenKey,
    exchange: Arc<Exchange>,
) -> Router {
    let observed = Router::new()
        .merge(objects::router(caches, access, key))
        .merge(token::router(exchange))
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
