//! Hexadecimal text.

use core::fmt;

/// Displays bytes as lowercase hexadecimal.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hex<'a>(pub(crate) &'a [u8]);

impl fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Decodes hexadecimal text in either case into `out`, which must fit it
/// exactly.
pub(crate) fn decode_into(text: &str, out: &mut [u8]) -> Option<()> {
    if text.len() != out.len().checked_mul(2)? {
        return None;
    }
    let digit = |byte: u8| u8::try_from(char::from(byte).to_digit(16)?).ok();
    let (pairs, _rest) = text.as_bytes().as_chunks::<2>();
    for (byte, [high, low]) in out.iter_mut().zip(pairs) {
        *byte = (digit(*high)? << 4) | digit(*low)?;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_bytes() {
        assert_eq!(
            Hex(&[0x00, 0x0F, 0xA5, 0xFF]).to_string(),
            "000fa5ff",
            "two digits per byte"
        );
    }

    #[test]
    fn decodes_text() {
        let mut out = [0; 4];
        assert_eq!(decode_into("000fA5ff", &mut out), Some(()), "both cases");
        assert_eq!(out, [0x00, 0x0F, 0xA5, 0xFF], "the bytes");
        let mut two = [0; 2];
        assert_eq!(decode_into("abc", &mut two), None, "odd length");
        assert_eq!(decode_into("zzzz", &mut two), None, "not hexadecimal");
        assert_eq!(decode_into("+1+1", &mut two), None, "signs aren't digits");
    }
}
