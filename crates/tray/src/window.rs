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

    // Shown because the daemon can fail while the window is up -- an
    // unreachable rendezvous service, most often -- and a window that goes on
    // reporting files and folders while the thing underneath has stopped is
    // worse than one that says nothing. Found exactly that way: the daemon
    // stopped on startup and the window showed no sign of it.
    let problem = label();
    problem.set_no_show_all(true);

    let recent_title = label();
    recent_title.set_markup("<b>Recently synced</b>");
    let recent = label();
    recent.set_opacity(0.75);

    column.pack_start(&state, false, false, 0);
    column.pack_start(&detail, false, false, 0);
    column.pack_start(&path, false, false, 0);
    column.pack_start(&problem, false, false, 0);
    column.pack_start(&recent_title, false, false, 12);
    column.pack_start(&recent, true, true, 0);

    // -- the storage allowance -----------------------------------------------

    let allowance_title = label();
    allowance_title.set_markup("<b>Storage allowance</b>");

    let usage_bar = gtk::ProgressBar::new();
    usage_bar.set_show_text(true);

    let cap = gtk::CheckButton::with_label("Limit how much space qurb may use");

    // In gibibytes, because that is the unit a person thinks in when deciding
    // how much of a disk to give away. The maximum is the size of the
    // filesystem the folder is on: offering more than the disk holds is
    // offering a number that cannot mean anything.
    let most = gibibytes_available(&root).max(2.0);
    let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, most, 1.0);
    slider.set_draw_value(false);
    slider.set_hexpand(true);

    let chosen = label();
    chosen.set_opacity(0.75);

    let allowance = gtk::Box::new(gtk::Orientation::Vertical, 6);
    allowance.pack_start(&allowance_title, false, false, 0);
    allowance.pack_start(&usage_bar, false, false, 0);
    allowance.pack_start(&cap, false, false, 0);
    allowance.pack_start(&slider, false, false, 0);
    allowance.pack_start(&chosen, false, false, 0);
    column.pack_start(&allowance, false, false, 14);

    // Written to the config file, which is where the daemon reads it from on
    // every maintenance pass. That is deliberately the only channel between
    // this window and the running daemon: `qurb config limit=10G` in a
    // terminal and this slider are then the same act, and neither can leave
    // the other showing something stale.
    let store = qurb_cli::store_dir(&root);
    let save = {
        let store = store.clone();
        move |bytes: u64| match qurb_cli::config::Config::load(&store) {
            Ok(mut config) => {
                if config.limit != bytes {
                    config.limit = bytes;
                    if let Err(e) = config.save(&store) {
                        tracing::warn!(error = %e, "could not save the storage allowance");
                    }
                }
            }
            Err(e) => tracing::warn!(error = %e, "could not read the settings"),
        }
    };

    // Set while the window is catching up with the daemon, so that filling the
    // controls in does not look like the person moving them.
    let settling = std::rc::Rc::new(std::cell::Cell::new(true));

    {
        let save = save.clone();
        let slider_for_cap = slider.clone();
        let settling = std::rc::Rc::clone(&settling);
        cap.connect_toggled(move |cap| {
            slider_for_cap.set_sensitive(cap.is_active());
            if settling.get() {
                return;
            }
            save(if cap.is_active() { gib(slider_for_cap.value()) } else { 0 });
        });
    }
    {
        let save = save.clone();
        let cap_for_slider = cap.clone();
        let settling = std::rc::Rc::clone(&settling);
        slider.connect_value_changed(move |slider| {
            if settling.get() || !cap_for_slider.is_active() {
                return;
            }
            save(gib(slider.value()));
        });
    }

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

        match &status.problem {
            Some(text) => {
                problem.set_markup(&format!(
                    "<span foreground='#c0392b'>⚠ {}</span>",
                    escape(text)
                ));
                problem.show();
            }
            None => problem.hide(),
        }

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

        // Read from the settings file rather than from the daemon's published
        // status. The daemon only republishes on a sync pass, so a slider
        // driven from the status would sit at the old value for up to two
        // minutes after someone moved it -- visibly snapping back under their
        // finger. The file is what this window writes and what the daemon
        // reads, so taking it as the source of truth makes the window agree
        // with itself immediately and with a change made in a terminal within
        // one poll.
        let limit = qurb_cli::config::Config::load(&store).map(|c| c.limit).unwrap_or(0);

        settling.set(true);
        if limit == 0 {
            if cap.is_active() {
                cap.set_active(false);
            }
            usage_bar.set_fraction(0.0);
            usage_bar.set_text(Some(&format!("{} used", crate::ui::human(status.used))));
            chosen.set_text("No limit — qurb will use what it needs.");
        } else {
            if !cap.is_active() {
                cap.set_active(true);
            }
            let wanted = (limit as f64 / GIB).round().clamp(1.0, most);
            if (slider.value() - wanted).abs() > 0.5 {
                slider.set_value(wanted);
            }
            let fraction = (status.used as f64 / limit as f64).clamp(0.0, 1.0);
            usage_bar.set_fraction(fraction);
            usage_bar.set_text(Some(&format!(
                "{} of {}",
                crate::ui::human(status.used),
                crate::ui::human(limit)
            )));
            chosen.set_text(&if status.used > limit {
                format!(
                    "Over by {}. qurb keeps files no other device has, even to stay under.",
                    crate::ui::human(status.used - limit)
                )
            } else {
                format!("{} free within the allowance.", crate::ui::human(limit - status.used))
            });
        }
        settling.set(false);

        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
    Ok(())
}

const GIB: f64 = (1u64 << 30) as f64;

fn gib(value: f64) -> u64 {
    (value.max(1.0) * GIB) as u64
}

/// How many gibibytes the filesystem holding `root` has in total.
///
/// The slider's maximum. A person deciding how much of their disk to give
/// away is choosing within what the disk actually has, and a slider that runs
/// past it would offer numbers that cannot mean anything.
fn gibibytes_available(root: &std::path::Path) -> f64 {
    let path = match std::ffi::CString::new(root.as_os_str().as_encoded_bytes()) {
        Ok(path) => path,
        Err(_) => return 64.0,
    };

    // SAFETY: `statvfs` writes into the struct and reads a NUL-terminated
    // path, both of which hold here. A failure leaves it untouched, which is
    // why the return value is checked before anything is read out.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) != 0 {
            return 64.0;
        }
        (stat.f_blocks as f64 * stat.f_frsize as f64) / GIB
    }
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
