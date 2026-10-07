# 0057 — Moving a file into or out of Private Vault

**Status:** Accepted — built on the desktop, Android and the command line;
see *Checked, and not*
**Date:** 2026-10-08

## What was asked

The brief (§2) designs *Move to Private Vault* and *Move to shared* on both
apps, with the move built after the design as "a small engine addition".
Until now a file stayed in whichever area it was made in. A phone that keeps
new files private put them in its vault, and nothing moved one back.

## Decision

**The same file, at the same path, in the other area** (`Store::move_area`).

**Into the vault.** The file is stored again as this device's own,
private, and the shared row is retired with a tombstone. Every other device
removes its copy at its next sync, as with any deletion. A device keeping
this one's vault ([0036](0036-a-phone-keeps-its-own-files.md)) then holds it
as part of that vault. It stays on disk where it was, and on this device
only its listing changes.

**Out of it.** The private row is retired the same way, which tells a device
keeping the vault to let it go. The file becomes an ordinary shared one, and
reaches every device at its next sync.

**Refused:**

- when the bytes are not on this device, since the move stores the file
  from the folder (*keep it here first*);
- when the path is live in both areas;
- for a file that is another device's, kept here for it.

**Asked before moving in, and told what it costs.** The window and the phone
say the other devices remove their copies, each keeping it in Recently
deleted for 30 days. They then say one of two things:

- which device keeps a backup of this one's vault;
- that none does, so this device will have the only copy.

Moving out needs no question: nothing is lost.

**Reached from a file's menu and details** on the desktop, its sheet on the
phone, and `qurb private <path>` / `qurb unprivate <path>`. The history
records it as *moved*.

## Why this, and not something else

- **Changing the row's area in place** (`set_scope`, which the tests use).
  Other devices would never hear of it. The shared copies would stay shared
  everywhere else, and the private file would still be offered to them.
  Retiring the old row is what tells them.
- **Removing other devices' copies without Recently deleted.** That is what
  "private" might suggest, but the wire cannot carry why a file was deleted,
  and a protocol change would make every device update together. A
  deletion is a deletion everywhere, and the question says where it goes.

## What it costs

- **A moved file lingers in other devices' Recently deleted** for 30 days,
  unless deleted for good there.
- **Moving a large file stores it again**, chunking the bytes on disk, since
  the vault's row is a new one. It costs a read of the file, not a transfer.
- **A file moved out arrives on other devices as new.** Anything that knew
  it before sees a fresh file, not the one it had.

## Checked, and not

- `crates/peer/tests/vaults.rs`:
  `a_file_moved_into_private_vault_leaves_the_other_device_and_returns_when_shared`,
  over a real connection: the other device's copy goes, the file is no
  longer offered, and moving it out brings it back. Also
  `moving_needs_the_bytes_here`.
- `crates/peer/tests/holding.rs`:
  `a_file_moved_into_the_vault_is_kept_by_the_device_holding_it`. The
  desktop keeping the phone's vault holds the moved file, as the phone's.
- The window's confirmation against its fixtures, in both themes.
- **Not yet watched**: either app moving a file on hardware.
