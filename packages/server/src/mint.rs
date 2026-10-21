//! The `token` command.

use std::io::{self, Write as _};

use clap::Args;
use jsonwebtoken::errors::Error as JwtError;

use crate::cache::CacheName;
use crate::duration::ConfigDuration;
use crate::token::{Grant, TokenKey};

/// Command-line arguments of the `token` command.
#[derive(Debug, Args)]
pub(crate) struct MintArgs {
    /// The caches that the token may write to.
    #[arg(long = "cache", required = true)]
    caches: Vec<CacheName>,
    /// The tier recorded in the token.
    #[arg(long, default_value = "manual")]
    tier: String,
    /// The subject recorded in the token.
    #[arg(long, default_value = "manual")]
    subject: String,
    /// How long the token is valid, for example `90m`.
    #[arg(long, default_value = "1h")]
    lifetime: ConfigDuration,
    /// Secret of at least 32 bytes that signs granary tokens.
    #[arg(long, env = "GRANARY_TOKEN_SECRET", hide_env_values = true)]
    token_secret: TokenKey,
}

impl MintArgs {
    /// Signs a write token and prints it.
    pub(crate) fn run(self) -> Result<(), MintError> {
        let grant = Grant::new(self.subject, self.tier, self.caches, self.lifetime.into());
        let token = self.token_secret.sign(&grant).map_err(MintError::Sign)?;
        writeln!(io::stdout(), "{token}").map_err(MintError::Write)
    }
}

/// Errors from the `token` command.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MintError {
    /// Signing the token failed.
    #[error("failed to sign the token")]
    Sign(#[source] JwtError),
    /// Printing the token failed.
    #[error("failed to print the token")]
    Write(#[source] io::Error),
}
