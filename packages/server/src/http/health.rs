//! Liveness and readiness probes.

use core::error::Error;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use deadpool_postgres::{Pool, PoolError};

/// Reasons why the server isn't ready.
#[derive(Debug, thiserror::Error)]
enum ReadyError {
    /// No database connection was available.
    #[error("failed to get a database connection")]
    Pool(#[from] PoolError),
    /// The database didn't answer.
    #[error("the database didn't answer")]
    Query(#[from] tokio_postgres::Error),
}

/// Builds the router for the probes.
pub(super) fn router(pool: Pool) -> Router {
    Router::new()
        .route("/healthz", get(live))
        .route("/readyz", get(ready))
        .with_state(pool)
}

/// Reports that the process is alive.
async fn live() -> StatusCode {
    StatusCode::OK
}

/// Reports whether the server can handle requests.
async fn ready(State(pool): State<Pool>) -> StatusCode {
    match check_database(&pool).await {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            tracing::warn!(error = &err as &dyn Error, "not ready");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// Checks that Postgres answers queries.
async fn check_database(pool: &Pool) -> Result<(), ReadyError> {
    let client = pool.get().await?;
    client.simple_query("SELECT 1").await?;
    Ok(())
}
