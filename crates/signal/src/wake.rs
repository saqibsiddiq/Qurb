//! Waking a device that is not connected.
//!
//! The rendezvous service can tell a device something only while it is holding
//! a socket open, and a phone does not hold one: Android stops a background
//! app's connection within minutes of the screen going off, and iOS never
//! allowed one. So the device most in need of being told something is the one
//! that cannot be told.
//!
//! A push notification is the way through, and it is the only way through:
//! both platforms reserve waking a sleeping app to their own service. What
//! qurb sends is an empty poke — no filenames, no sizes, no counts, not even
//! which peer — because the device already knows how to find out once it is
//! awake. See
//! [decision 0028](../../docs/decisions/0028-waking-a-sleeping-device.md).
//!
//! This module is the seam. The service holds a [`Waker`], the tests hold one
//! that records, and the deployment holds one that talks to Google.

use std::sync::Arc;

/// Somewhere a sleeping device can be reached.
///
/// Opaque to qurb: it is whatever the platform's push service issued, and the
/// only thing done with it is handing it back to that service.
pub type WakeToken = String;

/// How a device that is not connected gets told to wake up.
pub trait Waker: Send + Sync {
    /// Poke the device behind this token.
    ///
    /// Carries nothing. A woken device syncs with the peers it already knows
    /// about, so there is nothing useful to put in the message and every
    /// reason not to: the push service sees the message, and qurb's promise is
    /// that nobody outside the devices learns what is being synced.
    ///
    /// Best effort and non-blocking. A failure means a device syncs on its own
    /// schedule instead, which is what happened before any of this existed.
    fn wake(&self, token: &WakeToken);
}

/// The default: nobody can be woken.
///
/// A service configured with no push credentials behaves exactly as it did
/// before push existed — devices sync when they next look. Correct rather than
/// degraded, and it is what every test and every self-hosted deployment
/// without a Firebase project gets.
pub struct NoWaker;

impl Waker for NoWaker {
    fn wake(&self, _token: &WakeToken) {}
}

/// A waker shared by the whole service.
pub type SharedWaker = Arc<dyn Waker>;

/// The default waker: none.
pub fn none() -> SharedWaker {
    Arc::new(NoWaker)
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::Mutex;

    /// Records who it was asked to wake, for tests to assert on.
    #[derive(Default)]
    pub struct Recorder {
        woken: Mutex<Vec<WakeToken>>,
    }

    impl Recorder {
        pub fn woken(&self) -> Vec<WakeToken> {
            self.woken.lock().expect("recorder").clone()
        }
    }

    impl Waker for Recorder {
        fn wake(&self, token: &WakeToken) {
            self.woken.lock().expect("recorder").push(token.clone());
        }
    }
}
