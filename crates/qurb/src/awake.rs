//! Keeping the computer awake while it collects, when it is plugged in.
//!
//! The owner's item 1 of 2026-10-14. On 2026-10-13 the laptop suspended
//! itself when idle at 21:51, with 5,311 files of the S23's WhatsApp backup
//! still to come, and woke at 10:06: the idle suspend cost 12 of the backup's
//! 15.5 hours (decision 0062). A computer collecting is not idle.
//!
//! Held only while a plan brings content in, and only on mains power or with
//! no battery at all: a laptop on battery sleeps as its person set it to.
//!
//! Two inhibitors, measured on the owner's GNOME laptop: GNOME's idle suspend
//! consults its own session's (`gnome-session-inhibit`), which logind's alone
//! did not register with (`IsInhibited` stayed false); every other desktop
//! that suspends through logind consults logind's (`systemd-inhibit`). Both
//! are held, the first where it exists. What they hold open is `tail --pid`
//! on this process, so that if Qurb dies the inhibitors go with it and the
//! computer is never left awake by mistake.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Held while it lives; dropped, the computer may sleep again.
pub struct Awake(Option<Child>);

impl Awake {
    /// Keep the computer from idling to sleep, if it is plugged in, for as
    /// long as this is held. `why` is what the system shows as the reason.
    pub fn while_collecting(why: &str) -> Self {
        if !plugged_in() {
            return Self(None);
        }
        Self(inhibit(why))
    }

    /// Whether anything is being held: plugged in, and the tools there.
    pub fn holding(&self) -> bool {
        self.0.is_some()
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            // The whole group: the wrapper, the inhibitor inside it and what
            // it holds open, which killing the wrapper alone would orphan.
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGTERM);
            }
            let _ = child.wait();
        }
    }
}

/// On mains power, or a computer with no battery at all. Unknown counts as
/// plugged in: a desktop without power-supply information has no battery to
/// spare.
fn plugged_in() -> bool {
    plugged_in_from(std::path::Path::new("/sys/class/power_supply"))
}

fn plugged_in_from(supplies: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(supplies) else { return true };
    let read = |path: PathBuf| std::fs::read_to_string(path).map(|s| s.trim().to_string()).unwrap_or_default();
    let (mut battery, mut mains) = (false, false);
    for entry in entries.flatten() {
        let kind = read(entry.path().join("type"));
        if kind == "Battery" {
            battery = true;
        }
        // USB-C can power a laptop as mains does.
        if (kind == "Mains" || kind.starts_with("USB")) && read(entry.path().join("online")) == "1" {
            mains = true;
        }
    }
    mains || !battery
}

fn inhibit(why: &str) -> Option<Child> {
    let systemd = on_path("systemd-inhibit")?;
    let tail = on_path("tail")?;
    let mut command = match on_path("gnome-session-inhibit") {
        Some(gnome) => {
            let mut c = Command::new(gnome);
            c.args(["--app-id", "qurb", "--reason", why, "--inhibit", "suspend:idle"]).arg(systemd);
            c
        }
        None => Command::new(systemd),
    };
    command
        .args(["--what=sleep:idle", "--who=Qurb", &format!("--why={why}"), "--mode=block"])
        .arg(tail)
        .args(["--pid", &std::process::id().to_string(), "-f", "/dev/null"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    match command.spawn() {
        Ok(child) => {
            tracing::info!(why, "keeping the computer awake while collecting");
            Some(child)
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not keep the computer awake");
            None
        }
    }
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| PathBuf::from(dir).join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(dir: &std::path::Path, name: &str, kind: &str, online: Option<&str>) {
        let at = dir.join(name);
        std::fs::create_dir_all(&at).unwrap();
        std::fs::write(at.join("type"), format!("{kind}\n")).unwrap();
        if let Some(online) = online {
            std::fs::write(at.join("online"), format!("{online}\n")).unwrap();
        }
    }

    /// The owner's laptop as it reads: a battery, mains online.
    #[test]
    fn plugged_in_is_mains_or_usb_online_or_no_battery() {
        let laptop = tempfile::tempdir().unwrap();
        supply(laptop.path(), "BAT1", "Battery", None);
        supply(laptop.path(), "ACAD", "Mains", Some("1"));
        assert!(plugged_in_from(laptop.path()));

        let on_battery = tempfile::tempdir().unwrap();
        supply(on_battery.path(), "BAT1", "Battery", None);
        supply(on_battery.path(), "ACAD", "Mains", Some("0"));
        supply(on_battery.path(), "ucsi-source-psy-USBC000:002", "USB", Some("0"));
        assert!(!plugged_in_from(on_battery.path()));

        let usb_c = tempfile::tempdir().unwrap();
        supply(usb_c.path(), "BAT0", "Battery", None);
        supply(usb_c.path(), "ucsi-source-psy-USBC000:001", "USB", Some("1"));
        assert!(plugged_in_from(usb_c.path()));

        let desktop = tempfile::tempdir().unwrap();
        assert!(plugged_in_from(desktop.path()), "no battery: a desktop");
    }

    /// Not run by default: needs a desktop session. Held, GNOME and logind
    /// both say so; dropped, neither does, and nothing is left running.
    /// `cargo test -p qurb-cli --lib awake -- --ignored`
    #[test]
    #[ignore]
    fn held_and_let_go_on_this_desktop() {
        let gnome = || {
            let out = Command::new("gdbus")
                .args(["call", "--session", "--dest", "org.gnome.SessionManager", "--object-path",
                    "/org/gnome/SessionManager", "--method", "org.gnome.SessionManager.IsInhibited", "4"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).contains("true")
        };
        let logind = || {
            let out = Command::new("systemd-inhibit").arg("--list").output().unwrap();
            String::from_utf8_lossy(&out.stdout).contains("awake test")
        };
        assert!(!logind(), "an inhibitor from before");
        let awake = Awake::while_collecting("awake test");
        assert!(awake.holding(), "not plugged in, or no tools");
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert!(gnome() && logind(), "held, but not seen");
        drop(awake);
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert!(!logind(), "still held after letting go");
    }
}
