//! HTTP routes.

use core::error::Error;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use deadpool_postgres::{Pool, PoolError};

/// Shared state of the HTTP handlers.
#[derive(Clone, Debug)]
pub(crate) struct AppState {
    /// Postgres connections.
    pool: Pool,
}

impl AppState {
    /// Creates the state.
    pub(crate) const fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Builds the router for all routes.
    pub(crate) fn router(self) -> Router {
        Router::new()
            .route("/healthz", get(live))
            .route("/readyz", get(ready))
            .with_state(self)
    }

    /// Checks that Postgres answers queries.
    async fn check_database(&self) -> Result<(), ReadyError> {
        let client = self.pool.get().await?;
        client.simple_query("SELECT 1").await?;
        Ok(())
    }
}

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

/// Reports that the process is alive.
async fn live() -> StatusCode {
    StatusCode::OK
}

/// Reports whether the server can handle requests.
async fn ready(State(state): State<AppState>) -> StatusCode {
    match state.check_database().await {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            tracing::warn!(error = &err as &dyn Error, "not ready");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
