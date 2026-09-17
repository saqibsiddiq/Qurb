# 0011 — Peer identity is a pinned certificate fingerprint

**Status:** Accepted
**Date:** 2026-09-16
**Completed by:** [0014](0014-pairing.md), which builds the pairing this record
identified as missing and binds `DeviceId` to `Fingerprint`.

## Decision

Each device generates one self-signed certificate and keeps it. Its identity on
the network is the BLAKE3 hash of that certificate.

Both ends of a connection present a certificate, check the other's fingerprint
against a list supplied in advance, and verify the TLS 1.3 handshake signature.
A connection without a client certificate is refused.

`PeerClient::connect` requires the expected fingerprint. There is no way to
connect without one.

## Reasoning

**There is no certificate authority here, and there should not be.** The system
has no central party that vouches for devices, by design
([0001](0001-hybrid-p2p-topology.md)). A device's identity is its key, so the
only meaningful question at connection time is whether this is the key we were
told to expect.

**Hashing the whole certificate, not the key inside it.** Comparing public keys
means parsing X.509 to extract the SubjectPublicKeyInfo, which is a parser in
the trust path for no benefit. The certificate is self-signed, so it binds the
key it contains, and a device keeps exactly one — hashing the certificate is
equivalent and has no parser.

**The signature check is the part that matters.** Fingerprints are public;
anyone who has seen a certificate can present it. What proves the peer is
genuine is signing the handshake with the matching private key. Pinned TLS is
routinely got wrong by comparing the certificate and then waving the signature
through, and that failure mode is silent — everything works, including for an
attacker.

**Requiring the fingerprint is a design choice, not an inconvenience.** An API
where it could be omitted would make "trust whoever answers" the easy path, and
that is the entire attack. The type system refuses it instead.

## What is deliberately not decided here

**Pairing.** How a device learns which fingerprint to expect — the QR-code
exchange, out-of-band verification, and the device trust graph — is Phase 3
work and is not built. Until it is, the caller supplies fingerprints, which in
practice means tests and examples do.

Without pairing there is no protection against being told the wrong fingerprint
in the first place. The transport is sound; the thing that decides what to feed
it is missing.

## Relationship to the planned Noise handshake

The architecture specifies the Noise Protocol Framework with the `IK` pattern.
TLS 1.3 with pinned certificates provides the same properties that matter here —
mutual authentication against known static identities, and forward secrecy — and
arrives with QUIC rather than needing a second handshake layer inside it.

Whether to move to Noise later is open. The argument for it is initiator
identity hiding against a passive observer, which TLS 1.3 client certificates do
give (they are encrypted), so the practical gap is smaller than the architecture
document implies. Worth re-examining before pairing is built rather than
assumed either way.

## Consequences

`DeviceId` in the index and `Fingerprint` on the network are currently separate
identifiers. Binding them is pairing's job — a paired device records both. That
they are separate today is a gap, not a design: nothing yet proves the device
whose version vectors you are merging is the device you authenticated.

## Reversibility

**Low cost.** The transport is one module behind `PeerClient` and `PeerServer`,
and the protocol above it does not know how the connection was secured.
