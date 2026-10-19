//! What differs between the systems qurb is built for, said once.
//!
//! qurb was written on Linux and names its places as Linux does: `HOME`,
//! `XDG_CONFIG_HOME`, `USER`. Windows has the same places under other names,
//! and usually sets none of these. Rather than teach every caller both,
//! [`settle`] gives a Windows process the Linux names for the Windows places
//! before anything asks; elsewhere it does nothing.
//!
//! Windows builds are made but have not run on Windows (docs/releasing.md).

use std::io;
use std::ffi::OsStr;
use std::process::{Child, Command, Stdio};

/// Called first in each program's `main`, before any thread exists.
pub fn settle() {
    #[cfg(windows)]
    {
        let alias = |name: &str, from: &str| {
            if std::env::var_os(name).is_none() {
                if let Some(value) = std::env::var_os(from) {
                    std::env::set_var(name, value);
                }
            }
        };
        alias("HOME", "USERPROFILE");
        alias("XDG_CONFIG_HOME", "APPDATA");
        alias("USER", "USERNAME");
    }
}

/// Open `target` -- a folder, or an address -- with whatever the desktop
/// opens it with: `xdg-open` on Linux, the shell's `start` on Windows.
/// Reaped on a thread of its own, so it neither blocks nor lingers.
pub fn open(target: impl AsRef<OsStr>) -> io::Result<()> {
    let target = target.as_ref();
    #[cfg(windows)]
    let mut command = {
        let mut c = Command::new("cmd");
        // The empty title: `start` takes a first quoted argument as one.
        c.args(["/C", "start", ""]).arg(target);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(target);
        c
    };
    let child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    reap(child);
    Ok(())
}

/// Wait for `child` on a thread of its own, so it does not linger as a zombie
/// for as long as qurb runs.
pub fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}
