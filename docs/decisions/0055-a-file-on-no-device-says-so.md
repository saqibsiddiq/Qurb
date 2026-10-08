# 0055 — A file on no device says so

**Status:** Accepted — built on the desktop, the phone and the command line;
extends [0032](0032-the-interface-hosts-the-daemon.md) and
[0054](0054-a-file-the-other-device-does-not-hold.md)
**Date:** 2026-10-07

## What happened

After [0054](0054-a-file-the-other-device-does-not-hold.md), the 18 files lost
when the S23's data was cleared stopped failing on every sync, and were listed
on both devices as *Available elsewhere*. No device had them. The laptop had
freed them on the strength of the phone's old identity, which no longer
exists. The phone, set up again, had recorded their copies on the strength of
whoever made each file: 11 the old phone, which the new phone was never paired
with, and 7 the laptop, which had freed them. Asked to remove the old identity
and finish the job, the owner was owed a screen that says what is true.

Availability had three values (decision 0032), and every file not here was
*Elsewhere* by construction: `Availability::of(false, _)`.

## Decision

**A fourth value, `Nowhere`: not here, and no device this one can ask is
known to have it.** The window and the phone say *On no device*, with an icon
and in the attention colour, and offer only *Details* and *Delete*. There is
nothing to open and nowhere to fetch it from. The command line's `qurb ls`
marks it `nowhere`. It stays listed, so that it is not mistaken for a file
never made, and deleting it removes it from every device's list as any
deletion does.

**A copy counts as one to ask for only if it is on a paired device and not
marked out of reach** (`ASKABLE` in `crates/storage/src/db.rs`). That means
`replicas.private = 0` and a row in `peers`. A copy recorded for a device
never paired with this one, such as the device that made a file reached
through another, is not counted. Neither is one on a removed device, which
removal already marks ([0041](0041-removing-a-device.md), rule 5). This
decides what a listing shows, a file's *On* line in its details, and the
desktop's view.

**What a peer is recorded as holding is checked by asking it.** A copy is
recorded on the word of the device that made the file, or of a report, and
nothing took that back when the device freed its own. So at each sync a
device asks the other for the manifest of up to 16 files freed here that the
other is recorded as holding, each once (`check_holders`, a new `confirmed`
table, index schema 17). A manifest is given only for content held
([0054](0054-a-file-the-other-device-does-not-hold.md)). A copy confirmed is
not asked about again. A copy denied is marked out of reach, as a removed
device's is, and marked back if that device later reports holding it.

**A fetch answered "not held" does the same at once**, for the device that
answered (`ContentSource::not_held`). That is recorded after the file is
listed, since listing it notes its author as a holder.

**A copy marked out of reach is asked about too**, once, and counts again if
the device holds it (amended 2026-10-08). Removing a device marks all its
copies so ([0041](0041-removing-a-device.md), rule 5), and pairing it again
undid none of it. The owner removed the S23 from the laptop and paired it
again by scanning, and the laptop then called three files the only copy,
though the phone had just synced them. Removing a device also forgets what
it was asked, so pairing it again starts the asking afresh. Test:
`a_device_paired_again_has_its_copies_counted_again`.

**So is a shared file made by a device no longer paired**, once, when
nothing is recorded about the device being asked (amended 2026-10-08, later
the same day). A device reports holding a file only to the device that made
it (`unreported_to`), and learns of other copies only from a file's maker
and from asking. With the maker gone, nothing reports. A phone is always the
connecting side, and a report travels only that way, so the laptop's copies
were never told to the phone. The S23, set up again, showed one of its old
identity's photos as *Only copy here*. The laptop had it, and two others the
phone called the same. A held answer records the copy if none was recorded
(`note_held`). Test: `a_file_whose_maker_is_gone_is_asked_about_once`.

## Why this, and not something else

- **Leaving them *Available elsewhere*.** It is what the screen said, and it
  was false. Offering *Keep on this device* for a file nobody has turns a
  fact into a fetch that fails.
- **Hiding them.** A file that existed and is gone should not vanish without
  a word. Somebody looking for it should find out why it cannot be opened.
- **Deleting them.** That is the person's choice. *Delete* is one tap away
  on each.
- **Asking about every recorded copy at every sync.** Exact, and a request per
  freed file per sync, which a phone with hundreds freed cannot afford. Once
  each, a few at a time, is enough to correct a record that was wrong.
- **A flag on the wire saying which tree entries the sender holds.** Exact
  without any asking, and a protocol change that every device would have to
  take at once. Not needed for this.

## What it costs

- **Index schema 17.** A build at 16 cannot open an index a 17 has upgraded.
  The laptop's tools and window are updated together, and the upgrade keeps
  `index.before-schema-17.db`.
- **A confirmation can go stale.** A device confirmed as holding a file may
  free it later. Opening the file then asks, is answered "not held", and the
  file is shown on no device from then on.
- **So can a denial, for a file whose maker is gone.** Asked once and told
  "not held", this device does not learn of it if the other takes the file
  later from somewhere else. It errs towards *only copy here*, the safe
  side for freeing.
- **Freeing still counts a copy on a device never paired.** The display is
  the stricter of the two. A copy recorded for an unpaired device can still
  let this device free its own (`SAFE_ELSEWHERE` is unchanged). Such a copy
  arises only for a file made by a device this one reaches through another.
  The file is then not lost, but it may not be fetchable from here. Not
  changed here, to keep this to what is shown.

## Checked, and not

- `crates/qurb/tests/view.rs`: `a_file_whose_keeper_is_removed_reads_as_on_no_device`,
  `a_copy_on_a_device_never_paired_is_not_counted`, and the earlier view
  tests, now pairing the device that holds the copy.
- `crates/peer/tests/delivery.rs`: `a_copy_recorded_for_a_peer_is_asked_about_once`,
  `a_file_whose_maker_is_gone_is_asked_about_once`, and `a_file_the_peer_does_not_hold_is_listed_not_failed_every_sync`, now
  checking the file reads as on no device.
- `crates/peer/tests/holding.rs`: a phone's own file freed while a desktop
  keeps it (decision 0036) is confirmed when asked, and still reads as
  available elsewhere.
- On the laptop, 2026-10-07: removing the old phone identity
  (`qurb remove-device 7a4ebf0c --yes`, keeping the 244 KiB file kept for
  it), then `qurb ls` on the new build: 18 `nowhere`, 3 `here`.
- On the S23, the same evening: one sync asked the laptop about the 7 files it
  was recorded as holding and marked all 7 out of reach. The other 11 were
  recorded only for the old identity, never paired with this phone. Files
  showed all 18 as *On no device*.
- The phone's sheet for such a file, seen the same evening: *On no device*,
  the reason, and only *Delete*.
- The window's row, menu and details panel for such a file, against its
  fixtures: *Details* and *Delete* only, the reason, "On: No device".
- On the S23, 2026-10-08, with the build that asks about files whose maker
  is gone: one sync asked about the 14 the old identity made. The 3 the
  laptop holds read *On this phone* instead of *Only copy here*; the 11
  nobody holds stayed *On no device*.
- **Not watched**: the real window showing one; a confirmation going stale.
