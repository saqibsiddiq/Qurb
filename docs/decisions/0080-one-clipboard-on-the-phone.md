# 0080 — One clipboard on the phone, and a tap opens

**Status:** Accepted, 2026-11-01. Built on Android the same day; not yet
used on a device (see *Not verified*).
**Date:** 2026-11-01

## What was asked

The owner's list of 2026-11-01 for the phone's Files, item 5: open, rename,
copy, paste and delete, as the website promises — a tap opens, a long press
gives every option — in *Shared* on a computer, in *Vault* and *Received*,
and in the phone's own backup folder on a computer
([phase 5](../phases/phase-5-mobile.md)).

## Decided

- **A tap opens; a long press gives the sheet**, for files everywhere
  (`Kit.bindFile`'s `onHold`). A Vault file with no bytes on the phone opens
  its sheet on a tap, since there is nothing to open and *Keep here* is
  there. Folders already worked this way: a tap opens, a hold gives the sheet.
- **One clipboard for all four places** (`Clipboard.kt`), one thing at a
  time, held for this run of the app. It replaces the Vault-only `VaultClip`
  of the owner's list of 2026-10-26, item 13.
- **Where a copy is made.** Within one computer's *Shared*, the computer
  copies it (`Request::SharedCopy`, tag 33), and nothing crosses the
  connection. Within the Vault, the phone copies it as before (`copy_kept`).
  Anything else goes through the phone: each file fetched from the computer
  into the app's cache, or read out of the Vault, then sent on or imported,
  then deleted from the cache. A folder is copied file by file. A name
  already taken is numbered, `beach (2).jpg`, the same way at both ends.
- **Pasting goes into the Vault and into *Shared*.** Pasting into the backup
  folder is allowed only in its *Private Vault*, which is the same Vault. The
  rest of the backup folder mirrors the phone's own folders, and something
  pasted there would not be the phone's backup of anything. **Nothing is
  pasted into Received**, which lists what arrived.
- **A backup can be copied, not renamed.** The next backup would bring it
  back under its original name, leaving two copies. Deleting one is still
  refused while its original is on the phone, as before.
- **A rename on the phone is recorded in its history** (`moved`, with the
  detail `renamed from <old path>`), so Received follows a renamed file to
  its new name. Before this, nothing recorded a rename, and a file renamed
  from Received would have dropped out of it.

## Costs and gaps

- Copying a backup into *Shared* on the **same** computer sends it to the
  phone and back. The computer could copy it itself, as it does within
  *Shared*; that request was not built.
- A paste that goes through the phone shows only "Pasting…" and then
  "Pasted", with no progress bar, and it holds the whole file in the cache
  until it has been sent or imported. A file larger than the phone's free
  space fails.
- An older computer answers `SharedCopy` with "that computer does not take
  changes from here", which is misleading. Updating the computer is the cure.
- Copying a Vault folder whose files are only on a computer fails at the
  first such file, with the files before it already pasted.

## Tried on the S23, 2026-11-01

A backup photo copied and pasted into the laptop's *Shared* arrived there.
A copy pasted within *Shared* was made by the laptop: `name (2).jpg`,
byte-for-byte identical to the original. A rename from the phone was
renamed on the laptop. A file from *Shared* pasted into the Vault arrived
there. A tap opened a Vault file in Android's *Open with*. A delete
confirmed and done. Backups kept from before the phone took its permanent
identity count as not the phone's own, so Copy is offered for any kept
file, not only for this phone's own backups. One tap on *Paste* once left
three copies, probably the owner tapping at the same moment; a paste now
ignores taps while one runs. **Not tried:** Received's rename and copy,
copying folders, pasting into the backup folder's *Private Vault*.

## Not verified (as written before the trial)

Only the computer's half is tested (`crates/peer/tests/shared_folder.rs`: a
file copied, numbered, a folder copied whole, a copy into itself refused).
The phone's half compiles, but no copy, paste or rename has yet been done
on the S23.
