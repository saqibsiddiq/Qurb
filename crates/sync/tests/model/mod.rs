//! A model of a device, for tests that need many of them.
//!
//! An in-memory stand-in for a real device: a map of paths, a clock, and the
//! ability to apply the actions [`qurb_sync::reconcile`] produces. It has no
//! files, no chunks and no network.
//!
//! What it does have is the real decision logic, which is the part that can be
//! wrong in ways nobody notices for months. `qurb-engine` has its own tests
//! against real storage; this one exists so that thousands of randomised
//! histories can be run in seconds.

#![allow(dead_code)]

use qurb_sync::{reconcile, Action, Content, DeviceId, FileVersion, VersionVector};
use std::collections::BTreeMap;

pub const A: DeviceId = DeviceId::from_bytes([0xA1; 32]);
pub const B: DeviceId = DeviceId::from_bytes([0xB2; 32]);
pub const C: DeviceId = DeviceId::from_bytes([0xC3; 32]);

/// One device's whole view of the world.
#[derive(Debug, Clone)]
pub struct Device {
    pub id: DeviceId,
    pub files: BTreeMap<String, FileVersion>,
    pub clock: VersionVector,
    /// How wrong this device's wall clock is, in seconds.
    ///
    /// Ordering never consults it — that is the version vector's job. It exists
    /// so tests can prove that, by setting it absurdly and checking nothing
    /// changes.
    pub skew: i64,
}

impl Device {
    pub fn new(id: DeviceId) -> Self {
        Self { id, files: BTreeMap::new(), clock: VersionVector::new(), skew: 0 }
    }

    /// A device whose clock is wrong by `seconds`.
    pub fn with_skew(id: DeviceId, seconds: i64) -> Self {
        Self { skew: seconds, ..Self::new(id) }
    }

    fn now(&self) -> i64 {
        1_757_462_400_i64.saturating_add(self.skew)
    }

    /// Make a local change: write `hash` to `path`.
    pub fn edit(&mut self, path: &str, hash: u8) {
        self.clock.increment(self.id);
        let mut vector = self.files.get(path).map(|f| f.vector.clone()).unwrap_or_default();
        vector.set(self.id, self.clock.get(&self.id));
        self.files.insert(
            path.to_string(),
            FileVersion::file(path, [hash; 32], 100, vector, self.id, self.now()),
        );
    }

    pub fn delete(&mut self, path: &str) {
        if !self.files.contains_key(path) {
            return;
        }
        self.clock.increment(self.id);
        let mut vector = self.files[path].vector.clone();
        vector.set(self.id, self.clock.get(&self.id));
        self.files.insert(
            path.to_string(),
            FileVersion::tombstone(path, vector, self.id, self.now()),
        );
    }

    /// Apply everything this device should do after comparing with a peer.
    ///
    /// `Offer` is the peer's job, so it is ignored here; the peer sees the
    /// mirror-image `Adopt` when it reconciles.
    pub fn apply(&mut self, actions: &[Action]) {
        for action in actions {
            match action {
                Action::Offer { .. } => {}
                Action::Adopt { remote } => {
                    self.absorb(remote.clone());
                }
                Action::Conflict { keeps_path, renamed } => {
                    self.absorb(keeps_path.clone());
                    self.absorb(renamed.clone());
                }
                Action::Resurrect { resolved } | Action::Merge { resolved } => {
                    self.absorb(resolved.clone());
                }
            }
        }
    }

    pub fn absorb(&mut self, version: FileVersion) {
        self.clock.merge(&version.vector);
        self.files.insert(version.path.clone(), version);
    }

    pub fn versions(&self) -> Vec<FileVersion> {
        self.files.values().cloned().collect()
    }

    /// What a user would see: live paths and their contents.
    pub fn visible(&self) -> BTreeMap<String, [u8; 32]> {
        self.files
            .values()
            .filter_map(|f| match &f.content {
                Content::File { hash, .. } => Some((f.path.clone(), *hash)),
                Content::Deleted => None,
            })
            .collect()
    }
}

/// One exchange in both directions.
pub fn sync_once(a: &mut Device, b: &mut Device) {
    let a_actions = reconcile(&a.versions(), &b.versions());
    let b_actions = reconcile(&b.versions(), &a.versions());
    a.apply(&a_actions);
    b.apply(&b_actions);
}

/// Exchange until nothing changes, failing if that does not happen quickly.
///
/// The bound is the point of this helper. A sync engine that needs unbounded
/// rounds to settle is one that does not settle at all, and in production that
/// looks like two devices trading files forever while the user watches their
/// battery drain.
pub fn sync_until_stable(a: &mut Device, b: &mut Device) -> usize {
    for round in 1..=10 {
        let before = (a.files.clone(), b.files.clone());
        sync_once(a, b);
        if (a.files.clone(), b.files.clone()) == before {
            return round;
        }
    }
    panic!("did not converge within 10 rounds");
}

pub fn assert_converged(a: &Device, b: &Device) {
    assert_eq!(a.visible(), b.visible(), "devices disagree about what the user has");
    assert_eq!(a.files, b.files, "devices disagree about history, so they will diverge later");
}

