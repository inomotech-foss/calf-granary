//! Exchanging ID tokens from CI jobs for granary tokens.

use core::time::Duration;

use jsonwebtoken::dangerous::insecure_decode_claims;
use jsonwebtoken::errors::Error as JwtError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::CacheName;
use crate::oidc::{Issuer, VerifyError};
use crate::tiers::Tiers;
use crate::token::{Grant, TokenKey};

/// How long issued tokens are valid.
const TOKEN_LIFETIME: Duration = Duration::from_hours(6);

/// The claim that selects the issuer, read before the token is verified.
#[derive(Debug, Deserialize)]
struct Unverified {
    /// The issuer URL.
    iss: String,
}

/// A granary token and what it grants.
#[derive(Debug, Serialize)]
pub(crate) struct Issued {
    /// The granary token.
    token: String,
    /// When the token expires, in seconds since the Unix epoch.
    expires_at: u64,
    /// The tier that the ID token mapped to.
    tier: String,
    /// The caches that the token may write to.
    caches: Vec<CacheName>,
}

/// Exchanges ID tokens from trusted issuers for granary tokens.
#[derive(Debug)]
pub(crate) struct Exchange {
    /// The trusted issuers.
    issuers: Vec<Issuer>,
    /// The tiers that map claims to caches.
    tiers: Tiers,
    /// The key that signs granary tokens.
    key: TokenKey,
}

impl Exchange {
    /// Creates the exchange.
    pub(crate) const fn new(issuers: Vec<Issuer>, tiers: Tiers, key: TokenKey) -> Self {
        Self {
            issuers,
            tiers,
            key,
        }
    }

    /// Verifies an ID token and issues a granary token for its tier.
    pub(crate) async fn exchange(&self, id_token: &str) -> Result<Issued, ExchangeError> {
        let iss = insecure_decode_claims::<Unverified>(id_token)
            .map_err(ExchangeError::Malformed)?
            .iss;
        let issuer = self
            .issuers
            .iter()
            .find(|issuer| issuer.url() == iss)
            .ok_or_else(|| ExchangeError::UnknownIssuer(iss.clone()))?;
        let claims = issuer.verify(id_token).await?;
        let sub = claims
            .get("sub")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let tier = self
            .tiers
            .select(&claims)
            .ok_or_else(|| ExchangeError::NoTier { sub: sub.clone() })?;
        let grant = Grant::new(sub, tier.name.clone(), tier.caches.clone(), TOKEN_LIFETIME);
        let token = self.key.sign(&grant).map_err(ExchangeError::Sign)?;
        tracing::info!(
            issuer = iss,
            sub = grant.sub,
            tier = grant.tier,
            "issued a token"
        );
        Ok(Issued {
            token,
            expires_at: grant.exp,
            tier: grant.tier,
            caches: grant.caches,
        })
    }
}

/// Errors from exchanging an ID token.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ExchangeError {
    /// The ID token can't be decoded.
    #[error("the ID token is malformed")]
    Malformed(#[source] JwtError),
    /// No tier matches the ID token's claims.
    #[error("no tier matches {sub}")]
    NoTier {
        /// The subject of the ID token.
        sub: String,
    },
    /// Signing the granary token failed.
    #[error("failed to sign the token")]
    Sign(#[source] JwtError),
    /// The ID token comes from an issuer that isn't trusted.
    #[error("the issuer {0} isn't trusted")]
    UnknownIssuer(String),
    /// The ID token failed verification.
    #[error("the ID token failed verification")]
    Verify(#[from] VerifyError),
}

#[cfg(test)]
mod tests {
    use reqwest::Client;
    use serde_json::json;

    use super::*;
    use crate::testing::FakeIssuer;

    fn exchange(fake: &FakeIssuer) -> Exchange {
        let issuer = serde_json::from_value(json!({
            "issuer": fake.url(),
            "audience": "granary",
            "claims": { "repository_owner": ["inomotech"] },
        }))
        .unwrap();
        let tiers = serde_json::from_value(json!([
            { "name": "main", "caches": ["sstate-main"], "claims": { "ref": ["refs/heads/main"] } },
            { "name": "pr", "caches": ["sstate-pr"], "claims": { "ref": ["refs/pull/*"] } },
        ]))
        .unwrap();
        let key = "0123456789abcdef0123456789abcdef".parse().unwrap();
        Exchange::new(vec![Issuer::new(issuer, Client::new())], tiers, key)
    }

    #[tokio::test]
    async fn issues_tokens_for_the_matching_tier() {
        let fake = FakeIssuer::start().await;
        let exchange = exchange(&fake);
        let id_token = fake.token(&json!({
            "aud": "granary",
            "repository_owner": "inomotech",
            "ref": "refs/pull/7/merge",
        }));
        let issued = exchange.exchange(&id_token).await.unwrap();
        assert_eq!(issued.tier, "pr", "the pr tier matches");
        let grant: Grant = insecure_decode_claims(&issued.token).unwrap();
        assert_eq!(grant.caches, issued.caches, "the token writes sstate-pr");
    }

    #[tokio::test]
    async fn rejects_unmatched_and_unknown_tokens() {
        let fake = FakeIssuer::start().await;
        let other = FakeIssuer::start().await;
        let exchange = exchange(&fake);
        let unmatched = fake.token(&json!({
            "aud": "granary",
            "repository_owner": "inomotech",
            "ref": "refs/heads/feature",
        }));
        assert!(
            matches!(
                exchange.exchange(&unmatched).await,
                Err(ExchangeError::NoTier { sub: _ })
            ),
            "a ref outside every tier is rejected"
        );
        let unknown = other.token(&json!({ "aud": "granary", "repository_owner": "inomotech" }));
        assert!(
            matches!(
                exchange.exchange(&unknown).await,
                Err(ExchangeError::UnknownIssuer(_))
            ),
            "an untrusted issuer is rejected"
        );
    }
}
