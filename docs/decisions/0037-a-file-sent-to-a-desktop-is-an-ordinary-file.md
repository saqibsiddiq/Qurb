# 0037 — A file sent to a desktop is an ordinary file in Downloads

**Status:** Accepted — built 2026-09-25
**Date:** 2026-09-25

Amends [0023](0023-one-person-per-account.md) on the default location, and
[0030](0030-sending-a-file-to-one-device.md) rule 3 on devices that name a
downloads directory.

## Decision

A device may name a **downloads directory**. When it takes a delivery, it writes
the file there as an ordinary file and stops tracking it. The file is not
indexed as content, not scanned, not counted against the storage allowance and
not evictable, and deleting it there has nothing to do with qurb.

**Desktops name one by default:** `qurb` inside the Downloads folder, with
`XDG_DOWNLOAD_DIR` honoured as in 0023. **Phones do not.** A delivery to a phone
is filed in the phone's own vault, which is the brief's model there (§22).

That path is where 0023 put the synced folder, so the default for a **new**
synced folder moves back to `~/qurb`. Existing folders stay where they are, as
0023 already promised.

Chosen by the project owner on 2026-09-25; see
[product-plan §3.2](../product-plan.md).

## Why

The brief separates two kinds of storage on a desktop (§5): qurb storage, which
the allowance governs, and ordinary files that happen to have arrived through
qurb. Somebody who deletes a received file in their file manager is tidying
their Downloads folder, and qurb has no business treating that as anything.

It also removes, on the desktop, a class of bug found on 2026-09-24: received
files sitting in the synced folder, where every scan has to be taught that they
are not part of the shared area. A file that is not in the folder is never seen
by the scan at all.

## What qurb still remembers

- **That it took the delivery**, keyed by the send -- sender, name and
  version -- so that a send offered again is not taken twice (0030 rule 2;
  by content until [0059](0059-a-send-is-not-its-bytes.md), which kept the
  same file sent again from ever arriving). The record survives the file being deleted from Downloads;
  rule 2 counts tombstones for exactly this reason.
- **That the sender was told, and only once it was true.** The file is written
  under a temporary name in the same directory, synced to disk, and renamed into
  place. `Got` is sent after that and not before (brief §51.4).
- **What happened**, in the activity record (0031): arrived, from whom, and
  where it went. That is what "Received from Phone A — Saved to Downloads/qurb —
  Open folder" (§12) is drawn from.

For the sender nothing changes. The desktop's copy is still recorded as private,
a copy the sender cannot ask back. Under
[0036](0036-a-phone-keeps-its-own-files.md), if the desktop also holds the
sender's vault, that is a separate copy with its own record.

## The two directories must never overlap

If the downloads directory were inside the synced folder, or the other way
round, a received file would be scanned into the shared area and advertised to
every device. That is the leak 0030 exists to prevent. An existing install whose
synced folder *is* `~/Downloads/qurb` is already in that position.

So qurb checks at startup, and whenever either setting changes, and refuses an
overlap rather than resolving it quietly. Where the synced folder already sits
at the default downloads path, the downloads directory defaults to
`qurb-received` inside Downloads instead.

## Names

A name that is already taken gets 0030's treatment:
`report.from-<device>-<when>.pdf`. It says who sent it, which a bare `(1)` does
not.

## What this costs

- **A received file on a desktop is no longer in qurb.** qurb does not back it
  up, the storage cap does not free it, and qurb's Files screen does not list
  it. Activity still says it arrived and where it went.
- **Sending it on is an ordinary send** of a file picked from disk, like any
  other.

## Reversing it

Cheap. Files already written to Downloads stay there; turning the setting off
goes back to filing deliveries into the folder.

## As built

**2026-09-25.** A `downloads` setting in the folder's config: empty for the
default, a path, or `off`. `qurb run` resolves it before anything syncs and
hands it to the engine; `qurb config` and `qurb run` both refuse a directory
that overlaps the folder, compared after resolving symbolic links. `qurb
status` says where received files go. New folders default to `~/qurb`; the
previous default, `Downloads/qurb`, is still found for existing installs.

Three things the decision did not spell out, found while building it:

- **"Taken" needed a record of its own.** It had been the received file's row,
  and a deleted file's row is a tombstone that garbage collection expires after
  seven days on a desktop. A file sent to one, filed in the folder and deleted,
  would have come back once its tombstone expired; saved to Downloads, with no
  row at all, it would have come back on the next sync. A
  `taken` table (index schema V11), never expired, is now the record for both,
  and the migration fills it from everything an existing device had received.
- **A crash between the rename and the record** leaves the file in place and
  unrecorded. The next attempt finds a file of that name with exactly the
  delivered bytes, and records it rather than saving a second copy.
- **Paths from the other device were never checked.** Writing a delivery
  outside the folder is what this decision does, so it forced the question of
  where else a peer's path could reach. The answer was anywhere: see
  [phase 4](../phases/phase-4-product.md), "A path is an instruction".

**On upgrade, an existing desktop starts saving received files to
`Downloads/qurb`**, because its config has no `downloads` line and empty means
the default. That is this decision applied, and it is stated here so it is not
a surprise; `downloads = off` restores the old behaviour.

**Verified** on the development laptop (CachyOS), 2026-09-25: five engine tests
in `crates/engine/tests/downloads.rs`, and two daemons on one machine sharing a
throwaway identity, paired through `qurb pair` and `qurb join`. A 3 MB file sent
with `qurb send` arrived in the receiver's downloads directory with an identical
SHA-256, not in its folder; after it was deleted there and the receiver
restarted and reconnected, it did not come back.

**Not yet:**

- **From a phone.** A phone cannot send yet — its interface has no send — so the
  pair this decision is really about, phone to desktop, is unexercised.
- **"Open folder" in the notification.** The notification says where the
  file was saved; the button is in the window instead — *Show in folder* on the
  Transfers screen, and *Open that folder* in Settings (2026-09-25). A
  notification action needs the notifier to wait on the notification, which the
  desktop's notifier does not do yet.

The setting is in the window's Settings since 2026-09-25, refused there if it
overlaps the folder, and a running daemon picks up a change on its next pass
rather than at restart.
