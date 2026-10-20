//! HTTP routes.

mod health;
mod objects;

use std::sync::Arc;

use axum::Router;
use deadpool_postgres::Pool;

use crate::access::AccessLog;
use crate::cache::Caches;

/// Builds the router for all routes.
pub(crate) fn router(pool: Pool, caches: Arc<Caches>, access: Arc<AccessLog>) -> Router {
    Router::new()
        .merge(health::router(pool))
        .merge(objects::router(caches, access))
}
