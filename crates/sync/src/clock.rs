//! Version vectors.
//!
//! With a central server, "which version is current?" has an easy answer:
//! whatever the server says. There is no server here, so devices have to work
//! it out between themselves from what they each know.
//!
//! A version vector records, for each device, how many changes from that device
//! this version is aware of — `{laptop: 42, phone: 12}`. Comparing two vectors
//! answers the only question that matters: did one of these changes happen
//! *after* the other, or did they happen without either knowing about the
//! other?
//!
//! That second case is a conflict, and it cannot be resolved by looking at
//! clocks alone. See [`crate::resolve`].

use crate::device::DeviceId;
use std::collections::BTreeMap;

/// How two versions relate in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Causality {
    /// The same version.
    Equal,
    /// The left version happened before the right, which has seen it.
    Before,
    /// The left version happened after the right, and has seen it.
    After,
    /// Neither has seen the other. This is a conflict.
    Concurrent,
}

/// A per-device counter map.
///
/// A device missing from the map is treated as zero, so an empty vector is the
/// beginning of time and precedes everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionVector {
    // BTreeMap, not HashMap: encoding and comparison must be deterministic, and
    // identical state must always produce identical bytes.
    counters: BTreeMap<DeviceId, u64>,
}

impl VersionVector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a change made by `device`.
    pub fn increment(&mut self, device: DeviceId) {
        *self.counters.entry(device).or_insert(0) += 1;
    }

    pub fn get(&self, device: &DeviceId) -> u64 {
        self.counters.get(device).copied().unwrap_or(0)
    }

    pub fn set(&mut self, device: DeviceId, value: u64) {
        if value == 0 {
            self.counters.remove(&device);
        } else {
            self.counters.insert(device, value);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.counters.is_empty()
    }

    pub fn devices(&self) -> impl Iterator<Item = (&DeviceId, &u64)> {
        self.counters.iter()
    }

    /// Absorb everything the other vector knows, keeping the higher count for
    /// each device.
    ///
    /// Used when a device learns of a change: it now knows everything both
    /// versions knew. Also how a conflict resolution records that it has seen
    /// both sides, so the merged result supersedes each of them.
    pub fn merge(&mut self, other: &Self) {
        for (device, count) in &other.counters {
            let slot = self.counters.entry(*device).or_insert(0);
            if *count > *slot {
                *slot = *count;
            }
        }
    }

    pub fn merged(&self, other: &Self) -> Self {
        let mut out = self.clone();
        out.merge(other);
        out
    }

    /// Compare two vectors.
    ///
    /// One vector precedes another when every one of its counters is less than
    /// or equal to the other's. If that holds in neither direction, each knows
    /// something the other does not, and they are concurrent.
    pub fn compare(&self, other: &Self) -> Causality {
        let mut self_ahead = false;
        let mut other_ahead = false;

        for device in self.counters.keys().chain(other.counters.keys()) {
            let mine = self.get(device);
            let theirs = other.get(device);
            if mine > theirs {
                self_ahead = true;
            } else if theirs > mine {
                other_ahead = true;
            }
            if self_ahead && other_ahead {
                return Causality::Concurrent;
            }
        }

        match (self_ahead, other_ahead) {
            (false, false) => Causality::Equal,
            (true, false) => Causality::After,
            (false, true) => Causality::Before,
            (true, true) => Causality::Concurrent,
        }
    }

    /// Whether this version already accounts for the other.
    pub fn dominates(&self, other: &Self) -> bool {
        matches!(self.compare(other), Causality::After | Causality::Equal)
    }

    /// Encode for storage and for the wire.
    ///
    /// `[count: u32 LE][(device: 32 bytes, counter: u64 LE)]*`, in device
    /// order. Deterministic, so the same state always produces the same bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.counters.len() * 40);
        out.extend_from_slice(&(self.counters.len() as u32).to_le_bytes());
        for (device, counter) in &self.counters {
            out.extend_from_slice(device.as_bytes());
            out.extend_from_slice(&counter.to_le_bytes());
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 4 {
            return Err(DecodeError::Truncated);
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let expected = 4 + count * 40;
        if bytes.len() != expected {
            return Err(DecodeError::LengthMismatch { expected, actual: bytes.len() });
        }

        let mut counters = BTreeMap::new();
        for i in 0..count {
            let at = 4 + i * 40;
            let mut id = [0u8; 32];
            id.copy_from_slice(&bytes[at..at + 32]);
            let mut counter = [0u8; 8];
            counter.copy_from_slice(&bytes[at + 32..at + 40]);
            let counter = u64::from_le_bytes(counter);
            if counter > 0 {
                counters.insert(DeviceId::from_bytes(id), counter);
            }
        }
        Ok(Self { counters })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("version vector is truncated")]
    Truncated,
    #[error("version vector length mismatch: expected {expected} bytes, got {actual}")]
    LengthMismatch { expected: usize, actual: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: DeviceId = DeviceId::from_bytes([0xA1; 32]);
    const B: DeviceId = DeviceId::from_bytes([0xB2; 32]);
    const C: DeviceId = DeviceId::from_bytes([0xC3; 32]);

    fn vv(pairs: &[(DeviceId, u64)]) -> VersionVector {
        let mut v = VersionVector::new();
        for (d, n) in pairs {
            v.set(*d, *n);
        }
        v
    }

    #[test]
    fn a_fresh_vector_is_empty_and_reads_zero() {
        let v = VersionVector::new();
        assert!(v.is_empty());
        assert_eq!(v.get(&A), 0);
    }

    #[test]
    fn incrementing_counts_changes_per_device() {
        let mut v = VersionVector::new();
        v.increment(A);
        v.increment(A);
        v.increment(B);
        assert_eq!(v.get(&A), 2);
        assert_eq!(v.get(&B), 1);
        assert_eq!(v.get(&C), 0);
    }

    #[test]
    fn identical_vectors_are_equal() {
        assert_eq!(vv(&[(A, 3), (B, 1)]).compare(&vv(&[(A, 3), (B, 1)])), Causality::Equal);
    }

    #[test]
    fn an_empty_vector_precedes_everything() {
        let empty = VersionVector::new();
        assert_eq!(empty.compare(&vv(&[(A, 1)])), Causality::Before);
        assert_eq!(vv(&[(A, 1)]).compare(&empty), Causality::After);
        assert_eq!(empty.compare(&VersionVector::new()), Causality::Equal);
    }

    #[test]
    fn a_later_version_comes_after() {
        // The laptop made another change; the phone has not seen it.
        assert_eq!(vv(&[(A, 4), (B, 2)]).compare(&vv(&[(A, 3), (B, 2)])), Causality::After);
        assert_eq!(vv(&[(A, 3), (B, 2)]).compare(&vv(&[(A, 4), (B, 2)])), Causality::Before);
    }

    #[test]
    fn edits_neither_side_has_seen_are_concurrent() {
        // The laptop edited without seeing the phone's edit, and vice versa.
        // This is the case no clock can resolve.
        let laptop = vv(&[(A, 4), (B, 2)]);
        let phone = vv(&[(A, 3), (B, 3)]);
        assert_eq!(laptop.compare(&phone), Causality::Concurrent);
        assert_eq!(phone.compare(&laptop), Causality::Concurrent);
    }

    #[test]
    fn a_device_absent_from_one_side_still_counts() {
        // A vector that does not mention C treats it as zero, so knowing about
        // C at all puts you ahead.
        assert_eq!(vv(&[(A, 1), (C, 1)]).compare(&vv(&[(A, 1)])), Causality::After);
        assert_eq!(vv(&[(A, 2)]).compare(&vv(&[(A, 1), (C, 1)])), Causality::Concurrent);
    }

    #[test]
    fn comparison_is_antisymmetric() {
        let cases = [
            (vv(&[(A, 1)]), vv(&[(A, 2)])),
            (vv(&[(A, 1)]), vv(&[(B, 1)])),
            (vv(&[(A, 1)]), vv(&[(A, 1)])),
            (VersionVector::new(), vv(&[(C, 9)])),
        ];
        for (l, r) in cases {
            let expected = match l.compare(&r) {
                Causality::Before => Causality::After,
                Causality::After => Causality::Before,
                same => same,
            };
            assert_eq!(r.compare(&l), expected, "comparing {l:?} and {r:?}");
        }
    }

    #[test]
    fn merging_keeps_the_higher_count_for_each_device() {
        let mut merged = vv(&[(A, 4), (B, 1)]);
        merged.merge(&vv(&[(A, 2), (B, 7), (C, 1)]));
        assert_eq!(merged, vv(&[(A, 4), (B, 7), (C, 1)]));
    }

    #[test]
    fn a_merge_dominates_both_inputs() {
        // The property that makes conflict resolution terminate: the merged
        // vector supersedes both sides, so the resolution is not itself
        // concurrent with what it resolved.
        let laptop = vv(&[(A, 4), (B, 2)]);
        let phone = vv(&[(A, 3), (B, 3)]);
        let merged = laptop.merged(&phone);

        assert_eq!(merged.compare(&laptop), Causality::After);
        assert_eq!(merged.compare(&phone), Causality::After);
        assert!(merged.dominates(&laptop));
        assert!(merged.dominates(&phone));
    }

    #[test]
    fn merging_is_commutative() {
        let l = vv(&[(A, 4), (B, 2)]);
        let r = vv(&[(A, 1), (C, 9)]);
        assert_eq!(l.merged(&r), r.merged(&l));
    }

    #[test]
    fn dominating_covers_equal_and_after_but_not_concurrent() {
        let v = vv(&[(A, 2)]);
        assert!(v.dominates(&vv(&[(A, 2)])));
        assert!(v.dominates(&vv(&[(A, 1)])));
        assert!(!v.dominates(&vv(&[(A, 3)])));
        assert!(!v.dominates(&vv(&[(B, 1)])));
    }

    #[test]
    fn setting_zero_removes_a_device() {
        // Otherwise a vector carrying explicit zeroes would encode differently
        // from an equivalent one that simply omits them.
        let mut v = vv(&[(A, 1), (B, 2)]);
        v.set(B, 0);
        assert_eq!(v, vv(&[(A, 1)]));
        assert_eq!(v.encode(), vv(&[(A, 1)]).encode());
    }

    #[test]
    fn encoding_round_trips() {
        for v in [VersionVector::new(), vv(&[(A, 1)]), vv(&[(A, u64::MAX), (B, 2), (C, 3)])] {
            assert_eq!(VersionVector::decode(&v.encode()).unwrap(), v);
        }
    }

    #[test]
    fn encoding_is_deterministic_regardless_of_insertion_order() {
        let mut forwards = VersionVector::new();
        forwards.set(A, 1);
        forwards.set(B, 2);
        forwards.set(C, 3);

        let mut backwards = VersionVector::new();
        backwards.set(C, 3);
        backwards.set(B, 2);
        backwards.set(A, 1);

        assert_eq!(forwards.encode(), backwards.encode());
    }

    #[test]
    fn decoding_rejects_malformed_input() {
        assert_eq!(VersionVector::decode(&[1, 2]), Err(DecodeError::Truncated));
        let mut encoded = vv(&[(A, 1)]).encode();
        encoded.truncate(encoded.len() - 1);
        assert!(matches!(
            VersionVector::decode(&encoded),
            Err(DecodeError::LengthMismatch { .. })
        ));
    }
}
