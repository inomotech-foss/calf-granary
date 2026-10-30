//! Build cache client for CI runners.

mod nix;
mod nix_push;
mod scan;
mod target;
mod watch;
mod yocto;

use core::error::Error;
use std::io::{self, IsTerminal as _};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use crate::nix_push::PushArgs;
use crate::watch::WatchArgs;
use crate::yocto::YoctoCommand;

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
    /// Upload new store paths and Yocto files while a build runs.
    Watch(WatchArgs),
    /// Work with Yocto caches.
    Yocto {
        /// The Yocto command to run.
        #[command(subcommand)]
        command: YoctoCommand,
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
    let result: Result<(), Box<dyn Error>> = match Cli::parse().command {
        Command::Nix {
            command: NixCommand::Push(args),
        } => args.run().await.map_err(Into::into),
        Command::Watch(args) => args.run().await.map_err(Into::into),
        Command::Yocto { command } => command.run().await.map_err(Into::into),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = &*err, "push failed");
            ExitCode::FAILURE
        }
    }
}
