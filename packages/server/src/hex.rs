//! Lowercase hexadecimal text.

/// Encodes bytes as lowercase hexadecimal.
pub(crate) fn encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0xF])
        .filter_map(|digit| char::from_digit(u32::from(digit), 16))
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
}
