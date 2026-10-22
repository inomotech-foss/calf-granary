//! The `token` command.

use std::io::{self, Write as _};

use clap::Args;
use jsonwebtoken::errors::Error as JwtError;

use crate::cache::CacheName;
use crate::duration::ConfigDuration;
use crate::token::{Access, Grant, ShortSecret, TokenSecretArgs};

/// Command-line arguments of the `token` command.
#[derive(Debug, Args)]
pub(crate) struct MintArgs {
    /// The caches that the token may write to and read.
    #[arg(long = "cache")]
    writes: Vec<CacheName>,
    /// The caches that the token may only read.
    #[arg(long = "read")]
    reads: Vec<CacheName>,
    /// The tier recorded in the token.
    #[arg(long, default_value = "manual")]
    tier: String,
    /// The subject recorded in the token.
    #[arg(long, default_value = "manual")]
    subject: String,
    /// How long the token is valid, for example `90m`.
    #[arg(long, default_value = "1h")]
    lifetime: ConfigDuration,
    /// The secret that signs granary tokens.
    #[command(flatten)]
    token: TokenSecretArgs,
}

impl MintArgs {
    /// Signs a token and prints it.
    pub(crate) fn run(self) -> Result<(), MintError> {
        let key = self.token.key()?;
        let access = Access {
            writes: self.writes,
            reads: self.reads,
        };
        let grant = Grant::new(self.subject, self.tier, access, self.lifetime.into());
        let token = key.sign(&grant).map_err(MintError::Sign)?;
        writeln!(io::stdout(), "{token}").map_err(MintError::Write)
    }
}

/// Errors from the `token` command.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MintError {
    /// Signing the token failed.
    #[error("failed to sign the token")]
    Sign(#[source] JwtError),
    /// The token secret is too short.
    #[error(transparent)]
    TokenSecret(#[from] ShortSecret),
    /// Printing the token failed.
    #[error("failed to print the token")]
    Write(#[source] io::Error),
}
