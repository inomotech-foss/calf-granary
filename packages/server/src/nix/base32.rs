//! Nix's base32 encoding.
//!
//! It uses its own alphabet and reads the bytes from the end, so it differs
//! from RFC 4648 base32.

use core::fmt::{self, Write as _};

/// The digits, without `e`, `o`, `u` and `t`.
const ALPHABET: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// Displays bytes in Nix's base32.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Base32<'a>(pub(crate) &'a [u8]);

impl fmt::Display for Base32<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        let len = bytes.len().saturating_mul(8).div_ceil(5);
        for n in (0..len).rev() {
            let bit = n.saturating_mul(5);
            let (index, shift) = (bit / 8, bit % 8);
            let low = bytes.get(index).map_or(0, |byte| u16::from(*byte) >> shift);
            let high = bytes
                .get(index.saturating_add(1))
                .map_or(0, |byte| u16::from(*byte) << (8usize.saturating_sub(shift)));
            let digit = usize::from((low | high) & 0x1F);
            f.write_char(ALPHABET.get(digit).map_or('0', |digit| char::from(*digit)))?;
        }
        Ok(())
    }
}

/// Decodes text into `N` bytes.
pub(crate) fn decode<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N.saturating_mul(8).div_ceil(5) {
        return None;
    }
    let mut bytes = [0u8; N];
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
    use core::array;

    use aws_lc_rs::digest::{SHA256, digest};

    use super::*;

    #[test]
    fn matches_nix() {
        let empty = digest(&SHA256, b"");
        assert_eq!(
            Base32(empty.as_ref()).to_string(),
            "0mdqa9w1p6cmli6976v4wi0sw9r4p5prkj7lzfd1877wk11c9c73",
            "the hash of nothing as Nix prints it"
        );
    }

    #[test]
    fn round_trips() {
        let long: [u8; 20] = array::from_fn(|n| u8::try_from(n * 37 % 256).unwrap());
        assert_eq!(
            decode(&Base32(&long).to_string()),
            Some(long),
            "the bytes survive"
        );
        let short = [7u8];
        assert_eq!(
            decode(&Base32(&short).to_string()),
            Some(short),
            "one byte survives"
        );
    }

    #[test]
    fn rejects_invalid_text() {
        assert_eq!(decode::<1>("e"), None, "e isn't a digit");
        assert_eq!(decode::<2>("00"), None, "the length must match");
        assert_eq!(decode::<1>("zz"), None, "the value must fit");
    }
}
