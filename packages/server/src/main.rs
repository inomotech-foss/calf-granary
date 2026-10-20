//! Build cache server for CI runners.

mod migrations;

use core::error::Error;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tokio_postgres::{Client, Config, NoTls};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use crate::migrations::MigrationError;

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
        /// Postgres connection URL.
        #[arg(long, env = "GRANARY_DATABASE_URL", hide_env_values = true)]
        database_url: Config,
    },
}

impl Command {
    /// Runs the command.
    async fn run(self) -> Result<(), CommandError> {
        match self {
            Self::Migrate { database_url } => {
                let mut client = connect(&database_url).await?;
                let count = migrations::migrate(&mut client).await?;
                tracing::info!(count, "migrations applied");
                Ok(())
            }
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
}

/// Connects to Postgres and drives the connection in the background.
async fn connect(config: &Config) -> Result<Client, CommandError> {
    let (client, connection) = config.connect(NoTls).await.map_err(CommandError::Connect)?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            tracing::error!(error = &err as &dyn Error, "database connection failed");
        }
    });
    Ok(client)
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
