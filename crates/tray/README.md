# qurb-tray

qurb in the corner of the screen.

```bash
qurb-tray ~/qurb
```

An icon showing whether things are in step, a menu with recent files and paired
devices, and a way to open the folder or quit. Where there is no tray, a window
with the same information **and the one setting people want to change**: how
much disk qurb may use. It **runs the daemon** — the same one `qurb run` does.

## One process, not two

This runs the daemon rather than talking to one. The alternative is a socket and
a protocol between them, which buys the ability to attach to an already-running
daemon and costs a second surface to design, version and secure. Two daemons on
one store collide on the SQLite lock anyway, so the real choice is *which one
process runs* — and `qurb run` stays for servers and replicas with no screen.

The daemon publishes a [`Status`](../qurb/src/status.rs) on a `watch` channel.
Watch rather than broadcast because a display only wants the *current* state: an
icon that fell behind and had to work through a queue of stale summaries would
be showing the past.

## In the applications menu

```bash
./packaging/install.sh
```

Copies the binaries to `~/.local/bin`, the icon to the user's icon theme, and a
`.desktop` file into the applications menu. Per-user on purpose: it needs no
root, touches nothing outside `$HOME`, and `--uninstall` genuinely undoes it. A
packaged build for distribution is a separate job.

The binaries are copied rather than symlinked into `target/`, because a menu
entry that stops working after `cargo clean` is worse than one that is slightly
stale.

## When there is no tray

**GNOME has no system tray.** It was removed, and GNOME ships no
StatusNotifier host, so a tray icon there is not ugly — it is *invisible*.

Worse, creating one still succeeds. Nothing returns an error; the icon is simply
never drawn. A program that trusted the return value would be running, unseen
and unquittable, which is the worst outcome available.

So [`host::available`](src/host.rs) asks the session bus whether anything has
registered `org.kde.StatusNotifierWatcher` *before* anything is built. With no
watcher it **opens a window instead** — [`window.rs`](src/window.rs), in GTK 3
because that is what `tray-icon` already links.

The window matters more than it sounds. Printing to stderr is fine for someone
who started the program in a terminal and useless for someone who launched it
from the applications menu, which is how a program is normally started — and
which would otherwise show nothing at all. Falling back to stderr only would
have made the menu entry above worse than useless on the one desktop this
machine runs.

The check happens before GTK is touched for a second reason: `tray-icon` is
GTK-backed on Linux and **panics** if a menu is constructed before `gtk::init`,
so an error path alone would never have run.

## The storage slider

The window is where a person sets how much disk qurb may have: a usage bar, a
checkbox for whether there is a limit at all, and a slider from 1 GiB to the
size of the filesystem the folder is on. Offering more than the disk holds
would be offering a number that cannot mean anything.

**It writes the settings file and nothing else.** Not a socket to the daemon,
not shared state — the same file `qurb config` writes and the daemon re-reads
on every maintenance pass. So the command and the slider are literally the same
act, a change applies without a restart, and neither can leave the other showing
something stale.

**It reads that file too**, rather than the daemon's published status. The
daemon only republishes on a sync pass, so a slider driven from the status would
sit at its old value for up to two minutes after somebody moved it — visibly
snapping back under their finger.

## One daemon per folder

The desktop app and `qurb run` in a terminal are the same daemon with different
faces, and it used to be possible to run both. Two on one store contend on the
SQLite write lock, answer as the same device on the network, and both enforce
the same storage cap — none of which reports an error. It is slow and confusing
rather than broken, which is worse.

An advisory `flock` on the store, taken for the life of the daemon, now makes
the second one refuse with a sentence saying what is already running. The kernel
releases it when the process dies, which a PID file could not promise after a
crash.

## The icon

Drawn at startup rather than shipped — a handful of RGBA pixels, because the
alternative is carrying PNGs and a decoder for an image sixteen pixels across.
Four looks: a closed ring when settled, a ring with a gap when working, grey
when nothing is reachable, and a warning colour for a problem. No animation; a
tray icon that animates is a tray icon people turn off.

## What it will not say

**"Up to date" when no device has been reached.** A sync tool that shows a
contented icon beside a store that has not spoken to another device in a week is
lying in the way that makes people stop trusting it. `State::Alone` exists for
that case and reads "no devices reachable".

It also distinguishes *no paired devices* from *paired but unreachable*: one is
a setup step, the other is a network problem, and they want different actions.

## Not built

- **A passphrase prompt.** A passphrase-protected store cannot be opened here;
  it says so and points at `qurb run` or `qurb protect`. A graphical prompt for
  a secret deserves more care than a text field.
- **Verified on a machine with a tray.** The fallback path is exercised — it is
  what this desktop does — but the icon and menu have never been seen. That
  needs KDE, XFCE, Windows, macOS, or the GNOME extension above.
- **Clicking a recent file.** The list is shown, not actionable.
- **Pairing a device.** Still `qurb pair` in a terminal, which is the step a
  new person hits first.
- **Anything about files.** No browsing what is synced, no fetching back a file
  the storage cap dropped, no undeleting.
- **Notifications.** `notify-rust` is a dependency and nothing sends one yet;
  conflicts are the obvious first use.
