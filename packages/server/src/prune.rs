//! The `prune` command.

use core::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use calf_granary_server_db::queries::task::{complete_task, fail_task, start_task};
use clap::Args;
use deadpool_postgres::{BuildError, PoolError};

use crate::config::{Config, ConfigError};
use crate::database::{DatabaseArgs, TlsError};
use crate::migrations::{self, MigrationError};
use crate::tasks::{Runner, TaskError, TaskKind, WorkerId};

/// Command-line arguments of the `prune` command.
#[derive(Debug, Args)]
pub(crate) struct PruneArgs {
    /// Path to the configuration file.
    #[arg(long, env = "GRANARY_CONFIG")]
    config: PathBuf,
    /// Where to find Postgres.
    #[command(flatten)]
    database: DatabaseArgs,
}

impl PruneArgs {
    /// Prunes every cache once and records the run as a task.
    pub(crate) async fn run(self) -> Result<(), PruneCommandError> {
        let config = Config::load(&self.config)?;
        let caches = config.caches()?;
        let content = config.content()?;
        let pool = self
            .database
            .open()?
            .pool()
            .map_err(PruneCommandError::Pool)?;
        let client = pool.get().await.map_err(PruneCommandError::Connect)?;
        migrations::verify(&client).await?;

        let worker = WorkerId::current();
        let kind = TaskKind::Prune;
        let id = start_task()
            .bind(&client, &kind.name(), &"cli", &worker.as_ref())
            .one()
            .await?;
        let result = Runner::new(pool, Arc::new(caches), Arc::new(content))
            .run(id, kind)
            .await;
        match result {
            Ok(()) => {
                complete_task().bind(&client, &id, &worker.as_ref()).await?;
                Ok(())
            }
            Err(err) => {
                fail_task()
                    .bind(&client, &1, &0.0, &err.describe(), &id, &worker.as_ref())
                    .await?;
                tracing::error!(error = &err as &dyn Error, task = id, "prune failed");
                Err(err.into())
            }
        }
    }
}

/// Errors from the `prune` command.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PruneCommandError {
    /// The configuration is invalid.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// Connecting to Postgres failed.
    #[error("failed to connect to the database")]
    Connect(#[source] PoolError),
    /// Recording the task failed.
    #[error("failed to record the task")]
    Database(#[from] tokio_postgres::Error),
    /// The schema doesn't match this binary.
    #[error(transparent)]
    Migrations(#[from] MigrationError),
    /// Creating the connection pool failed.
    #[error("failed to create the database pool")]
    Pool(#[source] BuildError),
    /// Pruning failed.
    #[error(transparent)]
    Task(#[from] TaskError),
    /// TLS for Postgres can't be set up.
    #[error(transparent)]
    Tls(#[from] TlsError),
}
