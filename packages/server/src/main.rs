//! Build cache server for CI runners.

mod access;
mod cache;
mod claims;
mod config;
mod database;
mod duration;
mod exchange;
mod http;
mod migrations;
mod notifications;
mod oidc;
mod prune;
mod serve;
mod tasks;
mod telemetry;
#[cfg(test)]
mod testing;
mod tiers;
mod token;
mod worker;

use core::error::Error;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tokio::task;

use crate::database::DatabaseArgs;
use crate::migrations::MigrationError;
use crate::prune::{PruneArgs, PruneCommandError};
use crate::serve::{ServeArgs, ServeError};
use crate::telemetry::Telemetry;
use crate::worker::{WorkerArgs, WorkerError};

/// Build cache server for CI runners.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    /// The command to run.
    #[command(subcommand)]
    command: Command,
}

/// A server command.
#[derive(Debug, Subcommand)]
enum Command {
    /// Apply pending database migrations.
    Migrate {
        /// Where to find Postgres.
        #[command(flatten)]
        database: DatabaseArgs,
    },
    /// Delete objects that weren't used within their cache's retention.
    Prune(PruneArgs),
    /// Serve the caches over HTTP.
    Serve(ServeArgs),
    /// Run scheduled and requested background tasks.
    Worker(WorkerArgs),
}

impl Command {
    /// The `service.name` of the process in telemetry.
    const fn service_name(&self) -> &'static str {
        match self {
            Self::Migrate { database: _ } => "migrate",
            Self::Prune(_) => "prune",
            Self::Serve(_) => "serve",
            Self::Worker(_) => "worker",
        }
    }

    /// Runs the command.
    async fn run(self) -> Result<(), CommandError> {
        match self {
            Self::Migrate { database } => {
                let mut client = database.connect().await.map_err(CommandError::Connect)?;
                let count = migrations::migrate(&mut client).await?;
                tracing::info!(count, "migrations applied");
                Ok(())
            }
            Self::Prune(args) => Ok(args.run().await?),
            Self::Serve(args) => Ok(args.run().await?),
            Self::Worker(args) => Ok(args.run().await?),
        }
    }
}

/// Errors from running a command.
#[derive(Debug, thiserror::Error)]
enum CommandError {
    /// Connecting to Postgres failed.
    #[error("failed to connect to the database")]
    Connect(#[source] tokio_postgres::Error),
    /// Applying migrations failed.
    #[error(transparent)]
    Migrate(#[from] MigrationError),
    /// Pruning failed.
    #[error(transparent)]
    Prune(#[from] PruneCommandError),
    /// Serving failed.
    #[error(transparent)]
    Serve(#[from] ServeError),
    /// The worker failed.
    #[error(transparent)]
    Worker(#[from] WorkerError),
}

#[tokio::main]
async fn main() -> ExitCode {
    let command = Cli::parse().command;
    let telemetry = match Telemetry::init(command.service_name()) {
        Ok(telemetry) => telemetry,
        Err(err) => {
            Telemetry::fallback();
            tracing::error!(error = &err as &dyn Error, "failed to set up telemetry");
            return ExitCode::FAILURE;
        }
    };

    let code = match command.run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "command failed");
            ExitCode::FAILURE
        }
    };
    if let Err(err) = task::spawn_blocking(move || telemetry.shutdown()).await {
        tracing::error!(error = &err as &dyn Error, "failed to flush telemetry");
    }
    code
}
