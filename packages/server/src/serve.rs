//! The `serve` command.

use std::io;
use std::net::SocketAddr;

use clap::Args;
use deadpool_postgres::{BuildError, PoolError};
use tokio::net::TcpListener;
use tokio::signal::ctrl_c;
use tokio::signal::unix::{Signal, SignalKind, signal};

use crate::database::DatabaseArgs;
use crate::http;
use crate::migrations::{self, MigrationError};

/// Command-line arguments of the `serve` command.
#[derive(Debug, Args)]
pub(crate) struct ServeArgs {
    /// Address to listen on.
    #[arg(long, env = "GRANARY_LISTEN", default_value = "[::]:8080")]
    listen: SocketAddr,
    /// Where to find Postgres.
    #[command(flatten)]
    database: DatabaseArgs,
}

impl ServeArgs {
    /// Serves HTTP until the process receives SIGINT or SIGTERM.
    ///
    /// Refuses to start while migrations are pending.
    pub(crate) async fn run(self) -> Result<(), ServeError> {
        let pool = self.database.pool().map_err(ServeError::Pool)?;
        let client = pool.get().await.map_err(ServeError::Connect)?;
        migrations::verify(&client).await?;
        drop(client);

        let terminate = signal(SignalKind::terminate()).map_err(ServeError::Signal)?;
        let listener = TcpListener::bind(self.listen)
            .await
            .map_err(|source| ServeError::Bind {
                address: self.listen,
                source,
            })?;
        tracing::info!(address = %self.listen, "listening");
        axum::serve(listener, http::router(pool))
            .with_graceful_shutdown(shutdown(terminate))
            .await
            .map_err(ServeError::Serve)
    }
}

/// Errors from the `serve` command.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ServeError {
    /// Binding the listen address failed.
    #[error("failed to listen on {address}")]
    Bind {
        /// The address to listen on.
        address: SocketAddr,
        /// The error from the operating system.
        #[source]
        source: io::Error,
    },
    /// Connecting to Postgres failed.
    #[error("failed to connect to the database")]
    Connect(#[source] PoolError),
    /// The schema doesn't match this binary.
    #[error(transparent)]
    Migrations(#[from] MigrationError),
    /// Creating the connection pool failed.
    #[error("failed to create the database pool")]
    Pool(#[source] BuildError),
    /// Serving HTTP failed.
    #[error("failed to serve HTTP")]
    Serve(#[source] io::Error),
    /// Installing the signal handler failed.
    #[error("failed to install the SIGTERM handler")]
    Signal(#[source] io::Error),
}

/// Resolves when the process receives SIGINT or SIGTERM.
async fn shutdown(mut terminate: Signal) {
    tokio::select! {
        _ = ctrl_c() => {}
        _ = terminate.recv() => {}
    }
    tracing::info!("shutting down");
}
