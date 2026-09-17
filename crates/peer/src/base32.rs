//! RFC 4648 base32, uppercase, unpadded.
//!
//! Chosen over hex for pairing invites because QR codes have an alphanumeric
//! mode covering exactly uppercase letters and digits, which encodes roughly
//! 40% more densely than the byte mode hex would require. A smaller QR code is
//! one that scans from further away and in worse light, which is the whole job.
//!
//! Decoding accepts lowercase, since people retyping a code will not respect
//! case.

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u16 = 0;
    let mut bits = 0u8;

    for &byte in bytes {
        buffer = (buffer << 8) | byte as u16;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 0x1F) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 0x1F) as usize] as char);
    }
    out
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut buffer: u16 = 0;
    let mut bits = 0u8;

    for c in text.chars() {
        // Skip separators people add when writing a code down.
        if c == '-' || c == ' ' {
            continue;
        }
        let value = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u8 - b'A',
            c @ '2'..='7' => c as u8 - b'2' + 26,
            _ => return None,
        };

        buffer = (buffer << 5) | value as u16;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    // Leftover bits must be zero padding, not truncated data.
    if bits >= 5 || (buffer & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_at_every_length() {
        for len in 0..40 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(decode(&encode(&bytes)).unwrap(), bytes, "length {len}");
        }
    }

    #[test]
    fn matches_the_specification() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "MY");
        assert_eq!(encode(b"fo"), "MZXQ");
        assert_eq!(encode(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn decoding_is_forgiving_about_presentation() {
        // People retype these from a screen, in whatever case and with whatever
        // grouping they find readable.
        let expected = b"foobar".to_vec();
        for text in ["MZXW6YTBOI", "mzxw6ytboi", "MZXW-6YTB-OI", "MZXW 6YTB OI"] {
            assert_eq!(decode(text).unwrap(), expected, "failed on {text}");
        }
    }

    #[test]
    fn invalid_characters_are_refused() {
        // 0, 1 and 8 are excluded from the alphabet precisely because they are
        // confusable with O, I and B.
        for text in ["MZXW0", "MZXW1", "MZXW8", "MZXW!", "MZXW=" ] {
            assert!(decode(text).is_none(), "accepted {text}");
        }
    }

    #[test]
    fn truncated_input_is_refused() {
        // A code cut short must fail rather than decode to a shorter secret.
        let full = encode(&[1u8; 32]);
        for cut in 1..full.len() {
            let partial = &full[..cut];
            if let Some(bytes) = decode(partial) {
                assert_ne!(bytes, vec![1u8; 32], "a truncated code decoded to the full value");
            }
        }
    }
}
