//! Lowercase hexadecimal text.

/// Encodes bytes as lowercase hexadecimal.
pub(crate) fn encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0xF])
        .filter_map(|digit| char::from_digit(u32::from(digit), 16))
        .collect()
}

/// Decodes hexadecimal text in either case.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let digit = |byte: &u8| u8::try_from(char::from(*byte).to_digit(16)?).ok();
    text.as_bytes()
        .chunks(2)
        .map(|pair| match pair {
            [high, low] => Some((digit(high)? << 4) | digit(low)?),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_bytes() {
        assert_eq!(
            encode(&[0x00, 0x0F, 0xA5, 0xFF]),
            "000fa5ff",
            "two digits per byte"
        );
    }

    #[test]
    fn decodes_text() {
        assert_eq!(
            decode("000fA5ff"),
            Some(vec![0x00, 0x0F, 0xA5, 0xFF]),
            "both cases"
        );
        assert_eq!(decode("abc"), None, "odd length");
        assert_eq!(decode("zz"), None, "not hexadecimal");
        assert_eq!(decode("+1"), None, "signs aren't digits");
    }
}
