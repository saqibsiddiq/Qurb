# 0014 — Pairing transfers a full fingerprint out of band

**Status:** Accepted
**Date:** 2026-09-16
**Completes:** [0011](0011-peer-identity-pinning.md), which established pinned
identity and recorded pairing as the missing half.

## Decision

Pairing is an invite carrying the inviting device's **complete fingerprint**,
its address, a one-time token and an expiry, encoded for a QR code or for
someone to read aloud. The joining device connects with that fingerprint pinned,
presents the token, and the two exchange device identities.

Both sides record the other in a trust store binding **device id to
fingerprint**.

## Where the security comes from

**The out-of-band channel.** An attacker sitting in the network path can do a
great deal, but cannot change what is printed on a screen or said over a phone.
Carrying the whole fingerprint across that channel is what authenticates the
inviter — the same pinning every other connection uses, with the pin finally
coming from somewhere trustworthy.

The fingerprint is carried in full rather than abbreviated. A short code would
need a password-authenticated key exchange to be safe, which is more machinery
and more to get wrong. A QR code has room for 32 bytes.

**TLS authenticates the joiner.** It must present a certificate and sign the
handshake, so the fingerprint recorded for it is proven rather than claimed.
Neither side takes the other's word for anything that matters: the device id and
name are claims, and only the device id is load-bearing — bound, at that moment,
to a fingerprint that was proven.

## What the token is and is not

It is tempting to read the token as the secret that makes pairing safe. It is
not, and being clear about that matters for anyone extending this.

The token **authenticates nobody**. It lets the inviter distinguish a device
that saw the invite from one that merely found the port open, and it makes an
invite single-use. If it leaked, an attacker still could not impersonate the
inviter, because they cannot produce its certificate.

It is compared in constant time regardless, since a token compared byte by byte
can be guessed a byte at a time by anyone who can measure the reply.

## The listener accepts any certificate

Every other listener refuses an unrecognised peer. A pairing listener cannot:
the device joining is by definition not yet known.

What keeps that from being a hole is what it *does*. It answers nothing but a
pairing request, only one carrying the token, and it stops as soon as one device
succeeds. The certificate is still required and the signature still verified.

## What this closes

Decision 0011 recorded that `DeviceId` in the index and `Fingerprint` on the
network were separate identifiers, and that "nothing yet proves the device whose
version vectors you are merging is the device you authenticated." The trust store
is that proof: a row says the device whose certificate hashes to this
fingerprint is the one whose changes count under this device id.

`PeerServer::bind_trusting` now takes its guest list from the trust store rather
than from a caller, so the system finally has a notion of "my devices".

## Consequences

**Invites are short-lived** — five minutes. An invite left lying around is a
port that accepts strangers. This is a usability cost paid deliberately.

**Re-pairing replaces rather than accumulates.** A device that gets a new
certificate keeps its device id and its history; its old fingerprint stops being
trusted. Accumulating identities would mean a compromised certificate stayed
valid forever.

**The guest list is read once, at bind.** A device paired afterwards is not
accepted until the listener is rebuilt. Acceptable while pairing is a deliberate
act a person performs; it needs revisiting when devices come and go on their own.

## Not decided here

**Transitive trust.** Pairing A to B and B to C does not pair A to C. The
architecture describes an append-only device trust graph where an approval is
signed and distributed, which would make it transitive. That is a larger design
with real failure modes — revocation, conflicting approvals — and pairing each
pair of devices works today.

**Revocation.** `forget_peer` removes trust locally. Telling other devices that
a device is no longer trusted needs the trust graph above.

## Reversibility

**Moderate.** The invite format is versioned by its `qurb1-` prefix, so a
future format can coexist. The trust store schema is additive. Changing what
authenticates a pairing would be a protocol break.
