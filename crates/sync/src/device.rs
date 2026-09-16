//! Device identity.

use std::fmt;

/// A device's stable identity.
///
/// Eventually this is the fingerprint of the device's long-term Curve25519
/// public key, so identity and authentication are the same thing and a device
/// cannot claim to be another without holding its private key. Until key
/// management exists it is simply an opaque 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId([u8; 32]);

impl DeviceId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Short form for display and for conflict filenames.
    ///
    /// Eight hex characters is enough to tell a handful of a user's own devices
    /// apart in a filename. It is not enough to be secure against collision,
    /// and nothing may rely on it for identity.
    pub fn short(&self) -> String {
        self.0[..4].iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_form_is_eight_hex_characters() {
        let d = DeviceId::from_bytes([0xAB; 32]);
        assert_eq!(d.short(), "abababab");
    }

    #[test]
    fn display_is_the_full_identity() {
        let d = DeviceId::from_bytes([0x01; 32]);
        assert_eq!(d.to_string().len(), 64);
    }

    #[test]
    fn devices_order_deterministically() {
        // Ordering matters: version vectors are encoded in device order, and
        // an unstable order would make identical state encode differently.
        let a = DeviceId::from_bytes([1; 32]);
        let b = DeviceId::from_bytes([2; 32]);
        assert!(a < b);
    }
}
