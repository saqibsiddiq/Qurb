# 0041 — Removing a device

**Status:** Accepted — files kept only by a removed device read as *on no
device* since [0055](0055-a-file-on-no-device-says-so.md)
**Date:** 2026-09-28

## Decision

Removing a device ends this device's trust in it, here, now, and does nothing
it cannot actually do. From the Devices screen on either platform, or
`qurb remove-device <device>`:

1. **It can no longer connect here or sync with this device.** The trust row
   goes, the daemon closes any connection it holds to it, and a connection the
   removed device opened is refused at its next request — trust is asked per
   request, not only at the handshake.
2. **Nothing on the removed device is touched.** It keeps the master key, its
   files, and whatever this device had given it. The question says so, in
   those words (brief §35).
3. **Sends it has not collected are cancelled**, because nothing can collect
   them now.
4. **What this device keeps for it stays, unless the person ticks the box to
   delete it.** Those may be that device's only backup of its own files;
   deleting them is a choice, not a side effect.
5. **Copies it was known to hold stop counting as copies.** The `replicas` rows
   for it are marked `private` — "it has these bytes, and this device cannot ask
   for them" — which is exactly true once it is removed. Freeing a local copy
   is refused on the strength of it from then on. Paired again, it is asked
   about each such copy at its syncs, and what it still holds counts again
   (since 2026-10-08, [0055](0055-a-file-on-no-device-says-so.md)).
6. **Files already freed on the strength of it are named first**, with the
   offer to fetch them before removing it. Afterwards they have nowhere to come
   back from, and since [0055](0055-a-file-on-no-device-says-so.md) each says
   so: *On no device*.
7. **It is removed from this device only.** The person's other devices go on
   trusting it until it is removed on each of them. The question says that
   too.
8. **History keeps its name.** The removal is written to the activity log with
   the device's name, and history about a removed device shows that name rather
   than a short id.

## Why each

**Not the key.** Every device of one person holds the same master key
([0012](0012-key-hierarchy-and-recovery.md),
[0023](0023-one-person-per-account.md)). Taking it back needs key rotation, which
is designed and not built (product plan §2.4). Until then "removed" must not be
worded as "revoked", and the plan's §2.6 wording holds: the device can no longer
connect to your devices; it keeps what it has.

**The replica rows.** This is the part that loses data if got wrong. Freeing
space asks "does another device hold these bytes?" and answers from
`replicas`. Left as they were, a removed device's rows would go on answering
yes, and a file could be freed whose only other copy is on a device nothing
will ever connect to again. Deleting the rows instead would be wrong in the
other direction: the same rows are what say a send was collected, and a
collected send would reappear as waiting. Marking them private keeps both
answers right.

**Per-request trust.** Before this, a device that authenticated but was no
longer in the trust store was served the shared area ("Unplaced"): the leniency
existed for devices with incomplete bookkeeping. Combined with removal, it meant
a removed device holding a connection open went on being served. The listener
now asks the live trust list before each request, and closes the connection
once the answer is no. A long wait for changes already in progress finishes
first — within its ninety seconds.

**Only here.** Propagating a removal to the person's other devices needs a
record every device accepts as the owner's word, synced like a file but
authoritative like pairing. That is a protocol change, not a screen, and is not
built. Saying "only on this computer" is honest; a removal that silently held
on one device and not another would not be.

## Found on the way

Two faults in the daemon, both of which would have made removal a statement
rather than an action:

- `refresh_trust` updated the daemon's own list of peers only when a device was
  **added**. A removed device stayed in it — still dialled on every sweep and
  synced with, however firmly the listener refused its own connections.
- Dropping a connection from the peer table did not close it: the peer's change
  watcher holds its own reference. Removal now closes it explicitly, with a
  method that closes the connection alone — `PeerClient::close` also closes the
  endpoint, which in the daemon is the one every other connection shares.

Both have tests: `a_removed_device_is_disconnected` in the daemon, and
`a_device_removed_while_connected_is_no_longer_served` in `qurb-peer`.

## Not done

- Removal on one device does not reach the others (above).
- The key is not rotated; the removed device can still decrypt anything it
  already holds, and anything it is later given by a device that still trusts
  it.
- A removed device is not told. It keeps trying to connect and is refused.
