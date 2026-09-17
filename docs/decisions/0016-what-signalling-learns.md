# 0016 — What the signalling server is allowed to learn

**Status:** Accepted
**Date:** 2026-09-16

## Decision

Devices announce to the rendezvous service under identifiers derived from their
master key:

```
group  = KDF(master, "rendezvous-group/v1")
member = KDF(master, "rendezvous-member/v1", fingerprint)
```

Every device of one user derives the same group. A device's member identifier
depends on its fingerprint, which pairing has already established, so a peer can
work out where to look while the server sees only opaque bytes.

The server matches identifiers and forwards addresses. It holds no key, learns
no filename, and never sees a chunk.

## The problem this addresses

A rendezvous service has an unavoidable weakness: it exists to introduce devices,
so it necessarily learns that some set of addresses belong together. Run
naively — devices registering under an account name, or under their real
fingerprints — it would also learn *whose* devices these are, and could build a
map of every user, their devices, and their locations over time. That is a more
valuable database than the files it was carefully prevented from seeing.

## What the derivation buys

The server cannot link a group to a person, an account, or an email address. It
cannot recognise the same device appearing under two different groups, because
the member identifier depends on the master key as well as the fingerprint.
Identifiers come from the existing key hierarchy, so there is nothing extra for a
user to back up or lose, and the derivation is one-way, so an identifier that
leaks reveals nothing about the key.

## What it does not buy, stated plainly

**The server still sees IP addresses**, and can correlate them over time. That
is inherent to being a rendezvous point.

**The group identifier is a bearer secret.** Anyone holding one can enumerate
that group's addresses. It is derived from the master key precisely so that only
the user's own devices have it — but it means the signalling channel must be
encrypted, and the client refuses a plain `ws://` URL to anywhere but the local
machine. Connections still require pinned certificates, so a leaked identifier
exposes addresses rather than data.

**Traffic analysis is not addressed.** Someone watching the server can see which
groups are active and when.

## Why the channel is websockets over TCP, not QUIC

The data plane is QUIC over UDP, and reusing it here would have been tidier.

It would also fail exactly when it is needed. If UDP is blocked — which is
precisely the situation in which a device most needs coordinating — a UDP
control channel cannot even tell that device that it needs a relay. The channel
that arranges the fallback has to work where the fallback is needed.

## Why the connection is held open

Hole punching needs both routers to see an outbound packet at roughly the same
time. A request-and-response API cannot arrange that: whichever device has to
poll to discover it should punch will always be late.

So the server tells both sides at the same moment, on connections it is already
holding. That is the feature the transport was chosen for, not an incidental
consequence of it.

## Consequences

The server must be deployed behind TLS. This is a deployment requirement rather
than an optional hardening, and the client enforces it by refusing plaintext to
anything but localhost.

Cleaning up when a device disconnects is load-bearing rather than tidiness: a
device left in the directory means peers being handed a dead address and
spending their punch attempts on it.

## Reversibility

**Low cost.** The derivation is versioned by its label. Changing it would make
devices on different versions unable to find each other until all were updated,
which for a rendezvous service is an outage rather than data loss.
