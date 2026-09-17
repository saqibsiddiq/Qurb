# 0022 — The rendezvous service announces arrivals

**Status:** Accepted
**Date:** 2026-09-18

## Decision

When a device announces itself to the rendezvous service, every other member of
its group is told, immediately, with the arriving device's addresses attached:

```rust
FromServer::Appeared { peer: Presence }
```

A peer receiving one acts on it at once — clearing any backoff it was holding
against that device, because the device is demonstrably there.

## The problem it solves

The service always knew the moment a device appeared. It told only that device.

Everyone else had to discover it by asking, and asking fails when the peer is
not currently announced. So a device that is awake only briefly — a phone in a
background sync window, announced for twenty-odd seconds — could only be found
by a peer that happened to be asking during those seconds.

Worse, the polling interval grows *because* the phone keeps being absent. The
daemon's retry backoff doubles on each failure to a two-minute cap, so the
longer a phone is away, the less likely anyone is to be looking when it returns.
The two drift apart rather than converging.

**Measured before this existed:** a laptop retrying every 120 seconds against a
phone announcing for 25 seconds never once caught it, across repeated attempts a
minute apart. The phone could pull from the laptop — the laptop is always on and
always announced — but nothing on the phone could ever reach the laptop, so sync
was one-directional in practice while appearing symmetrical in design.

**After:** the daemon logs `peer appeared; syncing now` within a second of the
phone announcing, and a 4.7 MB photo transferred inside the phone's window,
byte-identical by SHA-256.

## Why push rather than shorter polling

Polling faster would mean every device waking every few seconds to ask about
peers that are usually absent — which is exactly the battery cost mobile sync is
supposed to avoid, paid by the always-on device on behalf of the one that is
asleep.

It also would not close the gap, only narrow it. A phone awake for twenty
seconds and a laptop asking every five still miss each other one time in four,
and the failure is silent.

The service is already holding a connection open to every member — that is what
[decision 0017](0017-relay.md) and the punch coordination require. Sending one
message down a connection that already exists costs nothing and is exact.

## What the service learns, and does not

Nothing changes about [decision 0016](0016-what-signalling-learns.md).

The notice carries a `MemberId`, which is blinded: derived from the group's
master key and the device's fingerprint, so the service cannot link it to a
device, a person, or a group of devices belonging to one person. It already had
this identifier — it is what the directory is keyed by — and it is already
telling the *arriving* device about everyone else. This reverses the direction
of information the service was already disclosing to that group.

A recipient recovers the fingerprint by re-deriving the identifier for each peer
it already trusts and looking for a match. That is a handful of hashes against a
list of a person's own devices, not a search, and it needs the master key — so a
network observer holding a rendezvous identifier learns no more than before.

## Consequences

- **`Appeared` is unsolicited** and can land between any request and its reply.
  Every reader has to tolerate it. One existing test broke the day this landed —
  it waited for a punch instruction and got an arrival notice first — and the
  test helper now skips them, which is the right shape: tests about arrivals
  read the stream directly, and everything else ignores them.
- **A busy group is chattier.** Each announcement costs one message per other
  member. Group size is capped, and the messages are small, so this is bounded
  by the same limit that already bounds the directory.
- **The daemon clears a peer's backoff on arrival.** The backoff exists to stop
  hammering a device that is switched off; a device that has just announced
  itself is not switched off, so the reason has gone.
- **Nothing depends on delivery.** A notice that fails to send is dropped and
  the recipient falls back to its ordinary sweep — slower, but correct. Arrival
  notices are a nudge to try now, not a log to be replayed, which is why a late
  subscriber misses earlier ones by design.
