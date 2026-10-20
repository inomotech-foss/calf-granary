//! HTTP routes.

mod health;

use axum::Router;
use deadpool_postgres::Pool;

/// Builds the router for all routes.
pub(crate) fn router(pool: Pool) -> Router {
    Router::new().merge(health::router(pool))
}
