# 0083 — The desktop's Files open in the computer's file manager

**Status:** Accepted and built, 2026-11-03, from the owner's review of the
desktop that day ([desktop-review-2026-11-03](../design/desktop-review-2026-11-03.md),
F-1 to F-5). **Not yet tried on the owner's laptop.** Amends
[0067](0067-a-folder-for-each-person.md): a person's folder is no longer
"opened inside Qurb only"; its rule that nothing readable is left on disk
stands.

## What was asked

The owner, shown a desktop Files page modelled on the phone's: "maintaining a
separate file system in the qurb application is not necessary, how about
when i click on shared, it just opens shared folder in the file app and if i
click on saqib it just gives me access in the file app itself." Asked whether
to mount a person's folder while it is open, and whether a computer should
keep everything in Shared: yes to both.

## Decided

- **Files is a launcher.** *This computer*: Shared and Received, ordinary
  folders, opened in the file manager. *People*: each person's folder.
  QurbSpace keeps no browser, previews, selection or search of its own for
  them; the file manager's are better.
- **A person's folder is mounted while it is open** (`crates/desktop/src/mount.rs`):
  after the yes on their phone, a read-only FUSE file system at the person's
  placeholder in `~/qurb`, through the system's `fusermount3` (`fuser`, without
  libfuse). The owner's files are read from the store a chunk at a time as
  they are asked for; a guest's, sealed whole, are unsealed on first read into
  memory-backed space (`$XDG_RUNTIME_DIR`), as opening one did before. Locked
  -- *Lock*, ten minutes unused, the computer asleep -- the folder unmounts and
  the placeholder is back. The placeholder layout leaves a mounted folder
  alone (`layout::mounted`).
- **One locked folder per person.** This computer's own private files appear
  in its owner's folder as *This computer*, read from where they are on disk.
- **A computer keeps everything** (F-3): the daemon no longer drops local
  copies to stay under a size, so the folder the file manager shows is always
  whole. The limit stays a phone's. The phone's Manage storage measures a
  computer against its disk's free space instead.

## Consequences

- **Read-only.** Nothing can be put into or changed in a person's folder
  through the mount. Adding to the Vault from the desktop is not offered.
- **Linux only.** Windows would need a driver of its own (WinFsp) and macOS
  macFUSE; there the folder cannot be opened from the desktop until that is
  done.
- Received loses who sent each file, which a file manager cannot show; the
  notification and Activity still say it.
- The window's old browser code for Shared, the Vault and people's folders is
  unreachable from Files and is to be removed once the mount has been used.

## Not yet known

How the mount behaves under a file manager that reads every file for
thumbnails over a phone's 37 GB backup; how long the first listing of a large
folder takes. To be measured on the owner's laptop.
