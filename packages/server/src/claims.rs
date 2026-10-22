//! Matching ID token claims against configured patterns.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

/// A claim value pattern in which `*` matches any sequence of characters.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub(crate) struct Pattern(String);

impl Pattern {
    /// Checks whether the whole value matches the pattern.
    fn matches(&self, value: &str) -> bool {
        let Some((head, last)) = self.0.rsplit_once('*') else {
            return value == self.0;
        };
        let mut parts = head.split('*');
        let first = parts.next().unwrap_or_default();
        let Some(mut rest) = value.strip_prefix(first) else {
            return false;
        };
        for part in parts {
            match rest.split_once(part) {
                Some((_, after)) => rest = after,
                None => return false,
            }
        }
        rest.ends_with(last)
    }
}

/// Patterns that the claims of an ID token must match.
///
/// Every listed claim must be present and match at least one of its
/// patterns. Strings, booleans and numbers are compared in their JSON text
/// form without quotes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub(crate) struct Requirements(BTreeMap<String, Vec<Pattern>>);

impl Requirements {
    /// Whether there are no requirements, so every token is allowed.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Checks whether the claims meet every requirement.
    pub(crate) fn allow(&self, claims: &Map<String, Value>) -> bool {
        self.0.iter().all(|(name, patterns)| {
            let value = match claims.get(name) {
                Some(Value::String(value)) => Cow::Borrowed(value.as_str()),
                Some(value @ (Value::Bool(_) | Value::Number(_))) => Cow::Owned(value.to_string()),
                Some(Value::Array(_) | Value::Object(_) | Value::Null) | None => return false,
            };
            patterns.iter().any(|pattern| pattern.matches(&value))
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pattern(text: &str) -> Pattern {
        Pattern(text.to_owned())
    }

    #[test]
    fn patterns() {
        let cases = [
            ("refs/heads/main", "refs/heads/main", true),
            ("refs/heads/main", "refs/heads/main2", false),
            ("refs/tags/v*", "refs/tags/v1.2.3", true),
            ("refs/tags/v*", "refs/heads/v1", false),
            ("*", "", true),
            ("*", "anything", true),
            ("refs/*/release-*", "refs/heads/release-1", true),
            ("refs/*/release-*", "refs/heads/main", false),
            ("a*a", "a", false),
            ("a*a", "aa", true),
            ("*-pr", "x-pr", true),
        ];
        for (text, value, expected) in cases {
            assert_eq!(
                pattern(text).matches(value),
                expected,
                "{text} against {value}"
            );
        }
    }

    #[test]
    fn requirements() {
        let requirements: Requirements = serde_json::from_value(json!({
            "repository_owner": ["inomotech"],
            "ref": ["refs/heads/main", "refs/tags/v*"],
            "ref_protected": ["true"],
        }))
        .unwrap();
        let claims = |value: Value| value.as_object().unwrap().clone();

        assert!(
            requirements.allow(&claims(json!({
                "repository_owner": "inomotech",
                "ref": "refs/tags/v1",
                "ref_protected": true,
            }))),
            "every claim matches"
        );
        assert!(
            !requirements.allow(&claims(json!({
                "repository_owner": "inomotech",
                "ref": "refs/heads/feature",
                "ref_protected": "true",
            }))),
            "the ref doesn't match"
        );
        assert!(
            !requirements.allow(&claims(json!({
                "ref": "refs/heads/main",
                "ref_protected": "true",
            }))),
            "a missing claim doesn't match"
        );
        assert!(
            Requirements::default().allow(&claims(json!({}))),
            "no requirements match everything"
        );
    }
}
