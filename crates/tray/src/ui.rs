//! The tray icon and its menu, and what to do when there is no tray.

use crate::icon::{self, Look};
use anyhow::{Context, Result};
use qurb_cli::status::{Status, Watcher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Show the interface and run until the user quits.
pub fn show(root: PathBuf, watcher: Watcher) -> Result<()> {
    // Asked before anything is built. Creating a tray icon *succeeds* on a
    // desktop that cannot show one -- it is simply never drawn -- so the only
    // way to avoid becoming an invisible, unquittable background process is to
    // find out first.
    if let Err(why) = crate::host::available() {
        eprintln!("qurb: no system tray available ({why})");
        if let Some(advice) = crate::host::advice() {
            eprintln!();
            eprintln!("{advice}");
        }
        eprintln!();

        // A window instead. Printing to stderr is fine for someone who started
        // this in a terminal and useless for someone who launched it from the
        // applications menu -- which is how a program is normally started, and
        // which would otherwise show nothing at all.
        #[cfg(target_os = "linux")]
        if gtk::init().is_ok() {
            eprintln!("  Showing a window instead.");
            eprintln!();
            return crate::window::show(root, watcher);
        }

        eprintln!("  Syncing regardless. Ctrl-C to stop.");
        eprintln!();
        return run_headless(watcher);
    }

    // GTK, which `tray-icon` is built on here, and which panics rather than
    // returning an error if a menu is constructed before it is initialised.
    #[cfg(target_os = "linux")]
    if let Err(e) = gtk::init() {
        eprintln!("qurb: could not start GTK ({e}); syncing without an icon.");
        return run_headless(watcher);
    }

    match build(&root, &watcher) {
        Ok(tray) => run_with_tray(root, watcher, tray),
        Err(e) => {
            eprintln!("qurb: could not show a tray icon ({e}); syncing without one.");
            run_headless(watcher)
        }
    }
}

/// The menu, rebuilt whenever the status changes.
///
/// Rebuilt rather than mutated because a tray menu is small and the platforms
/// differ in what they allow changing in place; constructing four items is not
/// a cost worth optimising against correctness.
struct Items {
    menu: Menu,
    open: MenuItem,
    quit: MenuItem,
}

fn build(root: &Path, watcher: &Watcher) -> Result<TrayIcon> {
    let status = watcher.borrow().clone();
    let (pixels, w, h) = icon::render(Look::from(status.state));

    let items = menu(&status);
    let icon = tray_icon::Icon::from_rgba(pixels, w, h).context("building the icon")?;

    TrayIconBuilder::new()
        .with_tooltip(tooltip(&status))
        .with_menu(Box::new(items.menu))
        .with_icon(icon)
        .build()
        .with_context(|| format!("showing a tray icon for {}", root.display()))
}

fn menu(status: &Status) -> Items {
    let menu = Menu::new();

    // The headline, not clickable: a menu that opens to tell you nothing is
    // a menu nobody opens twice.
    let heading = MenuItem::new(format!("qurb — {}", status.state.summary()), false, None);
    let _ = menu.append(&heading);

    let detail = MenuItem::new(
        format!(
            "{} file{} · {} · {}",
            status.files,
            if status.files == 1 { "" } else { "s" },
            human(status.bytes_on_disk),
            devices(status),
        ),
        false,
        None,
    );
    let _ = menu.append(&detail);

    if !status.recent.is_empty() {
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::new("Recently synced", false, None));
        for entry in status.recent.iter().take(5) {
            let _ = menu.append(&MenuItem::new(
                format!("    {}   {}", shorten(&entry.path), ago(entry.at)),
                false,
                None,
            ));
        }
    }

    if let Some(problem) = &status.problem {
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::new(format!("⚠ {problem}"), false, None));
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let open = MenuItem::new("Open folder", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let _ = menu.append(&open);
    let _ = menu.append(&quit);

    Items { menu, open, quit }
}

fn run_with_tray(root: PathBuf, mut watcher: Watcher, tray: TrayIcon) -> Result<()> {
    // Rebuilt on each change so the item ids stay in step with the menu the
    // user is looking at.
    let mut items = menu(&watcher.borrow().clone());
    tray.set_menu(Some(Box::new(items.menu)));
    let (mut open_id, mut quit_id) = (items.open.id().clone(), items.quit.id().clone());

    let menu_events = MenuEvent::receiver();

    loop {
        // Polled rather than driven by a platform event loop. A tray icon needs
        // one on macOS and Windows; on Linux the StatusNotifier item is served
        // over D-Bus by `tray-icon`'s own thread, so a plain loop suffices and
        // avoids pulling a window library in to do nothing but tick.
        std::thread::sleep(Duration::from_millis(200));

        while let Ok(event) = menu_events.try_recv() {
            if event.id == quit_id {
                return Ok(());
            }
            if event.id == open_id {
                open_folder(&root);
            }
        }

        if watcher.has_changed().unwrap_or(false) {
            let status = watcher.borrow_and_update().clone();

            if let Ok(icon) = tray_icon::Icon::from_rgba(
                icon::render(Look::from(status.state)).0,
                32,
                32,
            ) {
                let _ = tray.set_icon(Some(icon));
            }
            tray.set_tooltip(Some(tooltip(&status))).ok();

            items = menu(&status);
            open_id = items.open.id().clone();
            quit_id = items.quit.id().clone();
            tray.set_menu(Some(Box::new(items.menu)));
        }
    }
}

/// No tray: keep syncing, and say what changes.
///
/// Deliberately quiet. This prints when the *state* changes rather than on
/// every update, because a background program that scrolls a terminal is a
/// background program someone kills.
fn run_headless(mut watcher: Watcher) -> Result<()> {
    let mut last = String::new();
    loop {
        let status = watcher.borrow_and_update().clone();

        // Compared as the rendered line rather than by state.
        //
        // Printing only on a state change meant a file count that grew while
        // the state stayed "up to date" was never shown -- the display sat on
        // "5 files" with six in the index, which is precisely the kind of quiet
        // wrongness an interface exists to prevent. Comparing the text shows
        // every change worth showing and cannot repeat itself.
        let line = format!(
            "qurb: {} — {} file{}, {}",
            status.state.summary(),
            status.files,
            if status.files == 1 { "" } else { "s" },
            devices(&status),
        );
        if line != last {
            println!("{line}");
            last = line;
        }

        // Polled rather than awaited: `Receiver::changed` is async and this
        // thread is the interface's, not the runtime's. Two hundred
        // milliseconds is far below what a person notices and far above what
        // costs anything.
        std::thread::sleep(Duration::from_millis(200));

        // Every sender gone means the daemon thread has ended.
        if watcher.has_changed().is_err() {
            // Read once more before giving up. The daemon reports *why* it is
            // stopping and then stops, so the last update and the end of the
            // channel arrive together -- and printing the state from before
            // that update means telling the user the last thing that was true
            // rather than the thing that went wrong.
            let final_status = watcher.borrow().clone();
            if let Some(problem) = final_status.problem {
                anyhow::bail!("{problem}");
            }
            anyhow::bail!("the daemon stopped");
        }
    }
}

fn open_folder(root: &Path) {
    // Whatever the desktop uses. Failure is not worth reporting: the user
    // asked to see a folder, not to be told about `xdg-open`.
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";

    let _ = std::process::Command::new(opener).arg(root).spawn();
}

fn tooltip(status: &Status) -> String {
    format!("qurb — {}\n{}", status.state.summary(), devices(status))
}

pub(crate) fn devices(status: &Status) -> String {
    match (status.peers, status.peers_reachable) {
        (0, _) => "no paired devices".to_string(),
        (n, 0) => format!("{n} device{} · none reachable", plural(n)),
        (n, r) if r == n => format!("{n} device{}", plural(n)),
        (n, r) => format!("{r} of {n} devices reachable", n = n, r = r),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// A path short enough for a menu, keeping the end.
///
/// The end, because that is where the filename is — a truncated
/// `Documents/Projects/2026/…` tells you nothing about which file arrived.
pub(crate) fn shorten(path: &str) -> String {
    const MAX: usize = 32;
    if path.chars().count() <= MAX {
        return path.to_string();
    }
    let tail: String = path.chars().rev().take(MAX - 1).collect::<Vec<_>>().into_iter().rev().collect();
    format!("…{tail}")
}

pub(crate) fn ago(then: SystemTime) -> String {
    let Ok(elapsed) = then.elapsed() else { return "just now".into() };
    let seconds = elapsed.as_secs();
    match seconds {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}

pub(crate) fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_path_keeps_its_filename() {
        let long = "Documents/Projects/2026/quarterly/attachments/the-actual-file.pdf";
        let short = shorten(long);
        assert!(short.chars().count() <= 32, "still {} chars", short.chars().count());
        assert!(short.ends_with("the-actual-file.pdf"), "lost the filename: {short}");
    }

    #[test]
    fn a_short_path_is_untouched() {
        assert_eq!(shorten("notes.txt"), "notes.txt");
    }

    /// "no paired devices" and "none reachable" mean different things and must
    /// never be conflated: one is a setup step, the other is a network problem.
    #[test]
    fn device_counts_distinguish_unpaired_from_unreachable() {
        let mut status = Status::starting(PathBuf::from("/tmp"), "abcd1234".into());
        assert_eq!(devices(&status), "no paired devices");

        status.peers = 2;
        status.peers_reachable = 0;
        assert_eq!(devices(&status), "2 devices · none reachable");

        status.peers_reachable = 2;
        assert_eq!(devices(&status), "2 devices");

        status.peers_reachable = 1;
        assert_eq!(devices(&status), "1 of 2 devices reachable");
    }

    #[test]
    fn sizes_read_as_people_write_them() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(4_876_562), "4.7 MB");
    }
}
