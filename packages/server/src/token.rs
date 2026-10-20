//! Tokens that grant write access to caches.

use core::fmt;
use core::str::FromStr;
use core::time::Duration;

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
    /// The caches that the token may write to.
    pub(crate) caches: Vec<CacheName>,
}

impl Grant {
    /// Creates a grant that is valid for `lifetime` from now.
    pub(crate) fn new(
        sub: String,
        tier: String,
        caches: Vec<CacheName>,
        lifetime: Duration,
    ) -> Self {
        let iat = get_current_timestamp();
        Self {
            iss: ISSUER.to_owned(),
            sub,
            iat,
            exp: iat.saturating_add(lifetime.as_secs()),
            tier,
            caches,
        }
    }

    /// Checks whether the grant allows writing to the cache.
    pub(crate) fn allows(&self, cache: &str) -> bool {
        self.caches.iter().any(|allowed| allowed.as_ref() == cache)
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
        let cache = CacheName::try_from("sstate-pr".to_owned()).unwrap();
        let grant = Grant::new(
            "repo:inomotech/x:pull_request".to_owned(),
            "pr".to_owned(),
            vec![cache],
            Duration::from_hours(1),
        );
        let verified = key.verify(&key.sign(&grant).unwrap()).unwrap();
        assert_eq!(verified, grant, "the grant survives signing");
        assert!(verified.allows("sstate-pr"), "the listed cache is allowed");
        assert!(!verified.allows("sstate-main"), "other caches are not");
    }

    #[test]
    fn rejects_other_keys_and_expired_tokens() {
        let key_a = key("0123456789abcdef0123456789abcdef");
        let key_b = key("fedcba9876543210fedcba9876543210");
        let grant = Grant::new(
            "sub".to_owned(),
            "pr".to_owned(),
            Vec::new(),
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
