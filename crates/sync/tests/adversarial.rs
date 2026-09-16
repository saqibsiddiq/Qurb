//! Deliberate abuse.
//!
//! Each of these is a situation that occurs in the field and that a sync engine
//! is expected to survive: a device whose clock is badly wrong, a device that
//! was switched off for a month, a device that has been edited heavily while
//! disconnected. The interesting thing about all of them is that nothing
//! *fails* — they either converge or they quietly corrupt, and only a test
//! looking for it can tell which.

mod model;

use model::{assert_converged, sync_until_stable, Device, A, B, C};
use qurb_sync::reconcile;

// -- clocks ------------------------------------------------------------------

#[test]
fn a_device_with_a_badly_wrong_clock_still_converges() {
    // Ordering is the version vector's job; wall-clock time is used only for
    // conflict filenames. A device three days in the past must therefore not
    // lose every conflict, nor win every one.
    let mut a = Device::with_skew(A, -3 * 86_400);
    let mut b = Device::new(B);

    a.edit("shared.txt", 1);
    sync_until_stable(&mut a, &mut b);

    a.edit("shared.txt", 0xF0);
    b.edit("shared.txt", 0x0B);
    sync_until_stable(&mut a, &mut b);

    assert_converged(&a, &b);
    let contents: Vec<u8> = a.visible().values().map(|h| h[0]).collect();
    assert!(contents.contains(&0xF0), "the past-dated device's edit was lost");
    assert!(contents.contains(&0x0B), "the correctly-dated device's edit was lost");
}

#[test]
fn a_device_far_in_the_future_does_not_win_everything() {
    // The failure this guards against: ordering by timestamp, which would let
    // one wrong clock systematically beat every other device forever.
    let mut a = Device::with_skew(A, 10 * 365 * 86_400);
    let mut b = Device::new(B);

    b.edit("notes.txt", 1);
    sync_until_stable(&mut a, &mut b);

    // A is a decade ahead but has seen B's change, so its edit legitimately
    // supersedes -- by causality, not by clock.
    a.edit("notes.txt", 2);
    sync_until_stable(&mut a, &mut b);
    assert_eq!(b.visible().get("notes.txt").map(|h| h[0]), Some(2));

    // Now B edits having seen A's, and must win despite being a decade behind.
    b.edit("notes.txt", 3);
    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(
        a.visible().get("notes.txt").map(|h| h[0]),
        Some(3),
        "a device with a wrong clock overrode a causally later edit"
    );
    assert_eq!(a.visible().len(), 1, "a spurious conflict file appeared");
}

#[test]
fn clock_skew_does_not_change_who_wins_a_conflict() {
    // The same pair of concurrent edits must resolve the same way regardless of
    // what the devices think the time is, or two devices with different skew
    // would rename each other's copies and never agree.
    fn outcome(skew: i64) -> Vec<u8> {
        let mut a = Device::with_skew(A, skew);
        let mut b = Device::new(B);
        a.edit("f.txt", 1);
        sync_until_stable(&mut a, &mut b);
        a.edit("f.txt", 0xAA);
        b.edit("f.txt", 0xBB);
        sync_until_stable(&mut a, &mut b);

        let mut winner: Vec<u8> = a
            .visible()
            .iter()
            .filter(|(path, _)| path.as_str() == "f.txt")
            .map(|(_, h)| h[0])
            .collect();
        winner.sort_unstable();
        winner
    }

    let baseline = outcome(0);
    for skew in [-86_400 * 30, -3600, 3600, 86_400 * 365] {
        assert_eq!(outcome(skew), baseline, "skew of {skew}s changed the winner");
    }
}

// -- long absences -----------------------------------------------------------

#[test]
fn a_device_offline_for_a_month_catches_up() {
    let mut online_a = Device::new(A);
    let mut online_b = Device::new(B);
    let mut away = Device::with_skew(C, 0);

    // Everyone starts in agreement.
    online_a.edit("shared.txt", 1);
    sync_until_stable(&mut online_a, &mut online_b);
    sync_until_stable(&mut online_a, &mut away);

    // A month of activity between the two that stayed on.
    for day in 1..=30u8 {
        online_a.edit("shared.txt", day);
        online_a.edit(&format!("day{day}.txt"), day);
        if day % 3 == 0 {
            online_b.edit("shared.txt", day.wrapping_add(100));
        }
        if day % 7 == 0 {
            online_a.delete(&format!("day{}.txt", day - 1));
        }
        sync_until_stable(&mut online_a, &mut online_b);
    }

    // The absent device returns.
    sync_until_stable(&mut away, &mut online_a);
    sync_until_stable(&mut away, &mut online_b);
    sync_until_stable(&mut online_a, &mut online_b);

    assert_converged(&away, &online_a);
    assert_converged(&away, &online_b);
    assert!(
        reconcile(&away.versions(), &online_a.versions()).is_empty(),
        "work remained after catching up"
    );
}

#[test]
fn a_device_that_worked_offline_merges_rather_than_being_overwritten() {
    // The case that actually loses data if handled badly: a device edits while
    // disconnected, and its work must survive rejoining.
    let mut home = Device::new(A);
    let mut laptop = Device::new(B);

    home.edit("report.txt", 1);
    home.edit("keep.txt", 1);
    sync_until_stable(&mut home, &mut laptop);

    // A month apart, both working.
    for i in 1..=30u8 {
        home.edit("report.txt", i);
        laptop.edit("laptop-only.txt", i);
    }
    laptop.edit("report.txt", 0xEE);

    sync_until_stable(&mut home, &mut laptop);
    assert_converged(&home, &laptop);

    let visible = home.visible();
    assert!(visible.contains_key("laptop-only.txt"), "offline work was lost");
    assert!(visible.contains_key("keep.txt"));

    let report: Vec<u8> = visible
        .iter()
        .filter(|(p, _)| p.starts_with("report"))
        .map(|(_, h)| h[0])
        .collect();
    assert!(report.contains(&0xEE), "the laptop's edit was discarded");
    assert!(report.contains(&30), "the home device's edit was discarded");
}

#[test]
fn a_deletion_made_while_offline_is_not_undone_by_rejoining() {
    // A peer that never learned about a deletion still has the file and will
    // offer it back. Without a tombstone crossing, the deletion silently
    // reverses itself -- one of the most confusing bugs a sync engine has.
    let mut home = Device::new(A);
    let mut laptop = Device::new(B);

    home.edit("doomed.txt", 1);
    sync_until_stable(&mut home, &mut laptop);

    home.delete("doomed.txt");
    for _ in 0..20 {
        home.edit("other.txt", 2);
    }

    sync_until_stable(&mut home, &mut laptop);
    assert!(!laptop.visible().contains_key("doomed.txt"), "the deletion was undone");
    assert_converged(&home, &laptop);
}

#[test]
fn three_devices_with_three_different_wrong_clocks_converge() {
    let mut a = Device::with_skew(A, -86_400 * 7);
    let mut b = Device::with_skew(B, 86_400 * 400);
    let mut c = Device::with_skew(C, 0);

    a.edit("shared.txt", 1);
    sync_until_stable(&mut a, &mut b);
    sync_until_stable(&mut b, &mut c);
    sync_until_stable(&mut a, &mut c);

    a.edit("shared.txt", 0x11);
    b.edit("shared.txt", 0x22);
    c.edit("shared.txt", 0x33);

    for _ in 0..3 {
        sync_until_stable(&mut a, &mut b);
        sync_until_stable(&mut b, &mut c);
        sync_until_stable(&mut a, &mut c);
    }

    assert_converged(&a, &b);
    assert_converged(&b, &c);
    let contents: Vec<u8> = a.visible().values().map(|h| h[0]).collect();
    for expected in [0x11, 0x22, 0x33] {
        assert!(contents.contains(&expected), "edit {expected:#x} was lost among skewed clocks");
    }
}
