//! One daemon per folder.
//!
//! Two daemons on one store is not a theoretical problem. They contend on the
//! SQLite write lock, both answer as the same device on the network, both
//! reconcile the same directory, and both enforce the same storage cap — and
//! the symptoms are not "an error", they are slowness and confusion.
//!
//! It is easy to end up there by accident: the desktop app launched from the
//! applications menu and `qurb run` typed into a terminal are the same daemon
//! with different faces, and neither previously knew about the other. Found
//! exactly that way, with two of them running on this machine.
//!
//! An advisory `flock`, because the kernel releases it when the process dies.
//! A lock file holding a PID would need to answer "is that process still
//! alive, and is it still qurb", which is a question with no good answer after
//! a crash and a PID reuse.

use anyhow::{Context, Result};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;

/// Held for as long as the daemon runs. Dropping it releases the lock, and so
/// does the process exiting for any reason, including being killed.
pub struct Lock {
    _file: File,
}

impl Lock {
    /// Take the lock for a store, or say who has it.
    ///
    /// `Ok(None)` means another process holds it — a normal situation with a
    /// clear explanation, not an error to propagate.
    pub fn take(store_dir: &Path) -> Result<Option<Lock>> {
        std::fs::create_dir_all(store_dir)
            .with_context(|| format!("creating {}", store_dir.display()))?;

        let path = store_dir.join("daemon.lock");
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;

        // SAFETY: a valid descriptor and a constant operation. `LOCK_NB` makes
        // this return rather than wait, which is the whole point: the answer
        // wanted here is "is someone else running", not "let me queue up".
        let taken = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if taken == 0 {
            return Ok(Some(Lock { _file: file }));
        }

        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EWOULDBLOCK) => Ok(None),
            _ => Err(error).with_context(|| format!("locking {}", path.display())),
        }
    }
}
