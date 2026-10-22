//! Verifying ID tokens from trusted OIDC issuers.

use core::time::Duration;
use std::time::Instant;

use jsonwebtoken::errors::Error as JwtError;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use crate::claims::Requirements;

/// How long fetched keys are used before they are fetched again.
const KEYS_MAX_AGE: Duration = Duration::from_hours(1);

/// How long an unknown key ID waits before keys are fetched again.
const KEYS_MIN_AGE: Duration = Duration::from_mins(1);

/// The signature algorithms accepted from issuers.
const ALGORITHMS: [Algorithm; 9] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::EdDSA,
];

/// The configuration of a trusted issuer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssuerConfig {
    /// The issuer URL, as in the `iss` claim.
    issuer: String,
    /// The audience that tokens must be issued for.
    audience: String,
    /// Patterns that every token from this issuer must match.
    claims: Restrictions,
}

/// Claim patterns that limit an issuer to the expected repositories.
///
/// A forge signs tokens for every repository on it, so an issuer without
/// patterns would hand out granary tokens to all of them.
#[derive(Debug, Deserialize)]
#[serde(try_from = "Requirements")]
struct Restrictions(Requirements);

impl TryFrom<Requirements> for Restrictions {
    type Error = Unrestricted;

    fn try_from(requirements: Requirements) -> Result<Self, Self::Error> {
        if requirements.is_empty() {
            Err(Unrestricted)
        } else {
            Ok(Self(requirements))
        }
    }
}

/// An issuer has no claim patterns.
#[derive(Debug, thiserror::Error)]
#[error("issuer claims must not be empty, restrict them to your repositories")]
pub(crate) struct Unrestricted;

/// The part of the OIDC discovery document that is needed.
#[derive(Debug, Deserialize)]
struct Discovery {
    /// Where the signing keys are published.
    jwks_uri: String,
}

/// Signing keys fetched from an issuer.
#[derive(Debug)]
struct Keys {
    /// The keys.
    set: JwkSet,
    /// When the keys were fetched.
    fetched: Instant,
}

/// A trusted OIDC issuer.
#[derive(Debug)]
pub(crate) struct Issuer {
    /// The issuer URL, as in the `iss` claim.
    url: String,
    /// The audience that tokens must be issued for.
    audience: String,
    /// Patterns that every token must match.
    claims: Requirements,
    /// The client that fetches keys.
    client: Client,
    /// The cached keys.
    keys: Mutex<Option<Keys>>,
}

impl Issuer {
    /// Creates an issuer that fetches its keys with `client`.
    pub(crate) fn new(config: IssuerConfig, client: Client) -> Self {
        Self {
            url: config.issuer,
            audience: config.audience,
            claims: config.claims.0,
            client,
            keys: Mutex::new(None),
        }
    }

    /// The issuer URL, as in the `iss` claim.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Verifies a token and returns its claims.
    pub(crate) async fn verify(&self, token: &str) -> Result<Map<String, Value>, VerifyError> {
        let header = decode_header(token).map_err(VerifyError::Invalid)?;
        if !ALGORITHMS.contains(&header.alg) {
            return Err(VerifyError::Algorithm(header.alg));
        }
        let kid = header.kid.ok_or(VerifyError::MissingKeyId)?;
        let key = self.key(&kid).await?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.url]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let claims = decode::<Map<String, Value>>(token, &key, &validation)
            .map_err(VerifyError::Invalid)?
            .claims;
        if self.claims.allow(&claims) {
            Ok(claims)
        } else {
            Err(VerifyError::Rejected)
        }
    }

    /// Returns the key with the ID, fetching the keys if they are old or
    /// don't contain it.
    async fn key(&self, kid: &str) -> Result<DecodingKey, VerifyError> {
        let mut keys = self.keys.lock().await;
        let refresh = keys.as_ref().is_none_or(|cached| {
            let age = cached.fetched.elapsed();
            age >= KEYS_MAX_AGE || (cached.set.find(kid).is_none() && age >= KEYS_MIN_AGE)
        });
        if refresh {
            *keys = Some(Keys {
                set: self.fetch().await?,
                fetched: Instant::now(),
            });
        }
        let key = keys
            .as_ref()
            .and_then(|cached| cached.set.find(kid))
            .map(DecodingKey::from_jwk);
        drop(keys);
        key.ok_or_else(|| VerifyError::UnknownKey(kid.to_owned()))?
            .map_err(VerifyError::Key)
    }

    /// Fetches the signing keys through OIDC discovery.
    async fn fetch(&self) -> Result<JwkSet, VerifyError> {
        let discovery = format!(
            "{}/.well-known/openid-configuration",
            self.url.trim_end_matches('/')
        );
        let discovery: Discovery = self.get(&discovery).await?;
        self.get(&discovery.jwks_uri).await
    }

    /// Fetches and parses a JSON document.
    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, VerifyError> {
        self.client
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|source| VerifyError::Fetch {
                url: url.to_owned(),
                source,
            })?
            .json()
            .await
            .map_err(|source| VerifyError::Fetch {
                url: url.to_owned(),
                source,
            })
    }
}

/// Errors from verifying an ID token.
#[derive(Debug, thiserror::Error)]
pub(crate) enum VerifyError {
    /// The token is signed with an algorithm that isn't accepted.
    #[error("the signature algorithm {0:?} isn't accepted")]
    Algorithm(Algorithm),
    /// The issuer's keys can't be fetched.
    #[error("failed to fetch {url}")]
    Fetch {
        /// The URL that failed.
        url: String,
        /// The error from the HTTP client.
        #[source]
        source: reqwest::Error,
    },
    /// The token is malformed, expired or not signed by the issuer.
    #[error("the token is invalid")]
    Invalid(#[source] JwtError),
    /// The issuer published a key that can't be used.
    #[error("the issuer's key can't be used")]
    Key(#[source] JwtError),
    /// The token header has no key ID.
    #[error("the token has no key ID")]
    MissingKeyId,
    /// The token doesn't match the issuer's claim patterns.
    #[error("the token's claims aren't allowed")]
    Rejected,
    /// The issuer doesn't publish the key that signed the token.
    #[error("the issuer has no key {0:?}")]
    UnknownKey(String),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testing::FakeIssuer;

    fn issuer(fake: &FakeIssuer, claims: &Value) -> Issuer {
        let config = serde_json::from_value(json!({
            "issuer": fake.url(),
            "audience": "granary",
            "claims": claims,
        }))
        .unwrap();
        Issuer::new(config, Client::new())
    }

    #[test]
    fn requires_claim_patterns() {
        for claims in [json!({}), Value::Null] {
            let mut config = json!({ "issuer": "https://forge", "audience": "granary" });
            if !claims.is_null() {
                config["claims"] = claims;
            }
            assert!(
                serde_json::from_value::<IssuerConfig>(config).is_err(),
                "issuers without claim patterns are refused"
            );
        }
    }

    #[tokio::test]
    async fn accepts_valid_tokens() {
        let fake = FakeIssuer::start().await;
        let issuer = issuer(&fake, &json!({ "repository_owner": ["inomotech"] }));
        let token = fake.token(&json!({ "aud": "granary", "repository_owner": "inomotech" }));
        let claims = issuer.verify(&token).await.unwrap();
        assert_eq!(
            claims["repository_owner"], "inomotech",
            "claims are returned"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_tokens() {
        let fake = FakeIssuer::start().await;
        let other = FakeIssuer::start().await;
        let issuer = issuer(&fake, &json!({ "repository_owner": ["inomotech"] }));
        let cases = [
            (
                fake.token(&json!({ "aud": "other", "repository_owner": "inomotech" })),
                "the audience differs",
            ),
            (
                fake.token(&json!({ "aud": "granary", "repository_owner": "evil" })),
                "the claims don't match",
            ),
            (
                fake.token(&json!({ "aud": "granary", "repository_owner": "inomotech", "exp": 1 })),
                "the token expired",
            ),
            (
                other.token(&json!({
                    "iss": fake.url(),
                    "aud": "granary",
                    "repository_owner": "inomotech",
                })),
                "another key signed it",
            ),
        ];
        for (token, reason) in cases {
            assert!(issuer.verify(&token).await.is_err(), "rejected: {reason}");
        }
    }
}
