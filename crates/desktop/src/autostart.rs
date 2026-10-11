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

use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME`, or `~/.config`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

pub fn entry(config: &Path) -> PathBuf {
    config.join("autostart").join("qurb.desktop")
}

pub fn is_enabled(config: &Path) -> bool {
    std::fs::read_to_string(entry(config))
        .map(|text| !text.lines().any(|l| l.trim() == "Hidden=true"))
        .unwrap_or(false)
}

/// Start `program` at login, with its window hidden.
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

pub fn disable(config: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(entry(config)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
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
