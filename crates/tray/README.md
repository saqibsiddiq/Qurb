# qurb-tray

qurb in the corner of the screen.

```bash
qurb-tray ~/qurb
```

An icon showing whether things are in step, a menu with recent files and paired
devices, and a way to open the folder or quit. It **runs the daemon** — the same
one `qurb run` does.

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

## When there is no tray

**GNOME has no system tray.** It was removed, and GNOME ships no
StatusNotifier host, so a tray icon there is not ugly — it is *invisible*.

Worse, creating one still succeeds. Nothing returns an error; the icon is simply
never drawn. A program that trusted the return value would be running, unseen
and unquittable, which is the worst outcome available.

So [`host::available`](src/host.rs) asks the session bus whether anything has
registered `org.kde.StatusNotifierWatcher` *before* anything is built. With no
watcher, the program says so, explains how to get a tray back on GNOME, and
keeps syncing with status on stderr. Degrading to a working terminal program
beats pretending.

The check happens before GTK is touched for a second reason: `tray-icon` is
GTK-backed on Linux and **panics** if a menu is constructed before `gtk::init`,
so an error path alone would never have run.

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
- **Notifications.** `notify-rust` is a dependency and nothing sends one yet;
  conflicts are the obvious first use.
