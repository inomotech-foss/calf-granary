//! Nix's base32 encoding.
//!
//! It uses its own alphabet and reads the bytes from the end, so it differs
//! from RFC 4648 base32.

/// The digits, without `e`, `o`, `u` and `t`.
const ALPHABET: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// Encodes bytes.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let bits = bytes.len().saturating_mul(8);
    let len = bits.div_ceil(5);
    (0..len)
        .rev()
        .map(|n| {
            let bit = n.saturating_mul(5);
            let (index, shift) = (bit / 8, bit % 8);
            let low = bytes.get(index).map_or(0, |byte| u16::from(*byte) >> shift);
            let high = bytes
                .get(index.saturating_add(1))
                .map_or(0, |byte| u16::from(*byte) << (8usize.saturating_sub(shift)));
            let digit = usize::from((low | high) & 0x1F);
            ALPHABET.get(digit).map_or('0', |digit| char::from(*digit))
        })
        .collect()
}

/// Decodes text into `len` bytes.
pub(crate) fn decode(text: &str, len: usize) -> Option<Vec<u8>> {
    if text.len() != len.saturating_mul(8).div_ceil(5) {
        return None;
    }
    let mut bytes = vec![0u8; len];
    for (n, character) in text.bytes().rev().enumerate() {
        let digit = ALPHABET.iter().position(|digit| *digit == character)?;
        let digit = u16::try_from(digit).ok()?;
        let bit = n.saturating_mul(5);
        let (index, shift) = (bit / 8, bit % 8);
        let value = digit << shift;
        let byte = bytes.get_mut(index)?;
        *byte |= u8::try_from(value & 0xFF).ok()?;
        let carry = u8::try_from(value >> 8).ok()?;
        match bytes.get_mut(index.saturating_add(1)) {
            Some(next) => *next |= carry,
            None if carry != 0 => return None,
            None => {}
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::digest::{SHA256, digest};

    use super::*;

    #[test]
    fn matches_nix() {
        let empty = digest(&SHA256, b"");
        assert_eq!(
            encode(empty.as_ref()),
            "0mdqa9w1p6cmli6976v4wi0sw9r4p5prkj7lzfd1877wk11c9c73",
            "the hash of nothing as Nix prints it"
        );
    }

    #[test]
    fn round_trips() {
        for len in [1, 5, 20, 32] {
            let bytes: Vec<u8> = (0..len)
                .map(|n| u8::try_from(n * 37 % 256).unwrap())
                .collect();
            assert_eq!(
                decode(&encode(&bytes), bytes.len()).as_deref(),
                Some(bytes.as_slice()),
                "{len} bytes survive"
            );
        }
    }

    #[test]
    fn rejects_invalid_text() {
        assert_eq!(decode("e", 1), None, "e isn't a digit");
        assert_eq!(decode("00", 2), None, "the length must match");
        assert_eq!(decode("zz", 1), None, "the value must fit");
    }
}
