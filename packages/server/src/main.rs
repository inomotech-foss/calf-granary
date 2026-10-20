//! Build cache server for CI runners.

mod database;
mod http;
mod migrations;
mod serve;

use core::error::Error;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use crate::database::DatabaseArgs;
use crate::migrations::MigrationError;
use crate::serve::{ServeArgs, ServeError};

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
    /// Serve the caches over HTTP.
    Serve(ServeArgs),
}

impl Command {
    /// Runs the command.
    async fn run(self) -> Result<(), CommandError> {
        match self {
            Self::Migrate { database } => {
                let mut client = database.connect().await.map_err(CommandError::Connect)?;
                let count = migrations::migrate(&mut client).await?;
                tracing::info!(count, "migrations applied");
                Ok(())
            }
            Self::Serve(args) => Ok(args.run().await?),
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
    /// Serving failed.
    #[error(transparent)]
    Serve(#[from] ServeError),
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    match Cli::parse().command.run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "command failed");
            ExitCode::FAILURE
        }
    }
}
