//! Who a device says it is, to a server that should not know.
//!
//! A rendezvous service has an unavoidable problem: it exists to introduce
//! devices to each other, so it necessarily learns that some set of addresses
//! belong together. What it does *not* have to learn is whose they are.
//!
//! Both identifiers here are derived from the user's master key, which only
//! their own devices hold. The server sees opaque 32-byte values it cannot link
//! to a person, an account, or a device's real identity — it matches them and
//! forwards, and that is all it is able to do.
//!
//! See ../../docs/decisions/0016-what-signalling-learns.md.

use qurb_keys::MasterKey;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Which group of devices this one belongs to.
///
/// Every device of one user derives the same value, so the server can match
/// them without being told they are related. It is a bearer secret: anyone
/// holding it can see that group's addresses, which is why it is derived from
/// the master key rather than being an account name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupId(#[serde(with = "hex_bytes")] [u8; 32]);

/// Which device within the group.
///
/// Derived from the master key *and* the device's fingerprint, so a peer that
/// already knows a fingerprint — which is what pairing establishes — can work
/// out where to look, while the server sees only an opaque value. It never
/// learns a real device identity, so it cannot recognise the same device
/// appearing under two groups.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemberId(#[serde(with = "hex_bytes")] [u8; 32]);

impl GroupId {
    pub fn derive(master: &MasterKey) -> Self {
        Self(derive(master, b"qurb/rendezvous-group/v1", None))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl MemberId {
    /// The identifier under which the device with this fingerprint announces.
    pub fn derive(master: &MasterKey, fingerprint: &[u8; 32]) -> Self {
        Self(derive(master, b"qurb/rendezvous-member/v1", Some(fingerprint)))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Both identifiers come from the key hierarchy rather than from a new secret,
/// so there is nothing extra for a user to back up or lose. The derivation is
/// one-way, so an identifier that leaks reveals nothing about the key.
fn derive(master: &MasterKey, label: &'static [u8], extra: Option<&[u8; 32]>) -> [u8; 32] {
    let base = master.derive(qurb_keys::Purpose::MetadataAuth);

    let mut hasher = blake3::Hasher::new_keyed(base.as_bytes());
    hasher.update(label);
    if let Some(extra) = extra {
        hasher.update(extra);
    }
    *hasher.finalize().as_bytes()
}

fn short(bytes: &[u8; 32]) -> String {
    bytes[..4].iter().map(|b| format!("{b:02x}")).collect()
}

impl fmt::Debug for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GroupId({})", short(&self.0))
    }
}

impl fmt::Debug for MemberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemberId({})", short(&self.0))
    }
}

/// Hex in the wire format, so a capture is readable while debugging without
/// anyone having to write a decoder.
mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let text = String::deserialize(d)?;
        if text.len() != 64 {
            return Err(serde::de::Error::custom("identifier must be 32 bytes of hex"));
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
                .map_err(|_| serde::de::Error::custom("identifier is not hex"))?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_device_of_one_user_derives_the_same_group() {
        // What lets the server match devices without being told they belong
        // together.
        let master = MasterKey::generate();
        assert_eq!(GroupId::derive(&master), GroupId::derive(&master));
    }

    #[test]
    fn different_users_land_in_different_groups() {
        assert_ne!(GroupId::derive(&MasterKey::generate()), GroupId::derive(&MasterKey::generate()));
    }

    #[test]
    fn a_member_id_depends_on_both_the_key_and_the_device() {
        let master = MasterKey::generate();
        let a = MemberId::derive(&master, &[1; 32]);
        let b = MemberId::derive(&master, &[2; 32]);
        assert_ne!(a, b, "two devices of one user shared an identifier");

        let other = MasterKey::generate();
        assert_ne!(
            MemberId::derive(&other, &[1; 32]),
            a,
            "the same device under two keys announced identically"
        );
    }

    #[test]
    fn a_peer_can_work_out_where_to_look() {
        // The property the whole scheme rests on: knowing a fingerprint, which
        // pairing establishes, is enough to compute where that device announces.
        let master = MasterKey::generate();
        let fingerprint = [0xAB; 32];

        let announced_as = MemberId::derive(&master, &fingerprint);
        let looked_up_as = MemberId::derive(&master, &fingerprint);
        assert_eq!(announced_as, looked_up_as);
    }

    #[test]
    fn the_group_is_not_the_member() {
        // Domain separation. Without distinct labels the two derivations could
        // collide and a group identifier would name a device.
        let master = MasterKey::generate();
        assert_ne!(GroupId::derive(&master).as_bytes(), MemberId::derive(&master, &[0; 32]).as_bytes());
    }

    #[test]
    fn identifiers_do_not_print_themselves_in_full() {
        let master = MasterKey::generate();
        let shown = format!("{:?}", GroupId::derive(&master));
        assert!(shown.len() < 24, "a rendezvous identifier leaked into a log line: {shown}");
    }

    #[test]
    fn identifiers_round_trip_through_the_wire_format() {
        let master = MasterKey::generate();
        let group = GroupId::derive(&master);
        let text = serde_json::to_string(&group).unwrap();
        assert_eq!(serde_json::from_str::<GroupId>(&text).unwrap(), group);

        for bad in ["\"\"", "\"zz\"", "\"abcd\""] {
            assert!(serde_json::from_str::<GroupId>(bad).is_err(), "accepted {bad}");
        }
    }
}
