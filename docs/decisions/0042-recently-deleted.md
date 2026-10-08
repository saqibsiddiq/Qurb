# 0042 — Recently deleted

**Status:** Accepted
**Date:** 2026-09-28

## Decision

Every device with a folder keeps **Recently deleted**: files it took out of the
folder because they were deleted, moved to `trash/` in the store directory and
listed in a `trash` table in the index, instead of unlinked.

- **What goes there.** A file removed from the folder because another device
  deleted it; a file deleted through qurb itself (the phone's Vault); the
  version not kept when a conflict is settled ([0043](0043-settling-a-conflict.md)).
- **What does not.** A file somebody deletes with their own file manager is
  already gone before qurb sees it — their desktop's own trash has it. The
  *other* devices move their copies into Recently deleted as the deletion
  reaches them, so the file is still recoverable from any of those.
- **Restoring** puts the file back in the folder as a change made on that
  device: a new version, which reaches every other device the way any change
  does. The deletion is undone everywhere, including on the device that made
  it and had nothing left to restore from. A restore never overwrites: if
  something is at the old path now, it comes back as `name (restored).ext`.
- **Thirty days**, then it goes for good — in the housekeeping every device
  already runs (the daemon every few minutes, a phone after each sync). "Delete
  now" does it sooner.
- **Under a storage limit** Recently deleted goes after content held only on
  another device's behalf (which that device already has) and **before** any
  local copy of a live file. It counts towards what the device is using.

On the desktop it is at the bottom of the Files screen, on the phone under
Settings → Space, and on the command line `qurb deleted` / `qurb restore`.

## Why

Single-copy storage ([0024](0024-the-file-is-the-payload-store.md)) makes the
file in the folder the only copy of its bytes on a device. So before this, a
deletion on one device removed the file from every device it synced to, and
with it every copy anywhere: the retention window the index keeps for a
tombstone could restore the *record*, not the bytes. "Recover a deleted file"
(product plan §2.9) had nothing to recover from.

Moving instead of unlinking costs nothing at the moment of deletion — a rename
within one filesystem — and a phone whose store directory is on a different
filesystem from its folder copies and then deletes.

**Per device, not shared.** Each device's list is what *it* holds. A shared
trash would have to sync, and a synced trash is a second copy of everything
anyone deleted, on every device, which is the opposite of what deleting is for.

**Restore as a new version, not by un-tombstoning.** The index already had
`restore_file`, which revives the tombstone. That restores the row and not the
bytes, and only on the one device. Writing the file back and storing it as a
local change is what makes it return everywhere, by the ordinary rules.

## Privacy

A deleted file lingers for up to thirty days on every device that held it. That
is what makes it recoverable, and it is the wrong thing for a file somebody
deleted *because* it should not exist. "Delete now" is on each device, and the
Recently deleted list is in the open rather than hidden, so nobody is surprised
to find a file there. A deletion that reaches every device at once and removes
every copy is not built.

## Found on the way

A device that deleted a file could not take the **same bytes** again, under any
name. Its tombstone still answered "the content is here" from
`any_file_with_content`, but with an empty chunk list — the references to
payloads that left with the file had been released — and an empty list
assembles to the wrong bytes. The file failed as corrupt on every sync. Any
re-added photo or restored copy would have hit it. `any_file_with_content` now
only answers from a row whose chunks add up to its size; the regression test is
`content_deleted_here_can_arrive_again`.

## Not a person's files

**2026-10-08.** A sharing rule (decision 0044) is a file in the shared area, so
a rule deleted on another device arrived like any deletion and was kept here.
Recently deleted then listed `.qurb-sharing/<folder>` as a deleted file, and
restoring it would have put back a rule somebody had removed — on the device
it had let back into the folder, leaving that device out again. A rule deleted
elsewhere is now removed, not kept. One kept by an earlier build is not
listed, cannot be restored, and still expires.

## Not done

- A deletion made with a file manager is not in *that* device's Recently deleted
  (see above).
- A replica has no folder, so no Recently deleted; its tombstone retention is
  what it has.
- Restoring a file that was somebody's delivery does not make it a delivery
  again. (Restoring puts a file back in the area it was deleted from, shared or
  Private Vault, since 2026-09-28; before that, a phone filed it wherever a new
  file there went.)
