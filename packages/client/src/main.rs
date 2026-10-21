//! Build cache client for CI runners.

mod nix;
mod push;

use core::error::Error;
use std::io::{self, IsTerminal as _};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use crate::push::PushArgs;

/// Build cache client for CI runners.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    /// The command to run.
    #[command(subcommand)]
    command: Command,
}

/// A client command.
#[derive(Debug, Subcommand)]
enum Command {
    /// Work with Nix caches.
    Nix {
        /// The Nix command to run.
        #[command(subcommand)]
        command: NixCommand,
    },
}

/// A Nix command.
#[derive(Debug, Subcommand)]
enum NixCommand {
    /// Upload store paths and their closures that the cache lacks.
    Push(PushArgs),
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
    let Command::Nix {
        command: NixCommand::Push(args),
    } = Cli::parse().command;
    match args.run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "push failed");
            ExitCode::FAILURE
        }
    }
}
