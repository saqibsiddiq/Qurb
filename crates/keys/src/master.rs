//! The root secret, and the keys derived from it.

use crate::error::Result;
use crate::phrase::RecoveryPhrase;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Domain separation for the whole key hierarchy.
///
/// Bound into every derivation, so key material from this application can never
/// coincide with key material from anything else that happens to use HKDF over
/// the same secret.
const SALT: &[u8] = b"qurb/kdf/v1";

/// What a derived key is for.
///
/// An enum rather than a string the caller passes in. Two purposes accidentally
/// sharing an info string would silently produce the same key for both, which
/// is the kind of mistake that is invisible until it matters — and a typo
/// should not be able to cause it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Encrypting chunk payloads at rest.
    ChunkEncryption,
    /// This device's long-term network identity.
    DeviceIdentity,
    /// Authenticating metadata exchanged with peers.
    MetadataAuth,
    /// Beacons on the local network, so that devices can find each other with
    /// no server in the picture at all.
    LocalDiscovery,
}

impl Purpose {
    /// The HKDF info string. Versioned, so a future change to what a key
    /// protects can be a new label rather than a silent change of meaning.
    fn info(&self) -> &'static [u8] {
        match self {
            Purpose::ChunkEncryption => b"qurb/chunk-encryption/v1",
            Purpose::DeviceIdentity => b"qurb/device-identity/v1",
            Purpose::MetadataAuth => b"qurb/metadata-auth/v1",
            Purpose::LocalDiscovery => b"qurb/local-discovery/v1",
        }
    }
}

/// The root secret. Everything else in the system derives from it.
///
/// Losing this loses the data — not as a policy but as a fact, since nothing
/// else can decrypt the chunks. That is what makes the recovery phrase the
/// highest-stakes part of the product.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

/// A key derived for one purpose.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DerivedKey([u8; 32]);

impl MasterKey {
    /// Generate a fresh root secret.
    ///
    /// Uses the operating system's cryptographically secure generator via
    /// `rand::rngs::OsRng` — named explicitly rather than taken from
    /// `thread_rng`, because the difference between a CSPRNG and a fast PRNG is
    /// the entire security of the system and should not depend on a default.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derive the key for one purpose.
    ///
    /// HKDF-SHA256. One-way by construction, so a derived key that leaks
    /// reveals nothing about the master secret and nothing about the keys
    /// derived for other purposes.
    pub fn derive(&self, purpose: Purpose) -> DerivedKey {
        let hk = Hkdf::<Sha256>::new(Some(SALT), &self.0);
        let mut out = [0u8; 32];
        hk.expand(purpose.info(), &mut out)
            .expect("32 bytes is a valid HKDF output length for SHA-256");
        DerivedKey(out)
    }

    /// The 24 words that can rebuild this key.
    pub fn to_phrase(&self) -> RecoveryPhrase {
        RecoveryPhrase::from_entropy(&self.0)
    }

    /// Rebuild a key from its recovery phrase.
    pub fn from_phrase(phrase: &RecoveryPhrase) -> Result<Self> {
        Ok(Self(phrase.to_entropy()))
    }
}

impl DerivedKey {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Redacted, on both types.
///
/// A key that reaches a log file is a key that has leaked, and the usual way
/// that happens is a struct being printed for debugging.
impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}

impl std::fmt::Debug for DerivedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DerivedKey(<redacted>)")
    }
}

/// Constant-time comparison, so equality checks cannot be turned into a
/// byte-at-a-time oracle by timing them.
impl PartialEq for MasterKey {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for MasterKey {}

impl PartialEq for DerivedKey {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for DerivedKey {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_differ() {
        assert_ne!(MasterKey::generate(), MasterKey::generate());
    }

    #[test]
    fn derivation_is_deterministic() {
        let key = MasterKey::from_bytes([7; 32]);
        assert_eq!(key.derive(Purpose::ChunkEncryption), key.derive(Purpose::ChunkEncryption));
    }

    #[test]
    fn different_purposes_give_different_keys() {
        // If two purposes collided, compromising one would compromise the
        // other, and the whole point of deriving separately would be lost.
        let key = MasterKey::from_bytes([7; 32]);
        let chunk = key.derive(Purpose::ChunkEncryption);
        let identity = key.derive(Purpose::DeviceIdentity);
        let metadata = key.derive(Purpose::MetadataAuth);

        assert_ne!(chunk, identity);
        assert_ne!(chunk, metadata);
        assert_ne!(identity, metadata);
    }

    #[test]
    fn different_masters_give_different_derived_keys() {
        let a = MasterKey::from_bytes([1; 32]).derive(Purpose::ChunkEncryption);
        let b = MasterKey::from_bytes([2; 32]).derive(Purpose::ChunkEncryption);
        assert_ne!(a, b);
    }

    #[test]
    fn a_derived_key_does_not_reveal_the_master() {
        // Not a proof -- HKDF's one-wayness is the actual guarantee. This
        // catches the implementation mistake of returning the input.
        let key = MasterKey::from_bytes([9; 32]);
        assert_ne!(&key.derive(Purpose::ChunkEncryption).to_bytes(), key.as_bytes());
    }

    #[test]
    fn keys_do_not_print_themselves() {
        let key = MasterKey::from_bytes([0xAB; 32]);
        let shown = format!("{key:?}");
        assert!(!shown.contains("ab"), "a key leaked into its own Debug output: {shown}");
        assert!(format!("{:?}", key.derive(Purpose::ChunkEncryption)).contains("redacted"));
    }
}
