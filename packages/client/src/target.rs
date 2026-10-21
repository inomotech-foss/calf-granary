//! The cache that a command uploads to.

use core::str::FromStr;

use clap::Args;
use url::Url;

/// Command-line arguments that name a cache and how to upload to it.
#[derive(Debug, Args)]
pub(crate) struct Target {
    /// The granary server, such as `https://granary.example.org`.
    #[arg(long, env = "GRANARY_SERVER")]
    server: Server,
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
    /// The URL of a path on the server, given as segments.
    pub(crate) fn url<'a, I>(&self, segments: I) -> Url
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut url = self.server.0.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(segments);
        }
        url
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

/// A server URL that paths can be appended to.
#[derive(Clone, Debug)]
struct Server(Url);

impl FromStr for Server {
    type Err = ServerError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(text).map_err(ServerError::Parse)?;
        if url.cannot_be_a_base() {
            return Err(ServerError::NotABase);
        }
        Ok(Self(url))
    }
}

/// Errors from parsing a server URL.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ServerError {
    /// The URL has no path that can be appended to, such as `mailto:`.
    #[error("the server URL can't have paths")]
    NotABase,
    /// The text isn't a URL.
    #[error("the server URL is invalid")]
    Parse(#[source] url::ParseError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_segments() {
        for server in [
            "https://granary.example.org",
            "https://granary.example.org/",
        ] {
            let server: Server = server.parse().unwrap();
            let target = Target {
                server,
                cache: "nix main".to_owned(),
                token: String::new(),
                jobs: 1,
            };
            assert_eq!(
                target
                    .url(["_api", "nix", target.cache(), "upload"])
                    .as_str(),
                "https://granary.example.org/_api/nix/nix%20main/upload",
                "segments are escaped"
            );
        }
        assert!(
            "mailto:granary@example.org".parse::<Server>().is_err(),
            "the URL needs a path"
        );
    }
}
