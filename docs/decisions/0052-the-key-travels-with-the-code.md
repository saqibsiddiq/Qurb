# 0052 — The key travels with the pairing code; nobody writes 24 words down

**Status:** Accepted — built on the command line, the desktop and Android, and
watched on the S23 joining the laptop with a code;
supersedes [0033](0033-the-phrase-on-a-screen.md)'s confirmation step and
[0012](0012-key-hierarchy-and-recovery.md)'s "the phrase is the only way to
recover"; changes what the token in [0014](0014-pairing.md) guards
**Date:** 2026-10-05

## What happened

The owner, on 2026-10-05: "i dont like the idea the writing down 24words, its
just too inconvient for the user."

Until now a new device made a key, showed its 24 words, and would not continue
until three of them were typed back. Every device after it had to have all 24
typed in. Pairing did not carry the key: it only introduced two devices that
already shared one ([0014](0014-pairing.md)). Decision 0012 called onboarding
"a product problem this decision creates and does not solve".

The same day showed the cost. The phone's data was cleared from Android's
Settings, and the phone could rejoin the laptop only by having someone type
24 words into it.

## Decision

Chosen by the owner from three questions put to him the same day:

1. **A device joins with a code, and the key comes with it.** A device with no
   key connects to the device showing the code, proves it saw the code, and
   is sent the key over that connection. It sets itself up with the key and
   then pairs as before, on the same connection. A phone scans the
   computer's QR code; a computer types the code a phone shows
   (*Show a code on this phone*). `qurb join [dir] <code>` does the same on a
   folder not set up yet.
2. **The first setup asks nothing.** *Set up Qurb here* makes the key and
   carries on. No words are shown and none are checked.
3. **A phone keeps its key in Block Store**, Google Play services' store for a
   few bytes per app. It survives a reinstall, and when the phone has a screen
   lock it is backed up end-to-end encrypted with that lock, so a phone
   restored from this one's backup gets the key back. Setup offers *Use the
   key from your Google backup* when one is there.

The 24 words still exist. They are the key's spelling, and the key's
derivations are unchanged. *Use my 24 words instead* stays as a way in for
somebody who has them, and the desktop's Settings can still show them. Nobody
is asked to write them down.

## How the key travels

The wire gains two messages ([`crates/peer/src/wire.rs`](../../crates/peer/src/wire.rs)):
`Request::Join { token }` from the device with no key, and `Response::Key { key }`
in reply. `PairingHost::wait_giving_key` answers a `Join` that carries the
invite's token by sending the 32-byte key, then waits on the same connection
for the ordinary `Pair`. So the certificate the key went to is the
certificate that becomes trusted. `qurb_peer::join` is the other side: it sends
`Join`, hands the key to a caller-supplied `set_up`, because how a key is kept
differs by platform, and then pairs.

Its security rests on the same two things pairing always rested on:

- **The joining device knows it is talking to the right device**, because the
  code carries that device's full fingerprint and the connection is pinned to
  it (0014). Nothing is set up unless the key arrives over that connection.
- **The device giving the key knows the other saw the code**, because it
  presents the invite's 16-byte token, compared in constant time.

## What changed about the token

0014 said the token "authenticates nobody" and that its leaking would not let
an attacker impersonate the inviter. Both are still true. But it now guards the
key. Anyone who has the code while it is live, and reaches the device showing
it first, can take the key. That is close to what the token already allowed:
a device that pairs with it is trusted, and can fetch every shared file. What
is new is that the key also lets it read encrypted copies the person keeps
elsewhere, such as a replica, and join their rendezvous group. So:

- The key is given **once per code**. Once it has gone to one device, no other
  connection presenting the same code gets it, even if the first never
  finishes pairing.
- Codes still expire in five minutes and still work once.
- The spoken form of a code is now as sensitive as the QR. Reading it aloud
  over a phone line remains possible, and is now handing over the key.

## Why this, and not something else

- **The words, made optional but still prompted.** The owner did not pick this.
  A reminder to write them down is the same chore deferred.
- **A recovery password, with a copy of the key on the owner's server.**
  Memorable, but a weak password can be guessed by whoever holds that copy,
  and the server does not exist yet. Open for later.
- **No recovery at all.** Simplest, and on its own not chosen.

## What it costs

- **Files are still only on the person's devices.** This was true before and
  is easy to misread now. A key backed up or carried by a code brings back the
  key, not the files. Lose every device holding a file and the file is gone,
  unless a replica keeps a copy. The 24 words never protected against that
  either. What they protected, being able to read a surviving encrypted copy,
  Block Store now does for a phone. A computer has nothing equivalent: Secret
  Service, the Linux keyring, does not leave the machine.
- **Block Store needs Google Play services and a screen lock** to leave the
  phone. Without a screen lock the copy stays on the phone, which survives a
  reinstall but not a lost phone.
- **The pairing code now carries more weight**, as above.
- **Two devices that already have different keys still pair** as before;
  nothing yet refuses that or says it is wrong.

## Checked, and not

- `crates/peer/tests/pairing.rs`:
  `a_device_with_no_key_joins_with_the_code_and_gets_the_key`,
  `without_the_code_a_device_gets_no_key_and_sets_nothing_up`,
  `the_key_goes_to_one_device_only`, `plain_pairing_never_hands_out_the_key`.
- `crates/mobile-ffi/tests/syncing.rs`:
  `a_new_phone_joins_with_the_code_and_needs_no_words`. The FFI's `join_new`
  ends with the desktop's key and both paired.
- `crates/qurb/tests/arguments.rs`:
  `a_new_folder_joins_with_a_code_and_takes_the_key`. The real binary: `qurb
  pair` on one, `qurb join <code>` on a fresh folder, same key after.
- Writing that last test found that a program exiting straight after pairing
  left the device showing the code waiting 30 seconds for a close that was
  never sent. The joining side now waits briefly for its close to leave.
- **On hardware, 2026-10-05**: the S23, left mid-setup with an unwanted key
  after its data was cleared, chose *I already use Qurb*, was given a code by
  `qurb pair ~/qurb` on the laptop, and joined. The laptop printed *Paired with
  SM-S911B (b632a5b9)*; the phone opened into the app with the laptop's key,
  its unused key put aside, and the laptop then reached it and synced. The
  laptop's daemon refused the phone's first connection, four seconds before it
  reloaded its trust list: pairing from a terminal does not nudge it, as the
  window's pairing does. The key went to Block Store with no error.
- **Not yet watched**: joining by scanning (the same `join_new`, behind the
  camera), the desktop joining with a code from the phone, and Block Store
  giving the key back.

## Reversing it

Hosts that call `wait` instead of `wait_giving_key` stop giving the key, and the
setup screens' word steps are in the history. Keys handed over by codes are
ordinary keys, so nothing needs migrating either way.
