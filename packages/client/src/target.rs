//! The cache that a command uploads to.

use clap::Args;

/// Command-line arguments that name a cache and how to upload to it.
#[derive(Debug, Args)]
pub(crate) struct Target {
    /// The granary server, such as `https://granary.example.org`.
    #[arg(long, env = "GRANARY_SERVER")]
    server: String,
    /// The cache to upload to.
    #[arg(long)]
    cache: String,
    /// A granary write token for the cache.
    #[arg(long, env = "GRANARY_TOKEN", hide_env_values = true)]
    token: String,
    /// How many uploads run at the same time.
    #[arg(long, default_value_t = 4)]
    jobs: usize,
}

impl Target {
    /// The URL of a path on the server.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.server.trim_end_matches('/'))
    }

    /// The cache name.
    pub(crate) fn cache(&self) -> &str {
        &self.cache
    }

    /// The write token.
    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    /// How many uploads run at the same time, at least one.
    pub(crate) fn jobs(&self) -> usize {
        self.jobs.max(1)
    }
}
