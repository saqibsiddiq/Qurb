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

/// What this computer keeps for itself across a reinstall of qurb and a new
/// folder: its machine id. Made into an install marker
/// (`qurb_storage::db::install_marker`) before any device is told it. None
/// where it cannot be read.
pub fn install_id() -> Option<String> {
    let id = if cfg!(windows) {
        // HKLM\SOFTWARE\Microsoft\Cryptography, MachineGuid: set at install.
        let out = Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        String::from_utf8(out.stdout).ok()?.lines().find_map(|l| l.split("REG_SZ").nth(1).map(|v| v.trim().to_string()))?
    } else if cfg!(target_os = "macos") {
        let out = Command::new("ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).stderr(Stdio::null()).output().ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        let line = text.lines().find(|l| l.contains("IOPlatformUUID"))?;
        line.rsplit('"').nth(1)?.to_string()
    } else {
        ["/etc/machine-id", "/var/lib/dbus/machine-id"].iter().find_map(|p| std::fs::read_to_string(p).ok())?
    };
    let id = id.trim().to_string();
    (!id.is_empty()).then_some(id)
}
