//! A device that goes away part-way leaves the rest of a plan for next time.
//!
//! On 2026-10-13 a phone frozen by its maker timed out in the middle of its
//! WhatsApp backup, and the laptop collecting it went on through the plan,
//! failing and logging 10,408 files at once (decision 0062). Each would have
//! failed the same way. What it should do is stop asking a device that is not
//! there, without calling that a failure, and take the rest at the next sync.

use qurb_engine::{ContentSource, Engine};
use qurb_storage::{ChunkKey, Store};
use qurb_sync::{Content, DeviceId, FileVersion, VersionVector};
use qurb_watcher::IgnoreRules;
use std::collections::HashMap;
use std::fs;

/// Supplies `before` files, then is gone, and must not be asked again.
struct Leaving {
    files: HashMap<[u8; 32], Vec<u8>>,
    before: usize,
}

impl ContentSource for Leaving {
    fn fetch(&mut self, hash: &[u8; 32], _size: u64) -> qurb_engine::Result<Vec<u8>> {
        assert!(self.before > 0, "asked a device that had gone");
        self.before -= 1;
        Ok(self.files[hash].clone())
    }

    fn gone(&self) -> bool {
        self.before == 0
    }
}

fn from_peer(path: &str, bytes: &[u8]) -> FileVersion {
    let peer = DeviceId::from_bytes([0xEE; 32]);
    let mut vector = VersionVector::new();
    vector.increment(peer);
    FileVersion {
        path: path.to_string(),
        content: Content::File { hash: *blake3::hash(bytes).as_bytes(), size: bytes.len() as u64 },
        vector,
        modified_by: peer,
        modified_at: 1_790_000_000,
        area: qurb_sync::Area::Shared,
    }
}

#[test]
fn a_device_that_goes_away_leaves_the_rest_for_next_time() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("folder");
    fs::create_dir_all(&root).unwrap();
    let store_dir = root.join(".qurb");
    let store = Store::open(&store_dir, ChunkKey::from_bytes([9; 32])).unwrap();
    let mut engine = Engine::new(root.clone(), store, IgnoreRules::new().with_store_dir(&store_dir));

    let contents: Vec<Vec<u8>> = (0..10).map(|i| format!("file {i}").into_bytes()).collect();
    let remote: Vec<FileVersion> =
        contents.iter().enumerate().map(|(i, bytes)| from_peer(&format!("{i}.txt"), bytes)).collect();
    let files: HashMap<[u8; 32], Vec<u8>> =
        contents.iter().map(|bytes| (*blake3::hash(bytes).as_bytes(), bytes.clone())).collect();

    let plan = engine.plan_with(&remote, None).unwrap();
    assert_eq!(plan.len(), 10);
    let stats = engine.apply_plan(&plan, &mut Leaving { files: files.clone(), before: 3 }).unwrap();
    assert_eq!(stats.adopted, 3);
    assert_eq!(stats.left, 7, "the rest left for next time");
    assert!(stats.failures.is_empty(), "not failures: {:?}", stats.failures);

    // The next sync plans what was left, and takes it.
    let plan = engine.plan_with(&remote, None).unwrap();
    assert_eq!(plan.len(), 7);
    let stats = engine.apply_plan(&plan, &mut Leaving { files, before: 7 }).unwrap();
    assert_eq!((stats.adopted, stats.left), (7, 0));
    for (i, bytes) in contents.iter().enumerate() {
        assert_eq!(&fs::read(root.join(format!("{i}.txt"))).unwrap(), bytes);
    }
}
