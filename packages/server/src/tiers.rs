//! Tiers map ID token claims to the caches a CI job may write.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::cache::CacheName;
use crate::claims::Requirements;

/// A tier, as configured.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tier {
    /// The tier name, recorded in issued tokens.
    pub(crate) name: String,
    /// The caches that jobs in this tier may write.
    pub(crate) caches: Vec<CacheName>,
    /// Patterns that a token's claims must match to be in this tier.
    #[serde(default)]
    claims: Requirements,
}

/// The tiers in the order they are tried.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(transparent)]
pub(crate) struct Tiers(Vec<Tier>);

impl Tiers {
    /// Returns the first tier whose patterns the claims match.
    pub(crate) fn select(&self, claims: &Map<String, Value>) -> Option<&Tier> {
        self.0.iter().find(|tier| tier.claims.allow(claims))
    }

    /// Returns every tier.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Tier> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn first_match_wins() {
        let tiers: Tiers = serde_json::from_value(json!([
            { "name": "main", "caches": ["sstate-main"], "claims": { "ref": ["refs/heads/main"] } },
            { "name": "pr", "caches": ["sstate-pr"] },
        ]))
        .unwrap();
        let claims = |value: Value| value.as_object().unwrap().clone();
        let main = tiers.select(&claims(json!({ "ref": "refs/heads/main" })));
        assert_eq!(main.unwrap().name, "main", "main matches first");
        let pr = tiers.select(&claims(json!({ "ref": "refs/pull/1/merge" })));
        assert_eq!(pr.unwrap().name, "pr", "everything else is pr");
    }
}
