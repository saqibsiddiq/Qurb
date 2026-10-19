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

        // Without waiting, which is the whole point: the answer wanted here is
        // "is someone else running", not "let me queue up". The standard
        // library's, since 2026-10-18: `flock` on Unix as before, and the
        // same on Windows, which `libc::flock` was not.
        match file.try_lock() {
            Ok(()) => Ok(Some(Lock { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(e)) => Err(e).with_context(|| format!("locking {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second taker is told someone has it, and has it once they let go.
    #[test]
    fn one_daemon_holds_a_store_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let first = Lock::take(dir.path()).unwrap();
        assert!(first.is_some(), "the lock was not taken");
        assert!(Lock::take(dir.path()).unwrap().is_none(), "a second daemon was let in");
        drop(first);
        assert!(Lock::take(dir.path()).unwrap().is_some(), "the lock was not given back");
    }
}
