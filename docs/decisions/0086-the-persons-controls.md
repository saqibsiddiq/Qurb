# 0086 — The person's controls, on the phone and the computer

**Status:** Accepted and built, 2026-11-12. The engine's parts are covered by
tests (`qurb_peer::pause`, `tests/pause.rs`, `qurb_cli::reset`, the config
round trip); the window's commands by `scripts/desktop_full.py`. **Not yet
tried on the owner's S23 or laptop by the owner.** What has and has not been
run is in [testing.md](../testing.md).

## What was asked

The owner, 2026-11-12: "check the setting section and lets decide what else
can we add and remove so that the user will have enough control of the
application". Shown both Settings side by side and the gaps, he chose:

1. Pause sync, both.
2. Mobile data, on the phone.
3. Backup timing, on the phone.
4. Notification kinds, both.
5. Ask before updating, both.
6. Reset this device, both.
7. App lock, on the phone.
8. Where received files are saved, on the phone.
9. Stay awake for backups, on the computer.
10. Send a report, both.
11. Tidy-ups: *Meeting point* on both (the phone said *Rendezvous*); the
    computer's identity shown once (Advanced and its Key page both had it);
    Recently deleted in the computer's Settings, as the phone has it.

Not chosen: a speed limit on the phone, how long Recently deleted keeps
files.

## Decided

1. **Pause** is one process-wide switch, `qurb_peer::pause`: paused until a
   time, or until resumed. Paused, a device runs no pass, says no news, and
   its server closes another device's connection at the next request. The
   other side sees it as off, the ordinary state of a sleeping laptop or
   phone, rather than failing file by file. Choices: an hour, until 8:00 the
   next morning, until resumed. The computer keeps it in its config file
   (`paused`), read at every pass, so the terminal's `qurb run` honours it
   too. The phone keeps it in the app's settings and tells the engine before
   each sync and when the engine opens. Home says *Paused* on both, with
   until when; tapping it offers *Resume*.
2. **Mobile data** on the phone: *Off* (nothing synced on mobile data: the
   same switch, held), *No backups* (as before: WhatsApp and chosen folders
   only on Wi-Fi), *Everything* (backups too; the backup job runs on any
   network). Android's own idea of metered decides what mobile data is.
3. **Backups**: *Nightly* while charging (as before), or *Every 6 h*,
   charging or not. Either way on Wi-Fi unless mobile data is *Everything*.
   A pause skips a run.
4. **Notification kinds**. The master switch stays; under it, each kind
   alone. The computer has *Files received*, *Delivered*, *Problems*
   (config `quiet`). The phone has the same three, plus *Folder requests*
   (a computer asking to open this phone's folder). The question on screen
   is still asked when the app is open; only the notification is muted.
5. **Auto-update**, on by default. Off on the computer: a newer release is
   found and named but not fetched, and *Update* fetches it, then restarts
   into it on a second tap. Nothing restarts the window by itself. Off on the
   phone: the build is fetched as before, but the background worker does not
   install it. *Update* in Settings does.
6. **Reset**: a second yes ("I have my 24 words, or another device has the
   key" on the computer; a warning on the phone). Then:
   - **Computer:** every paired device is removed, so each is told goodbye
     by the daemon (decision 0084). The folder is marked, start at login is
     turned off, and the window restarts. The next start, before the index
     opens, forgets the key (the keystore's copy too), deletes the store
     with its backups and the placeholders in `~/qurb`, and forgets the
     folder (`qurb_cli::reset`). The files in Shared stay, as ordinary
     files, and so do files sent here.
   - **Phone:** each computer is dialled and told goodbye
     (`Qurb::leave`), then Android's *Clear data*
     (`clearApplicationUserData`). Files saved out of the app stay.
7. **App lock** on the phone: opening the app after a minute away asks for
   the screen lock first, the app hidden until then; refused, it closes.
   Turning it on asks once, so it is known to work. Offered only when the
   phone has a screen lock.
8. **Received files** on the phone: *QurbSpace* (kept there only, as
   before), *Downloads* (also saved to Downloads/Qurb), or *Folder…*
   (also saved into a folder picked with Android's picker). Only files sent
   to this phone, after each sync. Turning it on saves nothing already
   received.
9. **Stay awake to collect** on the computer: decision 0062's inhibitor,
   on by default, can now be turned off (config `stay-awake`).
10. **Save a report**: this build, the system, the settings (no secret is
    kept there; on the phone, any key named like a token is left out), the
    devices, the last 200 events, and the log. That is the last three days
    of the computer's journal for `qurb-desktop`, or the phone app's own
    logcat. Saved to Downloads on the computer, or where the person picks on
    the phone. **Sent nowhere.** File names are in it, and the sheet says so.
11. The tidy-ups as asked.

## Consequences

- A paused computer still answers pairing; it is not a privacy mode.
- A device paused for a long time looks *off* to the others, which is the
  intent. A send to it waits under Waiting as it would for an off device.
- The reset deletes what this computer keeps for the owner's phones. Their
  only other copy is on the phone itself. The sheet says the backups go.

## Left undone

- None of it has been tried on the owner's devices yet: the 2026-11-12
  build carries it.
- The tray has no *Pause*; the window's Settings and Home do.
- The computer's reset is tested in its deleting half
  (`a_reset_deletes_qurbspace_and_leaves_the_files`); the restart between
  the halves is not run by any test.
- The phone's sheets, the app lock, mobile data, backup timing and saving
  received files out have no automated test; they are checked on the phone.
- *Folder…* writes into the picked folder through Android's documents
  interface. A folder on removable storage that is taken out loses the
  permission, and saving there fails quietly (logged).
- The phone's report reads only this app's log since the process started:
  Android keeps no more for an app.
