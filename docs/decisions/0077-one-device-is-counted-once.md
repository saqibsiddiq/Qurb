# 0077 — One device is counted once, by its install

**Status:** Accepted — built 2026-10-31 and installed on the owner's laptop
and S23 the same evening (the S23's app updated in place, its data and
identity kept). Seen working on the laptop: within a minute of the phone
app opening, the laptop held its own marker and had learned skib phone's.
Not seen on the phone itself: its build is not debuggable, so its database
cannot be read from the laptop.
**Date:** 2026-10-31

## What happened

The owner's S23 was listed twice on the laptop: paired first as
"SM-S911B", then, its app data cleared, paired again under a new identity
and renamed "skib phone". The rule that lists a phone set up again as one
(item 6 of 2026-10-14) matched by name and could not see a rename; its
folder counted both pairings' backups (42 GB for 26.8 GB of content). A
manual "same as" was added the same day. The owner: "it is a problem if the
program is counting 1 device twice, we have fix this from root ... i want to
release this for public and this is a big bug."

## Why it happens

A device's identity is a key pair made when the app is set up. Clearing the
app's data or reinstalling it loses the key, and the next pairing is a new
device to everyone else. Nothing the other device saw — a name, a kind —
tells the same phone set up again from a second phone.

## Decision

1. **Every device has an install marker**: what its platform keeps for the
   app across a reinstall or cleared data — Android's `ANDROID_ID` (per app
   signing key, user and device; it changes only on a factory reset), a
   computer's machine id (`/etc/machine-id`, Windows' `MachineGuid`, a Mac's
   platform UUID) — made one way into 16 bytes
   (`qurb_storage::db::install_marker`), and never sent as it is.
2. **Told on request** (`Request::Install`), to a device it trusts, asked
   once by each side on a sync. A new request rather than a field in
   pairing: those messages refuse trailing bytes, and a build from before
   simply does not answer this one.
3. **One rule**, `Db::became`, which the computer's list, a phone's folder
   on a computer, and the phone's own list (`peers()` leaves out a device
   that became another) all ask, so none can count one device twice:
   - the person's word first, either way — *the same as*, or *not the same*;
   - then the same install;
   - then the same name and kind, for devices from before installs were told.
   Either rule also wants both the person's own, the newer paired after the
   older, and the older not seen since: two folders set up on one computer
   share its install and are two devices while both are in use.
4. **What is kept is counted once per content** (`Db::held_once`), so even a
   device listed twice could not double the size shown.

## Left undone

- **A factory reset** changes `ANDROID_ID`: a phone reset and set up again is
  matched by its name, or by the person's word.
- **Devices paired before this** have no marker on record until they next
  sync with a build that tells it; the owner's SM-S911B never will, and is
  linked by his word.
- What was kept for the earlier identity stays under it, reachable from the
  device it became; it is not moved over.
