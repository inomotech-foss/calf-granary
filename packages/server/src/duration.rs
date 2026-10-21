//! Durations in the configuration file.

use core::str::FromStr;
use core::time::Duration;

use serde::Deserialize;

/// A duration written as a whole number and a unit: `s`, `m`, `h` or `d`.
///
/// For example `90m` or `30d`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct ConfigDuration(Duration);

impl From<ConfigDuration> for Duration {
    fn from(duration: ConfigDuration) -> Self {
        duration.0
    }
}

impl FromStr for ConfigDuration {
    type Err = InvalidDuration;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidDuration {
            text: text.to_owned(),
        };
        let unit_start = text
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(invalid)?;
        let (number, unit) = text.split_at(unit_start);
        let number: u64 = number.parse().map_err(|_invalid| invalid())?;
        let seconds_per_unit = match unit {
            "s" => 1,
            "m" => 60,
            "h" => 60 * 60,
            "d" => 24 * 60 * 60,
            _ => return Err(invalid()),
        };
        let seconds = number.checked_mul(seconds_per_unit).ok_or_else(invalid)?;
        Ok(Self(Duration::from_secs(seconds)))
    }
}

impl TryFrom<String> for ConfigDuration {
    type Error = InvalidDuration;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

/// Text that isn't a duration.
#[derive(Debug, thiserror::Error)]
#[error("invalid duration {text:?}, expected a number and one of s, m, h or d")]
pub(crate) struct InvalidDuration {
    /// The rejected text.
    text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_units() {
        let cases = [
            ("45s", 45),
            ("90m", 90 * 60),
            ("12h", 12 * 60 * 60),
            ("30d", 30 * 24 * 60 * 60),
        ];
        for (text, seconds) in cases {
            let duration: Duration = text.parse::<ConfigDuration>().unwrap().into();
            assert_eq!(duration.as_secs(), seconds, "{text}");
        }
    }

    #[test]
    fn rejects_invalid_text() {
        for text in [
            "",
            "30",
            "d",
            "30w",
            "-1d",
            "1.5d",
            "30 d",
            "99999999999999999999d",
        ] {
            assert!(
                text.parse::<ConfigDuration>().is_err(),
                "{text:?} is invalid"
            );
        }
    }
}
