# qurb-desktop

The desktop application: the same daemon, in a window.

```bash
cargo build --release -p qurb-desktop
./target/release/qurb-desktop ~/Sync
```

With no argument it opens the folder the CLI would, so launching from the
applications menu and running `qurb status` in a terminal address the same one.

## What it is

Five screens over `qurb_cli::View` and the daemon's status channel:

| screen | what it answers |
|---|---|
| Home | is it working, how many devices, what moved lately, what is still on its way |
| Files | what is in the folder and **where each file's contents actually are** |
| Devices | who is paired, when each was last reached |
| Activity | what this device did — the answer to "why is my file not here?" |
| Storage | what qurb costs on this disk, and the allowance |

The distinction the Files screen exists for is three-way. A file that is here
and also on the phone, and a file that is here and nowhere else in the world,
look identical to anything that only checks whether the bytes are on disk — and
offering to free the second is offering to delete it. So: **here**, **not
here**, **only here**, and only the last is drawn in a colour that asks for
attention.

## How it is put together

Tauri 2, a single stylesheet, and one file of plain JavaScript. No framework and
no build step: the application is five screens of lists and numbers, and a
bundler would be more moving parts than the thing it was moving.

```
src/main.rs       opens the key, starts the daemon on its own threads, opens
                  the window on this one
src/commands.rs   every question the window may ask, each a thin wrapper over
                  the engine
ui/index.html     the five screens
ui/app.css        one stylesheet, both colour schemes from the system
ui/app.js         what to do with an answer
```

Nothing in `ui/` decides anything about syncing. If it looks like it is
deciding something, that is a bug in the layering.

## Two rhythms

The daemon's live state — syncing, devices reachable — is polled every 1.5
seconds, because it changes many times a second and only the latest value is
useful. Lists are fetched when their screen is opened and refreshed on a much
slower beat, because redrawing a list somebody is reading is a cost rather than
a feature. See
[decision 0032](../../docs/decisions/0032-the-interface-hosts-the-daemon.md).

## Looking at the screens without a daemon

[`experiments/desktop-fixtures`](../../experiments/desktop-fixtures) serves this
window's real markup, stylesheet and script against made-up data, so layout can
be worked on without a folder, a paired device or a running daemon.

## What it does not do yet

- **No onboarding.** The folder has to be set up with `qurb init` first; the
  window says so and stops.
- **No pairing.** `qurb pair` here and `qurb join` there.
- **No sending.** `qurb send` does it; the window shows what is outstanding but
  cannot start one.
- **No transfer progress.** Outcomes are recorded and shown; a transfer in
  flight is not.
- **No passphrase prompt.** A passphrase-protected key is asked for on the
  terminal the application was launched from, and launching from a menu with
  one fails with a message saying so. The window cannot ask, because opening
  the key is what decides whether there is anything to show.
- **Linux only, in practice.** The Rust is portable and Tauri is
  cross-platform; this has never been built or run on Windows or macOS.
