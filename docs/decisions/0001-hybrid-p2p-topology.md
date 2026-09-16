# 0001 — Hybrid P2P with a central coordination plane

**Status:** Accepted
**Date:** 2026-09-07

## Decision

File data moves directly between a user's own devices, encrypted. A small set of
cloud services handles identity, device discovery, NAT traversal coordination,
push notifications, and encrypted relay when a direct connection is impossible.
User file data is never stored on our servers.

## Reasoning

The two obvious topologies each fail on one of the project's two goals.

**Pure client-server** — storing files in S3 or Backblaze — is what everyone
else does, and it works. But it makes recurring storage cost scale with usage
forever, and it puts us in possession of user data, which contradicts the
premise of the product.

**Pure peer-to-peer**, in the manner of raw Syncthing or IPFS, keeps the privacy
property but is unusable for non-technical people. Devices find each other by
exchanging cryptographic keys or by local network broadcast, neither of which
survives cellular data, hotel wifi, or a strict corporate firewall without
manual configuration.

The hybrid keeps the data plane peer-to-peer and private while using a
coordination plane to make pairing feel like Dropbox: scan a code, done. The
servers know *that* two devices want to talk and roughly where they are, not
*what* they say.

## Tradeoff accepted

Availability now depends on our coordination servers for initial pairing and
reconnection, and on relay servers when hole punching fails. We are not
eliminating cloud dependency, we are shrinking it to metadata and to a fallback
path — and giving ourselves a bandwidth bill proportional to how often that
fallback is needed.

## Reversibility

**Medium.** The storage engine does not know how peers were found. Replacing the
discovery mechanism means replacing the signalling layer while leaving chunking,
storage, and synchronisation untouched.
