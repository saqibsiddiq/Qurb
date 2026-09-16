//! The 24 words that are the only way back.
//!
//! BIP-39, via the `bip39` crate rather than hand-rolled. The encoding itself
//! is simple, but the wordlist is not: it is chosen so that four letters
//! identify any word, similar words are avoided, and it sorts usefully. Getting
//! that wrong produces a phrase people mis-transcribe, which for a
//! zero-knowledge product means losing their data.
//!
//! # Why this exists at all
//!
//! The servers hold no key, so nobody can reset a forgotten one. These words
//! are not a backup of the key — they *are* the key, in a form a person can
//! write on paper. Everything about how they are presented follows from that.

use crate::error::{Error, Result};
use bip39::{Language, Mnemonic};
use zeroize::Zeroize;

/// A 24-word recovery phrase.
///
/// 256 bits of entropy plus an 8-bit checksum, which is what makes 24 words
/// rather than 23. The checksum catches a mistyped or misremembered word
/// instead of silently producing a different key and an empty account.
#[derive(Clone)]
pub struct RecoveryPhrase {
    words: Vec<String>,
}

impl RecoveryPhrase {
    pub(crate) fn from_entropy(entropy: &[u8; 32]) -> Self {
        let mnemonic = Mnemonic::from_entropy_in(Language::English, entropy)
            .expect("32 bytes is a valid BIP-39 entropy length");
        Self { words: mnemonic.words().map(|w| w.to_string()).collect() }
    }

    pub(crate) fn to_entropy(&self) -> [u8; 32] {
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, &self.to_string())
            .expect("a constructed phrase is always valid");
        let (entropy, len) = mnemonic.to_entropy_array();
        debug_assert_eq!(len, 32);
        let mut out = [0u8; 32];
        out.copy_from_slice(&entropy[..32]);
        out
    }

    /// Parse a phrase a person typed.
    ///
    /// Tolerant about presentation and strict about content: any run of
    /// whitespace separates words, case is ignored, and surrounding space is
    /// dropped — but the checksum must be right and the words must be real.
    /// People copy these off paper, across line breaks, with a trailing space.
    pub fn parse(input: &str) -> Result<Self> {
        let normalised = input.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();

        let mnemonic = Mnemonic::parse_in_normalized(Language::English, &normalised)
            .map_err(|e| Error::BadPhrase { detail: e.to_string() })?;

        let count = mnemonic.word_count();
        if count != 24 {
            return Err(Error::BadPhrase {
                detail: format!("expected 24 words, found {count}"),
            });
        }

        Ok(Self { words: mnemonic.words().map(|w| w.to_string()).collect() })
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// The phrase laid out for someone to copy down.
    ///
    /// Numbered and in columns, because the realistic failure is a person
    /// losing their place halfway through writing 24 words on paper.
    pub fn numbered(&self) -> String {
        let rows = self.words.len().div_ceil(3);
        let mut out = String::new();
        for row in 0..rows {
            for column in 0..3 {
                let i = row + column * rows;
                if let Some(word) = self.words.get(i) {
                    out.push_str(&format!("{:>3}. {:<12}", i + 1, word));
                }
            }
            out.push('\n');
        }
        out
    }
}

impl std::fmt::Display for RecoveryPhrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.words.join(" "))
    }
}

/// Never printed by accident.
///
/// `Display` exists because the phrase genuinely has to be shown once. `Debug`
/// must not, or it reaches a log line the first time a struct containing it is
/// printed during debugging.
impl std::fmt::Debug for RecoveryPhrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryPhrase(<redacted>)")
    }
}

impl Drop for RecoveryPhrase {
    fn drop(&mut self) {
        for word in &mut self.words {
            word.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master::MasterKey;

    #[test]
    fn a_phrase_is_twenty_four_words() {
        let phrase = MasterKey::generate().to_phrase();
        assert_eq!(phrase.words().len(), 24);
    }

    #[test]
    fn a_key_round_trips_through_its_phrase() {
        // The property the entire recovery story rests on.
        for _ in 0..50 {
            let key = MasterKey::generate();
            let phrase = key.to_phrase();
            let parsed = RecoveryPhrase::parse(&phrase.to_string()).unwrap();
            assert_eq!(MasterKey::from_phrase(&parsed).unwrap(), key);
        }
    }

    #[test]
    fn a_known_phrase_gives_a_known_key() {
        // Pinned against the BIP-39 specification's own test vector, so a
        // change of library or encoding cannot silently change what a phrase
        // means. If this breaks, every existing user's phrase stops working.
        let phrase = RecoveryPhrase::parse(
            "legal winner thank year wave sausage worth useful legal winner thank year \
             wave sausage worth useful legal will",
        );
        assert!(phrase.is_err(), "18 words must be refused");

        let all_zero = MasterKey::from_bytes([0; 32]);
        assert_eq!(
            all_zero.to_phrase().to_string(),
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon art"
        );
    }

    #[test]
    fn parsing_tolerates_how_people_actually_type() {
        let key = MasterKey::generate();
        let phrase = key.to_phrase().to_string();

        let messy = format!("  {}  ", phrase.replace(' ', "\n  ").to_uppercase());
        let parsed = RecoveryPhrase::parse(&messy).unwrap();
        assert_eq!(MasterKey::from_phrase(&parsed).unwrap(), key);
    }

    #[test]
    fn a_wrong_word_is_caught() {
        let phrase = MasterKey::generate().to_phrase().to_string();
        let mut words: Vec<&str> = phrase.split(' ').collect();
        words[5] = "zebra";
        assert!(RecoveryPhrase::parse(&words.join(" ")).is_err());
    }

    #[test]
    fn a_swapped_pair_is_caught_by_the_checksum() {
        // The realistic transcription error: two words written in the wrong
        // order. Without a checksum this would quietly produce a different key
        // and an account that appears empty.
        let phrase = MasterKey::generate().to_phrase().to_string();
        let mut words: Vec<&str> = phrase.split(' ').collect();
        words.swap(3, 4);
        let swapped = words.join(" ");

        if swapped == phrase {
            return; // the two words happened to be identical
        }
        assert!(
            RecoveryPhrase::parse(&swapped).is_err(),
            "a swapped pair passed the checksum"
        );
    }

    #[test]
    fn nonsense_is_refused() {
        for input in ["", "hello world", "   ", "abandon", &"abandon ".repeat(24)] {
            assert!(RecoveryPhrase::parse(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn the_phrase_does_not_print_itself_when_debugged() {
        let phrase = MasterKey::generate().to_phrase();
        assert_eq!(format!("{phrase:?}"), "RecoveryPhrase(<redacted>)");
        assert!(!format!("{phrase:?}").contains(&phrase.words()[0]));
    }

    #[test]
    fn the_numbered_layout_lists_every_word_once() {
        let phrase = MasterKey::generate().to_phrase();
        let rendered = phrase.numbered();
        for (i, word) in phrase.words().iter().enumerate() {
            assert!(rendered.contains(&format!("{:>3}. {word}", i + 1)), "missing word {}", i + 1);
        }
    }
}
