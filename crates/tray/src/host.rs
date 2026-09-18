//! Whether anything would actually display a tray icon.
//!
//! Creating one succeeds on a desktop that has no tray. The icon is simply
//! never drawn, and no return value says so — which on GNOME means a background
//! program that is running, invisible, and unquittable. Asking first is the
//! only way to know.
//!
//! The question is answered by the freedesktop StatusNotifierItem
//! specification: a host registers `org.kde.StatusNotifierWatcher` on the
//! session bus, and items check for it. No watcher, no tray.

/// Whether a tray icon would be visible if we made one.
#[cfg(target_os = "linux")]
pub fn available() -> Result<(), String> {
    // A blocking connection on the calling thread: this runs once at startup
    // before the interface exists, and pulling in an async context for a single
    // name lookup would cost more than it saves.
    let connection = zbus::blocking::Connection::session()
        .map_err(|e| format!("no session bus: {e}"))?;

    let proxy = zbus::blocking::fdo::DBusProxy::new(&connection)
        .map_err(|e| format!("could not query the session bus: {e}"))?;

    let names = proxy
        .list_names()
        .map_err(|e| format!("could not list bus names: {e}"))?;

    // The watcher is the host. Matched by prefix because some desktops register
    // it under a suffixed name.
    let has_watcher = names
        .iter()
        .any(|name| name.as_str().starts_with("org.kde.StatusNotifierWatcher"));

    if has_watcher {
        Ok(())
    } else {
        Err("no StatusNotifierWatcher on the session bus".to_string())
    }
}

/// Every other platform has a tray and no way to ask about it.
#[cfg(not(target_os = "linux"))]
pub fn available() -> Result<(), String> {
    Ok(())
}

/// Advice for a desktop with no tray, or `None` where none applies.
pub fn advice() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if desktop.to_ascii_uppercase().contains("GNOME") {
            return Some(
                "  GNOME removed the system tray. To get one back, install the\n  \
                 \"AppIndicator and KStatusNotifierItem Support\" extension from\n  \
                 https://extensions.gnome.org and log out and back in.",
            );
        }
        Some(
            "  Nothing on this desktop is offering to display tray icons.\n  \
             On most desktops that means the panel's system tray or\n  \
             notification area is switched off.",
        )
    }
    #[cfg(not(target_os = "linux"))]
    None
}
