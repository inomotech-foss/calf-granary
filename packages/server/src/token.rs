//! Tokens that grant access to caches.

use core::fmt;
use core::str::FromStr;
use core::time::Duration;

use clap::Args;
use jsonwebtoken::errors::Error as JwtError;
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode, get_current_timestamp,
};
use serde::{Deserialize, Serialize};

use crate::cache::CacheName;

/// The issuer of granary tokens.
const ISSUER: &str = "calf-granary";

/// The shortest accepted secret, in bytes.
const MIN_SECRET_LENGTH: usize = 32;

/// The caches that a token grants.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Access {
    /// The caches that the token may write to and read.
    #[serde(default)]
    pub(crate) writes: Vec<CacheName>,
    /// The caches that the token may only read.
    #[serde(default)]
    pub(crate) reads: Vec<CacheName>,
}

impl Access {
    /// Checks whether writing to the cache is allowed.
    pub(crate) fn may_write(&self, cache: &str) -> bool {
        self.writes.iter().any(|allowed| allowed.as_ref() == cache)
    }

    /// Checks whether reading the cache, and through it its fallback, is
    /// allowed.
    pub(crate) fn may_read(&self, cache: &str) -> bool {
        self.may_write(cache) || self.reads.iter().any(|allowed| allowed.as_ref() == cache)
    }

    /// The granted caches and whether each may be written to.
    pub(crate) fn caches(&self) -> impl Iterator<Item = (&CacheName, bool)> {
        let writes = self.writes.iter().map(|cache| (cache, true));
        let reads = self
            .reads
            .iter()
            .filter(|cache| !self.writes.contains(cache))
            .map(|cache| (cache, false));
        writes.chain(reads)
    }
}

/// What a granary token grants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Grant {
    /// Always [`ISSUER`].
    iss: String,
    /// The subject of the ID token that was exchanged.
    pub(crate) sub: String,
    /// When the token was issued, in seconds since the Unix epoch.
    iat: u64,
    /// When the token expires, in seconds since the Unix epoch.
    pub(crate) exp: u64,
    /// The tier that the ID token mapped to.
    pub(crate) tier: String,
    /// The granted caches.
    #[serde(flatten)]
    pub(crate) access: Access,
}

impl Grant {
    /// Creates a grant that is valid for `lifetime` from now.
    pub(crate) fn new(sub: String, tier: String, access: Access, lifetime: Duration) -> Self {
        let iat = get_current_timestamp();
        Self {
            iss: ISSUER.to_owned(),
            sub,
            iat,
            exp: iat.saturating_add(lifetime.as_secs()),
            tier,
            access,
        }
    }
}

/// Command-line arguments that provide the token secret.
///
/// The secret is checked after parsing, so that errors don't repeat it.
#[derive(Args)]
pub(crate) struct TokenSecretArgs {
    /// Secret of at least 32 bytes that signs granary tokens.
    #[arg(long, env = "GRANARY_TOKEN_SECRET", hide_env_values = true)]
    token_secret: String,
}

impl TokenSecretArgs {
    /// Creates the key from the secret.
    pub(crate) fn key(&self) -> Result<TokenKey, ShortSecret> {
        self.token_secret.parse()
    }
}

impl fmt::Debug for TokenSecretArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenSecretArgs(..)")
    }
}

/// The secret that signs and verifies granary tokens.
#[derive(Clone)]
pub(crate) struct TokenKey {
    /// The key that signs tokens.
    encoding: EncodingKey,
    /// The key that verifies tokens.
    decoding: DecodingKey,
}

impl TokenKey {
    /// Signs a grant.
    pub(crate) fn sign(&self, grant: &Grant) -> Result<String, JwtError> {
        encode(&Header::new(Algorithm::HS256), grant, &self.encoding)
    }

    /// Verifies a token and returns its grant.
    pub(crate) fn verify(&self, token: &str) -> Result<Grant, JwtError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[ISSUER]);
        validation.set_required_spec_claims(&["exp", "iss"]);
        decode(token, &self.decoding, &validation).map(|data| data.claims)
    }
}

impl fmt::Debug for TokenKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenKey(..)")
    }
}

impl FromStr for TokenKey {
    type Err = ShortSecret;

    fn from_str(secret: &str) -> Result<Self, Self::Err> {
        if secret.len() < MIN_SECRET_LENGTH {
            return Err(ShortSecret);
        }
        Ok(Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        })
    }
}

/// A token secret that is too short.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("the token secret must be at least 32 bytes")]
pub(crate) struct ShortSecret;

#[cfg(test)]
mod tests {
    use super::*;

    fn key(secret: &str) -> TokenKey {
        secret.parse().unwrap()
    }

    #[test]
    fn round_trip() {
        let key = key("0123456789abcdef0123456789abcdef");
        let name = |name: &str| CacheName::try_from(name.to_owned()).unwrap();
        let grant = Grant::new(
            "repo:inomotech/x:pull_request".to_owned(),
            "pr".to_owned(),
            Access {
                writes: vec![name("sstate-pr")],
                reads: vec![name("downloads")],
            },
            Duration::from_hours(1),
        );
        let verified = key.verify(&key.sign(&grant).unwrap()).unwrap();
        assert_eq!(verified, grant, "the grant survives signing");
        let access = &verified.access;
        assert!(access.may_write("sstate-pr"), "the written cache");
        assert!(access.may_read("sstate-pr"), "writing includes reading");
        assert!(
            access.may_read("downloads") && !access.may_write("downloads"),
            "the read cache"
        );
        assert!(
            !access.may_read("sstate-main"),
            "other caches are not granted"
        );
    }

    #[test]
    fn rejects_other_keys_and_expired_tokens() {
        let key_a = key("0123456789abcdef0123456789abcdef");
        let key_b = key("fedcba9876543210fedcba9876543210");
        let grant = Grant::new(
            "sub".to_owned(),
            "pr".to_owned(),
            Access::default(),
            Duration::from_hours(1),
        );
        assert!(
            key_b.verify(&key_a.sign(&grant).unwrap()).is_err(),
            "another key's token is rejected"
        );
        let mut expired = grant;
        expired.exp = 1;
        assert!(
            key_a.verify(&key_a.sign(&expired).unwrap()).is_err(),
            "an expired token is rejected"
        );
    }

    #[test]
    fn rejects_short_secrets() {
        assert!(
            "short".parse::<TokenKey>().is_err(),
            "short secrets are rejected"
        );
    }
}
