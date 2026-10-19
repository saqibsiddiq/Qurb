//! Starting qurb when somebody logs in.
//!
//! A sync product that syncs only after somebody remembers to open it syncs
//! rarely, and the phone can only meet the laptop while the laptop is running.
//! So qurb can start at login, with its window hidden, through the freedesktop
//! autostart directory -- which GNOME, KDE and the rest all read, and which a
//! person can see and remove without qurb's help.
//!
//! Every function takes the config directory, so tests never touch the real
//! one; [`config_dir`] is the real one.
//!
//! On Windows, the user's `Run` key in the registry instead, through
//! `reg.exe`, which is where Task Manager's *Startup apps* reads and turns it
//! off; the config directory is not used there. Built, and not yet run on
//! Windows (docs/releasing.md).

use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME`, or `~/.config`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[cfg(not(windows))]
pub fn entry(config: &Path) -> PathBuf {
    config.join("autostart").join("qurb.desktop")
}

#[cfg(windows)]
pub fn is_enabled(_config: &Path) -> bool {
    run_key::is_set()
}

#[cfg(windows)]
pub fn enable(_config: &Path, program: &Path) -> std::io::Result<()> {
    run_key::set(program)
}

#[cfg(windows)]
pub fn disable(_config: &Path) -> std::io::Result<()> {
    run_key::remove()
}

#[cfg(windows)]
mod run_key {
    use std::path::Path;
    use std::process::{Command, Stdio};

    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "Qurb";

    fn reg(args: &[&str]) -> std::io::Result<bool> {
        use std::os::windows::process::CommandExt;
        // No console window flashing up for it.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Ok(Command::new("reg")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()?
            .success())
    }

    pub fn is_set() -> bool {
        reg(&["query", KEY, "/v", NAME]).unwrap_or(false)
    }

    pub fn set(program: &Path) -> std::io::Result<()> {
        let value = format!("\"{}\" --hidden", program.display());
        match reg(&["add", KEY, "/v", NAME, "/t", "REG_SZ", "/d", &value, "/f"])? {
            true => Ok(()),
            false => Err(std::io::Error::other("the registry refused the startup entry")),
        }
    }

    pub fn remove() -> std::io::Result<()> {
        if is_set() && !reg(&["delete", KEY, "/v", NAME, "/f"])? {
            return Err(std::io::Error::other("the registry kept the startup entry"));
        }
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn is_enabled(config: &Path) -> bool {
    std::fs::read_to_string(entry(config))
        .map(|text| !text.lines().any(|l| l.trim() == "Hidden=true"))
        .unwrap_or(false)
}

/// Start `program` at login, with its window hidden.
#[cfg(not(windows))]
pub fn enable(config: &Path, program: &Path) -> std::io::Result<()> {
    let path = entry(config);
    std::fs::create_dir_all(path.parent().expect("autostart has a parent"))?;
    std::fs::write(
        path,
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Qurb\n\
             Comment=Keeps your files in sync in the background\n\
             Exec={} --hidden\n\
             Icon=qurb\n\
             Terminal=false\n\
             NoDisplay=true\n\
             X-GNOME-Autostart-enabled=true\n",
            program.display()
        ),
    )
}

#[cfg(not(windows))]
pub fn disable(config: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(entry(config)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn turned_on_and_off() {
        let config = tempfile::tempdir().unwrap();
        assert!(!is_enabled(config.path()));

        enable(config.path(), Path::new("/home/someone/.local/bin/qurb-desktop")).unwrap();
        assert!(is_enabled(config.path()));
        let text = std::fs::read_to_string(entry(config.path())).unwrap();
        assert!(text.contains("Exec=/home/someone/.local/bin/qurb-desktop --hidden"), "{text}");

        disable(config.path()).unwrap();
        assert!(!is_enabled(config.path()));
        disable(config.path()).unwrap();
    }

    /// The freedesktop way for a person to switch an entry off without
    /// deleting it. qurb must read that as off.
    #[test]
    fn an_entry_somebody_hid_is_off() {
        let config = tempfile::tempdir().unwrap();
        enable(config.path(), Path::new("/usr/bin/qurb-desktop")).unwrap();
        let path = entry(config.path());
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{text}Hidden=true\n")).unwrap();
        assert!(!is_enabled(config.path()));
    }
}
