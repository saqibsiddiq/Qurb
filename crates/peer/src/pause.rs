//! Whether this device has stopped syncing for now: paused by its person
//! until a time or until they resume, or held off a network they said not to
//! use (decision 0086). Process-wide, as [`crate::limit`] is: one process is
//! one device, and the desktop window hosts its daemon.
//!
//! Paused, a device behaves as one that is off: it starts no passes, and a
//! connection from another device is closed at its next request, so the
//! other side sees it unreachable -- quietly, as it would a computer asleep --
//! rather than failing file by file.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

/// Paused until this Unix time; 0 for not paused, [`UNTIL_RESUMED`] for no end.
static UNTIL: AtomicI64 = AtomicI64::new(0);
/// Held off the network it is on now.
static HELD: AtomicBool = AtomicBool::new(false);

/// Paused with no end: until the person resumes.
pub const UNTIL_RESUMED: i64 = i64::MAX;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Pause until `until`, a Unix time, or [`UNTIL_RESUMED`]; 0 resumes.
pub fn set(until: i64) {
    UNTIL.store(until, Ordering::SeqCst);
}

/// When the pause ends: `None` when not paused (or the time has passed),
/// [`UNTIL_RESUMED`] for no end.
pub fn until() -> Option<i64> {
    let until = UNTIL.load(Ordering::SeqCst);
    (until > now()).then_some(until)
}

/// Hold off, or stop holding off, the network this device is on.
pub fn hold(held: bool) {
    HELD.store(held, Ordering::SeqCst);
}

/// Whether nothing is to be synced now, for either reason.
pub fn paused() -> bool {
    HELD.load(Ordering::SeqCst) || until().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test: the state is process-wide, and tests run side by side.
    #[test]
    fn a_pause_ends_at_its_time_and_a_hold_when_let_go() {
        assert!(!paused());
        set(now() + 3600);
        assert!(paused());
        set(now() - 1);
        assert!(!paused(), "a pause outlived its time");
        assert_eq!(until(), None);
        set(UNTIL_RESUMED);
        assert_eq!(until(), Some(UNTIL_RESUMED));
        set(0);
        hold(true);
        assert!(paused());
        hold(false);
        assert!(!paused());
    }
}
