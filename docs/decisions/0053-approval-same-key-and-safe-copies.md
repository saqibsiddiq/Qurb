# 0053 — Pairing is approved, checks the key, and a phone's copy is not a safe last one

**Status:** Accepted — built and tested on the command line, the desktop and
Android; watched on the S23 with the laptop's command line on 2026-10-07,
not in the desktop window (see *Checked, and not*); amends
[0014](0014-pairing.md), [0025](0025-a-storage-cap-that-cannot-lose-data.md),
[0036](0036-a-phone-keeps-its-own-files.md) and
[0052](0052-the-key-travels-with-the-code.md)
**Date:** 2026-10-07

## What happened

[0052](0052-the-key-travels-with-the-code.md) did away with the 24 words, and
left four drawbacks, which the owner asked to have dealt with "without
sacrificing security or convenience":

1. A pairing code, seen or overheard while it lived, let whoever used it first
   take the key.
2. Two devices with different keys could still pair.
3. On 2026-10-05 the S23's data was cleared from Android's Settings. With it
   went 18 files the laptop had freed because the phone kept them, and a
   Private Vault no device kept.
4. A computer has no backup of its key of its own.

He chose, from options put to him: approval with matching numbers; all three
protections for files that exist only on a phone; and, for a computer's key,
reliance on the phone, with Settings saying where the key is safe.

## Decision

**A device is let in only when the person at the device showing the code
approves it.** Both screens show a six-digit number, derived with BLAKE3 from
three things: the fingerprint of the device showing the code, the fingerprint
of the device asking, and the code's token (`pairing_number`). The person
compares the two and taps *Approve*. A device that took the code from someone
else's screen holds a different certificate, so it shows a different number.
Declining leaves the code open for the right device. The device asking knows
its number before it connects (`Invite::number_for`). The device showing the
code learns the asker's fingerprint from the TLS handshake. Neither has to
trust the other's word for it.

**Pairing checks that both devices hold the same key** by comparing a value
derived from it (`Purpose::PairingCheck`), which is one-way, so the key never
crosses. A mismatch is refused with its own answer: *these two devices have
different keys*. This also means a code alone no longer pairs a device that
has a different key.

**Devices say what they are**: `phone`, `computer` or `replica`. They say it
when pairing, and devices paired before this ask once, with a new `About`
request, at their next sync.

**A phone's copy does not count as the other copy.** Freeing a file and the
storage cap's eviction both require that another device holds it, and now
that device must not be a phone (`safe_copies_elsewhere`). One tap in a
phone's settings can erase its copy. A device that has not said what it is
still counts, until it does.

**A phone's Private Vault is kept by the first computer it pairs with**, by
default and once only, when it has no keeper (`Store::learn_kind`). A person
who removes that choice is not overruled by the next computer.

**Android's *Clear data* opens qurb's own screen** (`manageSpaceActivity`).
That screen lists what exists only on the phone, offers to free space that
costs nothing, and still deletes everything if asked, after saying what it
costs. Uninstalling asks whether to keep the data (`hasFragileUserData`).

**Settings says where the key is safe.** On a phone: in its Google backup,
end-to-end encrypted, or on the phone only when it has no screen lock. On a
computer: which paired devices hold the key, and a warning when none do,
since a computer has no backup of its own.

**A device paired from outside the daemon is accepted on its first
connection.** On 2026-10-05 the first connection after `qurb pair` came four
seconds before the next sweep and was refused. The trust list now re-reads the
store when an unknown device connects, at most once a second.

## Why this, and not something else

- **No tap, with the code cancelled if a second device tries it.** That only
  catches an attacker who acts while the person is also joining. One tap,
  already familiar from Bluetooth, closes the gap entirely.
- **Treating every device whose kind is unknown as a phone.** Safer in
  principle, but it would have stopped every device paired before today from
  freeing anything until it re-paired. The `About` request settles the
  question at the next sync instead.
- **A recovery file for computers.** Not chosen. The phone carries the key,
  and Settings warns a computer with no other device holding it.

## What it costs

- **One tap** on the device showing the code, every time a device pairs.
- **Disk on the computer.** It keeps a copy of anything only a phone also
  has, and the phone's vault by default.
- **The wire changed**: `Pair`, `Paired` and `Join` carry more than before.
  A device and a peer on either side of this change cannot pair until both
  are updated. Syncing between them is unaffected, apart from an older peer
  not answering `About`.
- **The Clear-data screen replaces the system button**. Clearing qurb's data
  now takes two taps more, which is the point.

## Checked, and not

- `crates/peer/tests/pairing.rs`: `both_screens_show_the_same_number`,
  `a_declined_device_is_not_trusted`,
  `a_declined_device_gets_no_key_and_sets_nothing_up`,
  `devices_with_different_keys_do_not_pair`,
  `each_side_learns_what_kind_of_device_the_other_is`, and the earlier
  pairing tests with approval.
- `crates/storage/tests/storage_cap.rs`:
  `a_copy_only_a_phone_holds_does_not_let_the_bytes_go`,
  `a_phone_has_its_first_computer_keep_its_vault_once`,
  `a_computer_keeps_no_default_keeper`, `only_here_lists_what_a_wipe_would_lose`.
- `crates/peer/src/tls.rs`:
  `an_unknown_device_makes_the_list_reread_the_store_once_a_second`.
- `crates/qurb/tests/arguments.rs`: `qurb pair` asks, and a `y` approves.
- **Watched on the S23 on 2026-10-07**, with the laptop on the command line,
  both already holding the same key:
  - The phone joining the laptop's `qurb pair`. The phone showed 232 760, and
    the laptop asked about "SM-S911B (…, a phone)" with the same number. A
    `y` paired them.
  - The laptop's `qurb join` with the phone's code. The laptop printed
    996 097, and the phone's dialog showed 996 097 with *Decline* and
    *Approve*. *Approve* paired them.
  - Declining on the phone. The laptop got "the other device said no", and
    the phone's code stayed open with its time running.
  - Kinds in both databases: each recorded the other as `phone` or
    `computer`, and itself likewise. The phone had the laptop keep its vault
    and marked that default as made.
  - Android's Settings → qurb → Storage → *Clear data* opened
    `ManageSpaceActivity`. It listed one file, 1.6 GB, as existing only on
    the phone. *Free space safely* said "Nothing to free" and removed
    nothing. *Delete everything* was not tapped. The file was a send the
    laptop had already taken from the phone before its data was cleared,
    sent again afterwards. The laptop skipped it and never said so, a fault
    in [0030](0030-sending-a-file-to-one-device.md)'s rule 2, fixed the same
    day. See [phase 5](../phases/phase-5-mobile.md#a-send-taken-before-and-sent-again).
  - The key row in the phone's Settings said the key is on the phone and in
    its Google backup, end-to-end encrypted.
- **Not watched**: the desktop window's approval panel and its key row; two
  devices with different keys refused on hardware, where only the tests
  check it; a device being given the key while joining (0052's half);
  uninstalling with the offer to keep the data.

## Reversing it

Approval is one closure passed to `PairingHost::wait`; a closure that always
says yes restores the old behaviour, at the old risk. The safe-copy rule is
`SAFE_ELSEWHERE` in `crates/storage/src/db.rs`. The manifest's two attributes
undo the Clear-data guard. Kinds and key checks cost nothing to leave in place.
