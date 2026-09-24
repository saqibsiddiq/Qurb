# qurb-desktop

The desktop application: the same daemon, in a window.

```bash
cargo build --release -p qurb-desktop
./target/release/qurb-desktop ~/Sync
```

With no argument it opens the folder the CLI would, so launching from the
applications menu and running `qurb status` in a terminal address the same one.

## What it is

Six screens over `qurb_cli::View` and the daemon's status channel, plus setting
a device up in the first place:

| screen | what it answers |
|---|---|
| Home | is it working, how many devices, what moved lately, what is still on its way |
| Files | what is in the folder and **where each file's contents actually are** |
| Devices | who is paired, and pairing with another: show a code or enter one |
| Activity | what this device did — the answer to "why is my file not here?" |
| Storage | what qurb costs on this disk, and the allowance |
| Send | a file to one device, by dropping it on the window or choosing one |
| Settings | this device's name, how it finds the others, and the 24 words |

A folder with no device in it opens the setting-up flow instead: make a new
qurb, or add this device to one that exists. Setting a device up is the job of
a screen, so it cannot be a precondition of the screen existing.

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
src/main.rs       opens the window, and the daemon too if there is a device
src/session.rs    whether there is a device yet, the daemon once there is, and
                  the recovery phrase for the moment between showing it and
                  having it confirmed
src/commands.rs   every question the window may ask, each a thin wrapper over
                  the engine
ui/index.html     the screens, and the setting-up steps
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

## The 24 words

The phrase is held in the session rather than in the page: created, fetched
once to be drawn, checked against when three of the words are confirmed, and
dropped the moment that succeeds. The page can therefore drop its copy as soon
as it has drawn the list, and what crosses back is a yes or no rather than a
key. It is never logged, never persisted, and never put in debugging output.

It can be shown again from Settings, derived from the key that is already in
the folder — anybody who can read that folder can read the files, so this
reveals nothing new. See
[decision 0033](../../docs/decisions/0033-the-phrase-on-a-screen.md).

## Pairing

Show a code — a QR to scan, the same code to type, and a spoken form to read
down a telephone — or enter one from another device. The screen counts down to
the code's expiry rather than saying "waiting" under a code that stopped
working five minutes ago.

Two things worth knowing. The QR is drawn black on white whatever colour scheme
the desktop is in, because a scanner finds a code by its finder patterns
against a light ground and a code drawn dark-on-dark is not a code. And the
window binds port zero rather than the configured port, because the daemon in
this process already has that one — see
[decision 0032](../../docs/decisions/0032-the-interface-hosts-the-daemon.md).

## Sending

Drop a file on the window, or choose one, then pick a device. It goes to that
device and to nowhere else: your other devices never see it, and it is not added
to the synced folder.

Dragging is the better gesture and needs no plugin — Tauri reports the drop to
the window, and only the *path* crosses into the page, never the contents. The
"Choose a file…" button does the same thing for anybody who cannot drag.

## Notifications

Three things, and nothing else:

- somebody sent you a file — a thing another person did, on purpose, for you;
- a device collected what you sent it;
- something failed.

Ordinary syncing is silent, and so is pairing, eviction, and every file that
arrives because it was in a shared folder. A sync application that announced
every file it moved would be switched off within a day.

Raised from Rust rather than from the page, because a notification is most
useful exactly when nobody is looking at the window.

## What it does not do yet

- **No transfer progress.** Outcomes are recorded and shown; a transfer in
  flight is not.
- **One file at a time.** Dropping several takes the first and says so. A queue
  is a different interaction, with something to say about partial failure.
- **No passphrase prompt.** A passphrase-protected key is asked for on the
  terminal the application was launched from. The window cannot ask, because
  opening the key is what decides whether there is anything to show; launched
  from a menu it says so on the first screen.
- **No folder picker.** A text field with `~` expansion and a live description
  of what is already there.
- **Linux only, in practice.** The Rust is portable and Tauri is
  cross-platform; this has never been built or run on Windows or macOS.
