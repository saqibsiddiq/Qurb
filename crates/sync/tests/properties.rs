//! Property-based tests.
//!
//! The hand-written tests check cases someone thought of. These check
//! *properties* over inputs nobody chose — thousands of randomised histories
//! per run, with shrinking, so a failure arrives as the smallest sequence that
//! still breaks it rather than as a 30-step transcript.
//!
//! This is the Phase 2 opener. The bug that motivated it is instructive: the
//! Phase 1 convergence test (two devices reaching identical content
//! independently, and their histories then never merging) passed every unit
//! test and was found only by simulating whole histories. That was a
//! hand-rolled loop over 200 fixed seeds. This replaces it with a generator
//! that shrinks.

mod model;

use model::{Device, A, B, C};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use qurb_sync::{reconcile, Causality, Content, DeviceId, FileVersion, VersionVector};

// ---------------------------------------------------------------------------
// Version vectors: the algebra everything else rests on.
// ---------------------------------------------------------------------------

/// A failing case is written here and replayed on every later run, so a rare
/// counterexample becomes a permanent regression test rather than something
/// that shows up once and is never seen again.
fn config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/proptest-regressions.txt",
        ))),
        ..ProptestConfig::default()
    }
}

/// One count per device, so a device never appears twice and the entries are
/// unique by construction.
fn arb_entries() -> impl Strategy<Value = Vec<(usize, u64)>> {
    prop::collection::vec(prop::option::of(1u64..6), 4).prop_map(|counts| {
        counts.into_iter().enumerate().filter_map(|(i, c)| c.map(|c| (i, c))).collect()
    })
}

fn build(entries: &[(usize, u64)]) -> VersionVector {
    let mut v = VersionVector::new();
    for (device, count) in entries {
        v.set(DeviceId::from_bytes([*device as u8; 32]), *count);
    }
    v
}

/// Vectors over a small device set, so orderings and collisions actually occur.
/// Drawing from 2^256 device ids would make every pair trivially concurrent and
/// test nothing.
fn arb_vector() -> impl Strategy<Value = VersionVector> {
    prop::collection::vec((0usize..4, 0u64..6), 0..5).prop_map(|entries| {
        let mut v = VersionVector::new();
        for (device, count) in entries {
            v.set(DeviceId::from_bytes([device as u8; 32]), count);
        }
        v
    })
}

proptest! {
    #![proptest_config(config(256))]

    #[test]
    fn a_vector_equals_itself(v in arb_vector()) {
        prop_assert_eq!(v.compare(&v), Causality::Equal);
    }

    /// Comparison must be antisymmetric, or two devices looking at the same
    /// pair of versions would disagree about which came first.
    #[test]
    fn comparison_is_antisymmetric(l in arb_vector(), r in arb_vector()) {
        let expected = match l.compare(&r) {
            Causality::Before => Causality::After,
            Causality::After => Causality::Before,
            same => same,
        };
        prop_assert_eq!(r.compare(&l), expected);
    }

    #[test]
    fn merge_is_commutative(l in arb_vector(), r in arb_vector()) {
        prop_assert_eq!(l.merged(&r), r.merged(&l));
    }

    #[test]
    fn merge_is_associative(a in arb_vector(), b in arb_vector(), c in arb_vector()) {
        prop_assert_eq!(a.merged(&b).merged(&c), a.merged(&b.merged(&c)));
    }

    #[test]
    fn merge_is_idempotent(v in arb_vector()) {
        prop_assert_eq!(v.merged(&v), v.clone());
    }

    /// The property that makes conflict resolution terminate: a merge is
    /// strictly later than what it merged, so the resolution is not itself
    /// concurrent with the versions it resolved.
    #[test]
    fn a_merge_dominates_both_inputs(l in arb_vector(), r in arb_vector()) {
        let merged = l.merged(&r);
        prop_assert!(merged.dominates(&l));
        prop_assert!(merged.dominates(&r));
    }

    /// Absorbing something you have already seen changes nothing. Without this,
    /// re-receiving a version would advance the clock and manufacture
    /// concurrency out of nothing.
    #[test]
    fn merging_an_older_vector_changes_nothing(l in arb_vector(), r in arb_vector()) {
        prop_assume!(l.compare(&r) == Causality::After);
        prop_assert_eq!(l.merged(&r), l);
    }

    #[test]
    fn incrementing_moves_a_vector_strictly_forward(v in arb_vector(), d in 0usize..4) {
        let mut next = v.clone();
        next.increment(DeviceId::from_bytes([d as u8; 32]));
        prop_assert_eq!(next.compare(&v), Causality::After);
    }

    #[test]
    fn encoding_round_trips(v in arb_vector()) {
        prop_assert_eq!(VersionVector::decode(&v.encode()).unwrap(), v);
    }

    /// Equal vectors must encode identically however they were built, or the
    /// same state would produce different bytes and peers would see differences
    /// that are not there.
    #[test]
    fn equal_vectors_encode_identically(entries in arb_entries()) {
        let forwards = build(&entries);
        let mut reversed = entries.clone();
        reversed.reverse();
        let backwards = build(&reversed);

        prop_assert_eq!(&forwards, &backwards);
        prop_assert_eq!(forwards.encode(), backwards.encode());
    }
}

// ---------------------------------------------------------------------------
// Resolution: what happens when two versions of one path meet.
// ---------------------------------------------------------------------------

fn arb_version(path: &'static str) -> impl Strategy<Value = FileVersion> {
    (arb_vector(), 0u8..4, 0usize..3, any::<bool>()).prop_map(
        move |(vector, content, device, deleted)| {
            let by = DeviceId::from_bytes([device as u8; 32]);
            if deleted {
                FileVersion::tombstone(path, vector, by, 1_757_462_400)
            } else {
                FileVersion::file(path, [content; 32], 100, vector, by, 1_757_462_400)
            }
        },
    )
}

proptest! {
    #![proptest_config(config(256))]

    /// Both devices must reach the same answer from the same pair, with no
    /// negotiation. If they disagreed they would each rename the other's copy
    /// and end up with two conflict files and no original.
    #[test]
    fn resolution_is_symmetric(
        l in arb_version("f.txt"),
        r in arb_version("f.txt"),
    ) {
        let from_left = qurb_sync::resolve(&l, &r);
        let from_right = qurb_sync::resolve(&r, &l);

        prop_assert_eq!(&from_left.vector, &from_right.vector,
            "the two sides disagreed about the resolved history");

        use qurb_sync::Outcome::*;
        let mirrored = match (&from_left.outcome, &from_right.outcome) {
            (InSync, InSync) | (Merge, Merge) => true,
            (KeepLocal, TakeRemote) | (TakeRemote, KeepLocal) => true,
            (Conflict { renamed_to: a, .. }, Conflict { renamed_to: b, .. }) => a == b,
            (Resurrect { .. }, Resurrect { .. }) => true,
            _ => false,
        };
        prop_assert!(mirrored, "asymmetric: {:?} vs {:?}", from_left.outcome, from_right.outcome);
    }

    /// Every resolution must supersede what it resolved, or the same
    /// disagreement is rediscovered on the next exchange, forever.
    #[test]
    fn a_resolution_dominates_both_sides(
        l in arb_version("f.txt"),
        r in arb_version("f.txt"),
    ) {
        let resolved = qurb_sync::resolve(&l, &r);
        prop_assert!(resolved.vector.dominates(&l.vector));
        prop_assert!(resolved.vector.dominates(&r.vector));
    }

    /// Identical content is never a conflict, however the histories look.
    /// Producing a conflict file for two copies of the same bytes would be
    /// pure noise the user has to clean up.
    #[test]
    fn identical_content_never_conflicts(
        l in arb_version("f.txt"),
        r in arb_version("f.txt"),
    ) {
        prop_assume!(l.content == r.content);
        let outcome = qurb_sync::resolve(&l, &r).outcome;
        let conflicted = matches!(outcome, qurb_sync::Outcome::Conflict { .. });
        prop_assert!(!conflicted, "identical content produced a conflict: {:?}", outcome);
    }

    /// Decision 0005's promise, as a property rather than an example: whenever
    /// a resolution is a conflict, both contents survive it.
    #[test]
    fn a_conflict_keeps_both_contents(
        l in arb_version("f.txt"),
        r in arb_version("f.txt"),
    ) {
        let actions = reconcile(std::slice::from_ref(&l), std::slice::from_ref(&r));
        for action in actions {
            if let qurb_sync::Action::Conflict { keeps_path, renamed } = action {
                let kept = [&keeps_path.content, &renamed.content];
                prop_assert!(kept.contains(&&l.content), "the local edit was dropped");
                prop_assert!(kept.contains(&&r.content), "the remote edit was dropped");
                prop_assert_ne!(keeps_path.path, renamed.path, "both written to one path");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Convergence over whole randomised histories.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Op {
    Edit { device: usize, path: usize, content: u8 },
    Delete { device: usize, path: usize },
    Sync { left: usize, right: usize },
}

const PATHS: [&str; 4] = ["a.txt", "b.txt", "dir/c.txt", "notes.md"];

fn arb_ops(devices: usize) -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        // Weighted towards edits, so histories are mostly real work with syncs
        // interleaved at unpredictable points.
        4 => (0..devices, 0..PATHS.len(), 0u8..4)
            .prop_map(|(device, path, content)| Op::Edit { device, path, content }),
        2 => (0..devices, 0..PATHS.len())
            .prop_map(|(device, path)| Op::Delete { device, path }),
        3 => (0..devices, 0..devices)
            .prop_map(|(left, right)| Op::Sync { left, right }),
    ];
    prop::collection::vec(op, 0..40)
}

/// Run a history, then sync everyone until nothing changes.
///
/// Returns the devices and how many rounds settling took, so a test can assert
/// that it settles *at all* — an engine needing unbounded rounds does not
/// converge, and in production that is two devices trading files forever.
fn run(ops: &[Op], count: usize) -> Result<(Vec<Device>, usize), String> {
    let ids = [A, B, C];
    let mut devices: Vec<Device> = (0..count).map(|i| Device::new(ids[i])).collect();

    for op in ops {
        match *op {
            Op::Edit { device, path, content } => {
                devices[device].edit(PATHS[path], content + 1);
            }
            Op::Delete { device, path } => devices[device].delete(PATHS[path]),
            Op::Sync { left, right } if left != right => {
                let (lo, hi) = (left.min(right), left.max(right));
                let (head, tail) = devices.split_at_mut(hi);
                model::sync_once(&mut head[lo], &mut tail[0]);
            }
            Op::Sync { .. } => {}
        }
    }

    for round in 1..=12 {
        let before: Vec<_> = devices.iter().map(|d| d.files.clone()).collect();
        for i in 0..devices.len() {
            for j in (i + 1)..devices.len() {
                let (head, tail) = devices.split_at_mut(j);
                model::sync_once(&mut head[i], &mut tail[0]);
            }
        }
        if before == devices.iter().map(|d| d.files.clone()).collect::<Vec<_>>() {
            return Ok((devices, round));
        }
    }
    Err("did not settle within 12 rounds".into())
}

proptest! {
    #![proptest_config(config(400))]

    /// The property the whole system exists to provide.
    #[test]
    fn any_history_converges_on_two_devices(ops in arb_ops(2)) {
        let (devices, _) = run(&ops, 2).map_err(TestCaseError::fail)?;
        for d in &devices[1..] {
            prop_assert_eq!(&d.visible(), &devices[0].visible(), "user-visible state diverged");
            prop_assert_eq!(&d.files, &devices[0].files, "recorded history diverged");
        }
    }

    /// Three devices, where a change can arrive by two routes and "has this
    /// device seen that edit?" stops having an obvious answer. Two devices can
    /// be reconciled by much simpler schemes than vector clocks; three is where
    /// they earn their complexity.
    #[test]
    fn any_history_converges_on_three_devices(ops in arb_ops(3)) {
        let (devices, _) = run(&ops, 3).map_err(TestCaseError::fail)?;
        for d in &devices[1..] {
            prop_assert_eq!(&d.visible(), &devices[0].visible(), "user-visible state diverged");
            prop_assert_eq!(&d.files, &devices[0].files, "recorded history diverged");
        }
    }

    /// Settling must not take unboundedly many exchanges.
    #[test]
    fn convergence_is_quick(ops in arb_ops(3)) {
        let (_, rounds) = run(&ops, 3).map_err(TestCaseError::fail)?;
        prop_assert!(rounds <= 4, "took {} rounds to settle", rounds);
    }

    /// Once settled, there is nothing left to do. A plan that is non-empty
    /// after convergence means devices would keep exchanging forever.
    #[test]
    fn no_work_remains_after_convergence(ops in arb_ops(3)) {
        let (devices, _) = run(&ops, 3).map_err(TestCaseError::fail)?;
        for i in 0..devices.len() {
            for j in 0..devices.len() {
                if i == j { continue }
                let plan = reconcile(&devices[i].versions(), &devices[j].versions());
                prop_assert!(plan.is_empty(), "work remained: {:?}", plan.first());
            }
        }
    }

    /// Syncing again after convergence changes nothing. Delivery elsewhere in
    /// the system is at-least-once, so repeated syncs are normal.
    #[test]
    fn syncing_is_idempotent(ops in arb_ops(3)) {
        let (mut devices, _) = run(&ops, 3).map_err(TestCaseError::fail)?;
        let settled: Vec<_> = devices.iter().map(|d| d.files.clone()).collect();

        for _ in 0..3 {
            for i in 0..devices.len() {
                for j in (i + 1)..devices.len() {
                    let (head, tail) = devices.split_at_mut(j);
                    model::sync_once(&mut head[i], &mut tail[0]);
                }
            }
        }
        let after: Vec<_> = devices.iter().map(|d| d.files.clone()).collect();
        prop_assert_eq!(after, settled, "repeated syncing changed settled state");
    }

    /// Nothing may appear that nobody wrote. Catches a whole class of bug in
    /// which resolution invents, duplicates or corrupts content.
    #[test]
    fn no_content_is_fabricated(ops in arb_ops(3)) {
        let written: std::collections::BTreeSet<u8> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Edit { content, .. } => Some(content + 1),
                _ => None,
            })
            .collect();

        let (devices, _) = run(&ops, 3).map_err(TestCaseError::fail)?;
        for (path, hash) in devices[0].visible() {
            prop_assert!(
                written.contains(&hash[0]),
                "{path} holds content nobody ever wrote"
            );
        }
    }

    /// A path that ends up visible must hold content, and one that ends up
    /// deleted must hold a tombstone -- never a half-state where the index
    /// disagrees with itself.
    #[test]
    fn every_path_is_either_present_or_tombstoned(ops in arb_ops(3)) {
        let (devices, _) = run(&ops, 3).map_err(TestCaseError::fail)?;
        for device in &devices {
            for (path, version) in &device.files {
                match &version.content {
                    Content::File { size, .. } => {
                        prop_assert!(*size > 0, "{path} is present with no content");
                        prop_assert!(device.visible().contains_key(path));
                    }
                    Content::Deleted => {
                        prop_assert!(!device.visible().contains_key(path));
                    }
                }
            }
        }
    }
}
