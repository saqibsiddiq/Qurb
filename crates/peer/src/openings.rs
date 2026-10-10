//! A guest's folder opened at the computer that keeps it, with the guest's
//! approval on their phone (decision 0060, step 5).
//!
//! The computer keeps a guest's folder sealed and cannot open it. To open it
//! here, the window asks; the guest's phone collects the ask at its next sync
//! with this computer, the person approves behind their fingerprint, face or
//! screen lock, and the phone sends the folder's key. This computer then
//! holds the key **in memory only**, for as long as the folder is open, and
//! forgets it when it is locked, or after [`IDLE`] without use.
//!
//! One registry per process, because the window that asks and the server that
//! is answered live in the same one: the desktop application hosts its daemon
//! (decision 0032). Keyed by the guest's person, which is unique to it.

use qurb_sync::DeviceId;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long an open folder stays open without being used.
pub const IDLE: Duration = Duration::from_secs(10 * 60);

/// How long an ask waits for the phone before it lapses.
pub const ASK_LIFETIME: Duration = Duration::from_secs(5 * 60);

struct Registry {
    asks: HashMap<DeviceId, ([u8; 16], Instant)>,
    open: HashMap<DeviceId, ([u8; 32], Instant)>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry { asks: HashMap::new(), open: HashMap::new() }))
}

/// Ask to open `person`'s folder here. Returns the ask's nonce, which the
/// phone's answer has to carry.
pub fn ask(person: &DeviceId) -> [u8; 16] {
    use rand::RngCore;
    let mut nonce = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    if let Ok(mut r) = registry().lock() {
        r.asks.insert(*person, (nonce, Instant::now()));
    }
    nonce
}

/// The ask waiting for `person`'s phone, if one is and it has not lapsed.
pub fn asking(person: &DeviceId) -> Option<[u8; 16]> {
    let mut r = registry().lock().ok()?;
    match r.asks.get(person) {
        Some((nonce, at)) if at.elapsed() < ASK_LIFETIME => Some(*nonce),
        Some(_) => {
            r.asks.remove(person);
            None
        }
        None => None,
    }
}

/// The phone approved: open the folder with `key`, if `nonce` is the ask's
/// and `check` -- which tries the key on something sealed with it -- agrees.
/// Returns whether it opened.
pub fn unlock(person: &DeviceId, nonce: &[u8; 16], key: [u8; 32], check: impl Fn(&[u8; 32]) -> bool) -> bool {
    let Ok(mut r) = registry().lock() else { return false };
    let asked = matches!(r.asks.get(person), Some((n, at)) if n == nonce && at.elapsed() < ASK_LIFETIME);
    if !asked || !check(&key) {
        return false;
    }
    r.asks.remove(person);
    r.open.insert(*person, (key, Instant::now()));
    true
}

/// The key `person`'s folder is open with, if it is open: and its use counts,
/// so the idle time starts again.
pub fn key(person: &DeviceId) -> Option<[u8; 32]> {
    let mut r = registry().lock().ok()?;
    match r.open.get_mut(person) {
        Some((key, used)) if used.elapsed() < IDLE => {
            *used = Instant::now();
            Some(*key)
        }
        Some(_) => {
            r.open.remove(person);
            None
        }
        None => None,
    }
}

/// Whether `person`'s folder is open here, without counting as a use.
pub fn is_open(person: &DeviceId) -> bool {
    registry()
        .lock()
        .ok()
        .is_some_and(|r| r.open.get(person).is_some_and(|(_, used)| used.elapsed() < IDLE))
}

/// Close `person`'s folder here: the key is forgotten, and any ask withdrawn.
pub fn lock(person: &DeviceId) {
    if let Ok(mut r) = registry().lock() {
        r.open.remove(person);
        r.asks.remove(person);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_opens_only_with_the_ask_and_a_key_that_fits() {
        let person = DeviceId::from_bytes([77; 32]);
        assert!(!unlock(&person, &[0; 16], [1; 32], |_| true), "opened without being asked");
        let nonce = ask(&person);
        assert_eq!(asking(&person), Some(nonce));
        assert!(!unlock(&person, &[9; 16], [1; 32], |_| true), "opened with another ask's answer");
        assert!(!unlock(&person, &nonce, [2; 32], |_| false), "opened with a key that does not fit");
        assert!(unlock(&person, &nonce, [1; 32], |k| *k == [1; 32]));
        assert_eq!(key(&person), Some([1; 32]));
        assert_eq!(asking(&person), None, "the ask stayed after it was answered");
        lock(&person);
        assert_eq!(key(&person), None);
        assert!(!is_open(&person));
    }
}
