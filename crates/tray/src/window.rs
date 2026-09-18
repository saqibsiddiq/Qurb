//! A window, for desktops that cannot show a tray icon.
//!
//! GNOME removed the system tray, so on the most common Linux desktop there is
//! nowhere to put an icon. Printing to stderr is fine when someone started the
//! program in a terminal and useless when they launched it from the
//! applications menu — which is the normal way to start a program, and the way
//! that would otherwise produce nothing visible at all.
//!
//! So: the same information, in a small window. Deliberately plain. This is a
//! status readout, not a file manager, and every line answers a question people
//! actually ask — is it working, how much is here, can it reach my devices.
//!
//! GTK 3, because that is what `tray-icon` already links on Linux. A second
//! toolkit for one window would double the dependency for no gain.

use gtk::prelude::*;
use qurb_cli::status::{State, Watcher};
use std::path::PathBuf;

/// Show the window and run until it is closed.
///
/// Closing it quits: there is no tray to retreat into, so a hidden window
/// would recreate the invisible-process problem this exists to avoid.
pub fn show(root: PathBuf, watcher: Watcher) -> anyhow::Result<()> {
    // GTK is already initialised by the caller.
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("qurb");
    window.set_default_size(420, 280);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
    column.set_margin_top(22);
    column.set_margin_bottom(22);
    column.set_margin_start(26);
    column.set_margin_end(26);

    let state = label();
    let detail = label();
    let path = label();
    path.set_markup(&format!("<span size='small'>{}</span>", escape(&root.display().to_string())));
    path.set_opacity(0.55);

    let recent_title = label();
    recent_title.set_markup("<b>Recently synced</b>");
    let recent = label();
    recent.set_opacity(0.75);

    column.pack_start(&state, false, false, 0);
    column.pack_start(&detail, false, false, 0);
    column.pack_start(&path, false, false, 0);
    column.pack_start(&recent_title, false, false, 12);
    column.pack_start(&recent, true, true, 0);

    let open = gtk::Button::with_label("Open folder");
    let folder = root.clone();
    open.connect_clicked(move |_| {
        let _ = std::process::Command::new("xdg-open").arg(&folder).spawn();
    });
    column.pack_end(&open, false, false, 0);

    window.add(&column);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        gtk::glib::Propagation::Proceed
    });
    window.show_all();

    // Polled rather than driven by the channel: GTK's main loop owns this
    // thread, and a blocking wait on the watcher would freeze the window. Half
    // a second is well under what anyone notices in a status readout.
    let mut watcher = watcher;
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        let status = watcher.borrow_and_update().clone();

        state.set_markup(&format!(
            "<span size='xx-large' weight='bold'>qurb — {}</span>",
            escape(status.state.summary())
        ));
        detail.set_text(&format!(
            "{} file{} · {} · {}",
            status.files,
            if status.files == 1 { "" } else { "s" },
            crate::ui::human(status.bytes_on_disk),
            crate::ui::devices(&status),
        ));
        state.set_opacity(match status.state {
            State::Alone => 0.6,
            _ => 1.0,
        });

        let lines: Vec<String> = status
            .recent
            .iter()
            .take(5)
            .map(|e| format!("{}   {}", crate::ui::shorten(&e.path), crate::ui::ago(e.at)))
            .collect();
        recent.set_text(&if lines.is_empty() {
            "Nothing yet.".to_string()
        } else {
            lines.join("\n")
        });

        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
    Ok(())
}

fn label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label
}

/// Escape text before it goes into Pango markup.
///
/// Filenames reach these labels, and a file called `a<b` would otherwise turn
/// the rest of the label into a parse error and blank it. Not a security
/// boundary -- the text is the user's own -- but a correctness one.
fn escape(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}
