# 0074 — A device about to join is told who shows the code

**Status:** Accepted. Built in `crates/peer`, the phone's engine, the command
line and the Android app's setup; tested in the peer crate and the phone's
engine; checked on an emulator against `qurb pair`, and against a build from
before it. Used by Android's setup and, since the same day, by a phone
already set up joining from Devices; not yet by the desktop window. Amends [0053](0053-approval-same-key-and-safe-copies.md).
**Date:** 2026-10-24

## What was asked

The redesign's *Waiting for saqib* page
([design/redesign.md](../design/redesign.md), Pairing draft 1) says *Approve
on saqib* while the device showing the code decides. Built first as *Approve
on your other device*, because the joining device learned the other's name only
once the join was done: the code carries a fingerprint, an address and a token,
not a name. Asked whether the name should be shown, the owner, 2026-10-24: "do
the recommended options and continue".

## Decision

1. **A new request, `Who { token }`, answered with `Who { name, kind }`** by
   the device showing the code, to a device presenting that code's token,
   before anybody approves anything. A wrong token is answered *not found*.
2. **Asked on a connection of its own, before the join**, and that connection
   is let go once answered. The join then runs on another connection exactly
   as before.
3. **Never an error.** `qurb_peer::who` gives `None` when the other device is
   too old to know the question, when it does not answer within three seconds,
   or when the code has expired. The page then says *your other device*, as
   before, and the join that follows reports whatever went wrong.
4. **The protocol stays `qurb/2`.** A device from before this answers an
   unknown request *not found* and drops only that connection; its pairing
   listener goes on waiting for the next one, so the join after it works. The
   same reasoning kept guest invites on `qurb/2`
   ([0060](0060-a-computer-keeps-private-folders-for-several-people.md)).
5. **The name is a claim**, as the joining device's own name is to the device
   approving it. What is checked is the number both screens show (0053).

`pairing_number` in the phone's engine became `pairing_with`, which returns the
number together with the name and kind. The command line's `qurb join` says
*Approve it on saqib* when it is told.

## Why

The person holding both devices has to find the right screen to approve on.
"Your other device" is accurate and vague when there are two computers, or a
computer and another phone. The name is what the device showing the code
already calls itself on every other screen.

## What was rejected

- **The name in the code.** It would lengthen every QR code and read-aloud code
  for something the network can say in one round trip. It would also need a
  new code format (`qurb1-` is fixed).
- **Asking on the join's own connection.** One round trip fewer, but an older
  device drops the connection on an unknown request, so the joining device
  would have to tell old from new and reconnect: the same cost as asking
  separately, with more ways to get it wrong.
- **Sending the name with the key, after approval.** The page that needs it is
  the one shown before approval.

## Tradeoff accepted

- **Somebody holding the code learns the device's name without approval.**
  The code already gives the address and the fingerprint, and its holder is
  asked about by name on the other screen. The name is no more secret than
  that.
- **A few seconds more before the number shows, at most**, for a device that
  is not there. The join after it fails the same way it did.

## How it was checked, and what was not

- `crates/peer/tests/pairing.rs`:
  `a_device_about_to_join_is_told_who_shows_the_code_and_joins` and
  `who_shows_a_code_is_not_said_without_it`; `Who` added to the wire
  round-trip tests.
- `crates/mobile-ffi/tests/syncing.rs`:
  `a_new_phone_joins_with_the_code_and_needs_no_words` asks `pairing_with`
  first and is told the desktop's name.
- **On an emulator** (`qurb-34g`: Android 34 with Google APIs, 1080 by 2340),
  2026-10-24, against a throwaway `qurb pair` in a scratch folder renamed
  *Scratch-Laptop*. The long code was handed to setup by a temporary,
  uncommitted patch, since the emulator's camera cannot load the barcode model
  and the six digits cannot be heard behind its NAT. The page said *Approve on
  Scratch-Laptop* and *Scratch-Laptop shows this number too*, round a laptop;
  the terminal asked about the same number, 482 126; approved, the phone
  joined and opened on Home.
- **Against a build from before this**, the same day: the command line built
  from `e2484fa`, which has no `Who`, ran `qurb pair`; this one's `qurb join`
  said *your other device*, the old one went on waiting, asked, was approved
  and paired. 0.66 s for the whole join, approval included, both on this
  laptop: an older device's *not found* costs no wait.
- **Not checked:** on the S23; an older Android app or desktop window showing
  the code, which run the same pairing loop as the command line; and the page
  for a device that does not answer at all, which waits the three seconds and
  says *your other device* -- from reading `who`, and the wrong-token test.

## How hard to reverse

Easy. Removing the request leaves every device working as before; a device
that still asks would be told *not found* and say *your other device*.
