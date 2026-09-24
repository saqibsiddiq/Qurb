# 0034 — Finding each other with no server

**Status:** Accepted
**Date:** 2026-09-24

## Decision

Devices on the same network find each other by multicast beacon, with nothing
else in the picture. A rendezvous service is no longer required to start, to
run, or to sync — only to introduce devices that are on *different* networks.

Two changes, and they are one idea:

1. **Beacons.** Every device multicasts a small encrypted packet saying who it
   is and where it can be reached, and listens for others. That is the whole
   protocol.
2. **The rendezvous is best effort.** `Connector::start` no longer fails when
   it cannot reach one. It logs, carries on, and keeps trying in the
   background.

## Why

Two devices on one Wi-Fi have a path to each other and need nobody to introduce
them. Until this existed they needed one anyway, and the consequence was a
dependency the product should not have: to sync a laptop and a phone sitting on
the same desk, something on the internet had to be reachable and up.

It was also fragile in practice. The rendezvous here was reached through an
overlay network, and a phone spent three hours unable to sync because that
overlay had quietly dropped off — the hostname stopped resolving and every sync
failed with a DNS error. Nothing was wrong with either device or with the
network between them.

## What a stranger on the network sees

Random bytes.

The beacon is encrypted under a key derived from the master key
(`Purpose::LocalDiscovery`), nonce in the clear followed by ciphertext and
nothing else — no magic number, no version outside the encryption, no length
prefix. An eavesdropper on a café network cannot read one, cannot tell how many
devices are present, and cannot tell that qurb is running at all.

This is not decoration. The identifier a device announces under is a bearer
secret: anyone holding it can ask the rendezvous service where that device is.
Broadcasting it in the clear would hand it to every machine on every network a
device ever joins, which is strictly worse than the service it replaces.

Replay is bounded by a timestamp inside the encryption, two minutes either way.
A captured beacon replayed later costs a wasted connection attempt, which is why
the window is generous rather than tight.

## Beacons are answered, not just broadcast

A device announces on arrival and every twenty seconds after. That alone is not
enough, and the reason was found on hardware rather than by reasoning:

A phone runs discovery only for the length of a sync pass. It starts with an
empty address book, and the peer's next scheduled beacon is up to a full
interval away — so the pass finishes before it learns anything, every time.

So an arriving device sends a **probe**, and everyone who hears one answers
immediately. A reply is never itself a probe, so an arrival costs one round of
answers rather than a storm, and an address book is full within a moment of
starting instead of over the following twenty seconds.

That was still not enough, and the second half came from the same phone.
Probing fills the address book in a few hundred milliseconds, and a sync pass
asks for a peer in rather less than that — so it asked, found nothing, and gave
up before its own answers came back. `reach` now probes and waits up to a
second before falling through to the rendezvous. The wait is paid once per
connector, only when the answer is not already known.

## A sighting is better evidence than a rendezvous record

`reach` consults the network before the service. A beacon means "a device
holding our key sent this, from a network we are on, moments ago", which is
stronger than anything a directory can offer — a rendezvous record says only
where a device *claimed* to be when it last announced, possibly from a network
with no route to this one.

On failure it falls through and asks properly, because a sighting can be stale
by the time it is acted on.

## What this does not do

- **It does not replace the rendezvous service.** Two devices on different
  networks still need introducing, and nothing here helps. What is removed is
  the requirement for one when the devices can already see each other — which,
  for most people, is most of the time.
- **It does not work where multicast does not.** A network that blocks it, a
  guest network that isolates clients, a VPN that captures the default route:
  all of them mean no local discovery, and all of them are survivable because
  the service is still there.
- **IPv4 only.** An IPv6-only network gets no local discovery. Worth adding and
  not yet added.
- **IPv6 is still not covered**, as above. That is the only platform gap left:
  Android is verified in both directions with no server running at all, in
  sixty-eight milliseconds — see the phase document, including which of three
  suspected causes it actually was.
