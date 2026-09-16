//! Convergence: two devices exchanging changes must end up agreeing.
//!
//! This is the property a sync engine lives or dies by, and it is not implied
//! by any of the unit tests. Each decision can be individually correct while
//! the system as a whole oscillates — two devices politely renaming each
//! other's files forever, or re-detecting the same conflict on every pass.
//!
//! The simulation here is deliberately crude: an in-memory map per device, and
//! a "sync" that reconciles in both directions and applies the results. It has
//! no network, no chunks, and no disk. What it does have is the real decision
//! logic, which is the part that can be wrong in ways nobody notices for
//! months.
//!
//! Phase 2 will replace this with property-based testing over random
//! interleavings. This is the seed of that work.

mod model;

use model::{assert_converged, sync_once, sync_until_stable, Device, A, B, C};
use qurb_sync::reconcile;

// -- basic propagation -------------------------------------------------------

#[test]
fn a_new_file_reaches_the_other_device() {
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("notes.txt", 1);

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(b.visible().get("notes.txt"), Some(&[1u8; 32]));
}

#[test]
fn an_edit_propagates_and_replaces() {
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("notes.txt", 1);
    sync_until_stable(&mut a, &mut b);

    a.edit("notes.txt", 2);
    sync_until_stable(&mut a, &mut b);

    assert_converged(&a, &b);
    assert_eq!(b.visible().get("notes.txt"), Some(&[2u8; 32]));
}

#[test]
fn a_deletion_propagates_and_stays_deleted() {
    // The failure this guards against: the peer still has the file, offers it
    // back, and the deletion silently undoes itself.
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("doomed.txt", 1);
    sync_until_stable(&mut a, &mut b);

    a.delete("doomed.txt");
    sync_until_stable(&mut a, &mut b);

    assert_converged(&a, &b);
    assert!(b.visible().is_empty(), "the file must not come back");

    // And it must still be gone after further syncing.
    sync_until_stable(&mut a, &mut b);
    assert!(a.visible().is_empty() && b.visible().is_empty());
}

#[test]
fn syncing_an_unchanged_pair_does_nothing() {
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("a.txt", 1);
    b.edit("b.txt", 2);
    sync_until_stable(&mut a, &mut b);

    assert!(reconcile(&a.versions(), &b.versions()).is_empty(), "no work should remain");
}

// -- conflicts ---------------------------------------------------------------

#[test]
fn concurrent_edits_converge_with_both_versions_kept() {
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("shared.txt", 1);
    sync_until_stable(&mut a, &mut b);

    // Both edit without seeing the other.
    a.edit("shared.txt", 0xFF);
    b.edit("shared.txt", 0x0A);

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);

    let visible = a.visible();
    assert_eq!(visible.len(), 2, "both edits survive: {:?}", visible.keys().collect::<Vec<_>>());
    assert!(visible.contains_key("shared.txt"));
    assert!(
        visible.keys().any(|k| k.starts_with("shared.conflict-")),
        "the losing edit must be kept under a conflict name"
    );

    let contents: Vec<_> = visible.values().map(|h| h[0]).collect();
    assert!(contents.contains(&0xFF) && contents.contains(&0x0A), "no edit was discarded");
}

#[test]
fn a_conflict_settles_and_does_not_recur() {
    // The bug this exists to catch: if a resolution does not record that it has
    // seen both sides, the next comparison finds the same conflict, and the two
    // devices conflict with each other forever.
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("shared.txt", 1);
    sync_until_stable(&mut a, &mut b);

    a.edit("shared.txt", 0xFF);
    b.edit("shared.txt", 0x0A);

    let rounds = sync_until_stable(&mut a, &mut b);
    assert!(rounds <= 3, "took {rounds} rounds to settle one conflict");

    assert!(
        reconcile(&a.versions(), &b.versions()).is_empty(),
        "the conflict was re-detected after being resolved"
    );

    let before = a.visible();
    sync_until_stable(&mut a, &mut b);
    assert_eq!(a.visible(), before, "further syncing must not create more conflict files");
}

#[test]
fn concurrent_identical_edits_do_not_produce_a_conflict() {
    // The same file copied onto both devices. Concurrent, but there is nothing
    // to disagree about, and a conflict file here would be pure noise.
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("same.txt", 42);
    b.edit("same.txt", 42);

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(a.visible().len(), 1, "no conflict file should appear");
}

#[test]
fn an_edit_survives_a_concurrent_delete() {
    // Decision 0005's promise. A deletion stays recoverable in the retention
    // window; a discarded edit does not.
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("contested.txt", 1);
    sync_until_stable(&mut a, &mut b);

    a.edit("contested.txt", 0x77);
    b.delete("contested.txt");

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(
        a.visible().get("contested.txt"),
        Some(&[0x77u8; 32]),
        "the edit must survive the delete"
    );
}

#[test]
fn concurrent_deletes_converge_without_a_conflict() {
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("doomed.txt", 1);
    sync_until_stable(&mut a, &mut b);

    a.delete("doomed.txt");
    b.delete("doomed.txt");

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert!(a.visible().is_empty());
}

// -- the general property ----------------------------------------------------

/// Deterministic pseudo-random sequences of edits, deletes and syncs.
///
/// Every seed must end converged. This is a weak stand-in for the property
/// testing Phase 2 will do properly, but it already explores interleavings
/// nobody would think to write by hand.
#[test]
fn random_operation_sequences_always_converge() {
    const PATHS: [&str; 4] = ["a.txt", "b.txt", "dir/c.txt", "notes.md"];

    for seed in 0..200u32 {
        let (mut a, mut b) = (Device::new(A), Device::new(B));
        let mut rng = seed | 1;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng
        };

        for _ in 0..24 {
            let r = next();
            let device = if r & 1 == 0 { &mut a } else { &mut b };
            let path = PATHS[(r >> 1) as usize % PATHS.len()];

            match (r >> 8) % 10 {
                0..=5 => device.edit(path, ((r >> 16) % 4) as u8 + 1),
                6..=7 => device.delete(path),
                // Sync at an arbitrary point, so devices diverge by varying
                // amounts before comparing notes.
                _ => sync_once(&mut a, &mut b),
            }
        }

        sync_until_stable(&mut a, &mut b);

        assert_eq!(a.visible(), b.visible(), "seed {seed}: user-visible state diverged");
        assert_eq!(a.files, b.files, "seed {seed}: histories diverged");
        assert!(
            reconcile(&a.versions(), &b.versions()).is_empty(),
            "seed {seed}: work remained after converging"
        );
    }
}

#[test]
fn syncing_is_idempotent() {
    // Running a sync twice must not change anything the first one settled.
    // Because delivery elsewhere in the system is at-least-once, repeated
    // syncs are normal rather than exceptional.
    let (mut a, mut b) = (Device::new(A), Device::new(B));
    a.edit("x.txt", 1);
    b.edit("y.txt", 2);
    a.edit("shared.txt", 0xFF);
    b.edit("shared.txt", 0x0A);

    sync_until_stable(&mut a, &mut b);
    let settled = (a.files.clone(), b.files.clone());

    for _ in 0..5 {
        sync_once(&mut a, &mut b);
    }
    assert_eq!((a.files.clone(), b.files.clone()), settled, "repeated syncing changed state");
}

// -- three devices -----------------------------------------------------------

/// Sync every pair until the whole set stops changing.
fn sync_all_until_stable(devices: &mut [Device]) -> usize {
    for round in 1..=20 {
        let before: Vec<_> = devices.iter().map(|d| d.files.clone()).collect();
        for i in 0..devices.len() {
            for j in (i + 1)..devices.len() {
                let (left, right) = devices.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                sync_once(a, b);
            }
        }
        let after: Vec<_> = devices.iter().map(|d| d.files.clone()).collect();
        if before == after {
            return round;
        }
    }
    panic!("three devices did not converge within 20 rounds");
}

#[test]
fn three_devices_converge() {
    // Two devices can be reconciled by much simpler schemes than vector
    // clocks. Three is where they start to earn their complexity: a change can
    // reach one device by two different routes, and "has this device seen that
    // edit?" stops having an obvious answer.
    let mut devices = vec![Device::new(A), Device::new(B), Device::new(C)];

    devices[0].edit("shared.txt", 1);
    sync_all_until_stable(&mut devices);

    devices[0].edit("shared.txt", 0xF0);
    devices[1].edit("shared.txt", 0x0B);
    devices[2].edit("other.txt", 0x22);

    sync_all_until_stable(&mut devices);

    let visible = devices[0].visible();
    for d in &devices {
        assert_eq!(d.visible(), visible, "device {} disagrees", d.id.short());
        assert_eq!(d.files, devices[0].files, "device {} has a different history", d.id.short());
    }

    let contents: Vec<_> = visible.values().map(|h| h[0]).collect();
    assert!(contents.contains(&0xF0) && contents.contains(&0x0B), "an edit was lost");
    assert!(contents.contains(&0x22));
}

#[test]
fn a_change_propagates_through_an_intermediary() {
    // A and C never talk to each other directly. B has to carry the change,
    // which is the situation that makes a laptop-plus-phone-plus-desktop setup
    // work when they are rarely all awake at once.
    let (mut a, mut b, mut c) = (Device::new(A), Device::new(B), Device::new(C));

    a.edit("relay.txt", 9);
    sync_until_stable(&mut a, &mut b);
    sync_until_stable(&mut b, &mut c);

    assert_eq!(c.visible().get("relay.txt"), Some(&[9u8; 32]));
    assert_eq!(a.files, c.files, "the history must survive the relay, not just the content");
}

#[test]
fn three_devices_converge_under_random_operations() {
    const PATHS: [&str; 3] = ["a.txt", "b.txt", "sub/c.txt"];

    for seed in 0..120u32 {
        let mut devices = vec![Device::new(A), Device::new(B), Device::new(C)];
        let mut rng = seed | 1;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng
        };

        for _ in 0..30 {
            let r = next();
            let which = (r >> 1) as usize % 3;
            let path = PATHS[(r >> 3) as usize % PATHS.len()];

            match (r >> 8) % 10 {
                0..=5 => devices[which].edit(path, ((r >> 16) % 3) as u8 + 1),
                6..=7 => devices[which].delete(path),
                _ => {
                    // Sync one arbitrary pair, so devices learn about changes
                    // by different routes and at different times.
                    let i = which;
                    let j = (which + 1 + (r >> 20) as usize % 2) % 3;
                    if i != j {
                        let (lo, hi) = (i.min(j), i.max(j));
                        let (left, right) = devices.split_at_mut(hi);
                        sync_once(&mut left[lo], &mut right[0]);
                    }
                }
            }
        }

        sync_all_until_stable(&mut devices);

        for d in &devices[1..] {
            assert_eq!(d.visible(), devices[0].visible(), "seed {seed}: visible state diverged");
            assert_eq!(d.files, devices[0].files, "seed {seed}: history diverged");
        }
    }
}
