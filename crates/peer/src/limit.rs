//! How fast this device lets files cross the network: one rate for what it
//! sends, one for what it fetches, each in bytes a second, none by default.
//!
//! The owner's list of 2026-10-28: a computer catching up with a phone's
//! camera roll should be able to leave room on the connection for everything
//! else. Counted in chunks' bytes, the only large thing on the wire: a chunk
//! served is "up", a chunk fetched or a part of an upload received is
//! "down". Process-wide, since one process is one device; the desktop window
//! hosts its daemon, so a change made there applies at once.
//!
//! A token bucket that may go into debt: a chunk larger than a second's
//! allowance still goes, and the next waits until the debt is paid. A rate
//! set lower takes effect from the next chunk.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Which way bytes are going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// Sent to another device.
    Up,
    /// Received from another device.
    Down,
}

struct Bucket {
    /// Bytes a second, or zero for no limit.
    rate: u64,
    /// Bytes that may go now; negative while in debt.
    allowance: f64,
    at: Instant,
}

static UP: Mutex<Option<Bucket>> = Mutex::new(None);
static DOWN: Mutex<Option<Bucket>> = Mutex::new(None);

fn bucket(way: Way) -> &'static Mutex<Option<Bucket>> {
    match way {
        Way::Up => &UP,
        Way::Down => &DOWN,
    }
}

/// Set both rates, in bytes a second; zero for no limit.
pub fn set(up: u64, down: u64) {
    for (way, rate) in [(Way::Up, up), (Way::Down, down)] {
        let mut b = bucket(way).lock().expect("limit poisoned");
        *b = (rate > 0).then(|| Bucket { rate, allowance: rate as f64, at: Instant::now() });
    }
}

/// The rates in force: (up, down), zero for none.
pub fn rates() -> (u64, u64) {
    let rate = |way| bucket(way).lock().expect("limit poisoned").as_ref().map_or(0, |b: &Bucket| b.rate);
    (rate(Way::Up), rate(Way::Down))
}

/// How long `bytes` going `way` must wait, charging them to the bucket.
fn charge(way: Way, bytes: usize, now: Instant) -> Duration {
    let mut guard = bucket(way).lock().expect("limit poisoned");
    let Some(b) = guard.as_mut() else { return Duration::ZERO };
    let rate = b.rate as f64;
    // Refilled for the time since, never past one second's worth: a quiet
    // hour does not earn a burst of an hour's bytes.
    b.allowance = (b.allowance + now.saturating_duration_since(b.at).as_secs_f64() * rate).min(rate);
    b.at = now;
    b.allowance -= bytes as f64;
    if b.allowance >= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(-b.allowance / rate)
    }
}

/// Wait until `bytes` may go `way`.
pub async fn spend(way: Way, bytes: usize) {
    let wait = charge(way, bytes, Instant::now());
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both ways in one test: the buckets are the process's own, and tests
    /// run side by side.
    #[test]
    fn a_rate_holds_bytes_back_and_none_does_not() {
        set(0, 0);
        let start = Instant::now();
        assert_eq!(charge(Way::Up, 10 << 20, start), Duration::ZERO);

        set(1_000_000, 0);
        assert_eq!(rates(), (1_000_000, 0));
        // A second's worth goes at once; the next half second's waits half a
        // second; after a second's quiet, a second's worth goes again.
        let start = Instant::now();
        assert_eq!(charge(Way::Up, 1_000_000, start), Duration::ZERO);
        let wait = charge(Way::Up, 500_000, start);
        assert!((wait.as_secs_f64() - 0.5).abs() < 0.01, "{wait:?}");
        let later = start + Duration::from_millis(1500);
        assert_eq!(charge(Way::Up, 1_000_000, later), Duration::ZERO);
        // Downloads are not held back by an upload limit.
        assert_eq!(charge(Way::Down, 50 << 20, later), Duration::ZERO);

        set(0, 0);
        assert_eq!(rates(), (0, 0));
    }
}
