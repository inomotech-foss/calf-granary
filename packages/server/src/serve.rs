//! The `serve` command.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use deadpool_postgres::{BuildError, PoolError};
use tokio::net::TcpListener;
use tokio::signal::ctrl_c;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::oneshot;
use tokio::task::JoinError;

use crate::access::AccessLog;
use crate::config::{Config, ConfigError};
use crate::database::DatabaseArgs;
use crate::http;
use crate::migrations::{self, MigrationError};
use crate::token::TokenKey;

/// Command-line arguments of the `serve` command.
#[derive(Debug, Args)]
pub(crate) struct ServeArgs {
    /// Path to the configuration file.
    #[arg(long, env = "GRANARY_CONFIG")]
    config: PathBuf,
    /// Address to listen on.
    #[arg(long, env = "GRANARY_LISTEN", default_value = "[::]:8080")]
    listen: SocketAddr,
    /// Where to find Postgres.
    #[command(flatten)]
    database: DatabaseArgs,
    /// Secret of at least 32 bytes that signs granary tokens.
    #[arg(long, env = "GRANARY_TOKEN_SECRET", hide_env_values = true)]
    token_secret: TokenKey,
}

impl ServeArgs {
    /// Serves HTTP until the process receives SIGINT or SIGTERM.
    ///
    /// Refuses to start while migrations are pending.
    pub(crate) async fn run(self) -> Result<(), ServeError> {
        let config = Config::load(&self.config)?;
        let caches = config.caches()?;
        let exchange = config.exchange(self.token_secret.clone())?;
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

        let access = Arc::new(AccessLog::default());
        let (stop, stopped) = oneshot::channel();
        let recorder = tokio::spawn(Arc::clone(&access).run(pool.clone(), stopped));
        let served = axum::serve(
            listener,
            http::router(
                pool,
                Arc::new(caches),
                access,
                self.token_secret,
                Arc::new(exchange),
            ),
        )
        .with_graceful_shutdown(shutdown(terminate))
        .await;
        drop(stop);
        recorder.await.map_err(ServeError::Recorder)?;
        served.map_err(ServeError::Serve)
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
    /// The configuration is invalid.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// Connecting to Postgres failed.
    #[error("failed to connect to the database")]
    Connect(#[source] PoolError),
    /// The schema doesn't match this binary.
    #[error(transparent)]
    Migrations(#[from] MigrationError),
    /// Creating the connection pool failed.
    #[error("failed to create the database pool")]
    Pool(#[source] BuildError),
    /// The task that records reads failed.
    #[error("the read recorder failed")]
    Recorder(#[source] JoinError),
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
