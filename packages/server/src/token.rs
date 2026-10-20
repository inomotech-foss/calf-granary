//! Tokens that grant write access to caches.

use core::fmt;
use core::str::FromStr;
use core::time::Duration;

use jsonwebtoken::errors::Error as JwtError;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, get_current_timestamp};
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
}

/// The secret that signs granary tokens.
#[derive(Clone)]
pub(crate) struct TokenKey {
    /// The key that signs tokens.
    encoding: EncodingKey,
}

impl TokenKey {
    /// Signs a grant.
    pub(crate) fn sign(&self, grant: &Grant) -> Result<String, JwtError> {
        encode(&Header::new(Algorithm::HS256), grant, &self.encoding)
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
        })
    }
}

/// A token secret that is too short.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("the token secret must be at least 32 bytes")]
pub(crate) struct ShortSecret;

#[cfg(test)]
mod tests {
    use jsonwebtoken::dangerous::insecure_decode_claims;

    use super::*;

    fn key(secret: &str) -> TokenKey {
        secret.parse().unwrap()
    }

    #[test]
    fn signed_tokens_carry_the_grant() {
        let key = key("0123456789abcdef0123456789abcdef");
        let cache = CacheName::try_from("sstate-pr".to_owned()).unwrap();
        let grant = Grant::new(
            "repo:inomotech/x:pull_request".to_owned(),
            "pr".to_owned(),
            vec![cache],
            Duration::from_hours(1),
        );
        let signed: Grant = insecure_decode_claims(key.sign(&grant).unwrap()).unwrap();
        assert_eq!(signed, grant, "the grant survives signing");
    }

    #[test]
    fn rejects_short_secrets() {
        assert!(
            "short".parse::<TokenKey>().is_err(),
            "short secrets are rejected"
        );
    }
}
