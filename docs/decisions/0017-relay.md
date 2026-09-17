# 0017 — The relay carries datagrams, not messages

**Status:** Accepted
**Date:** 2026-09-16

## Decision

When two devices cannot reach each other directly, they connect through a relay
that forwards **opaque datagrams**. An ordinary QUIC session — the same pinned
certificates, the same handshake, the same encryption — runs inside.

The relay is reached over TCP, and belongs on port 443 behind TLS.

## Why datagrams rather than messages

The obvious design is to relay application messages: the relay understands
"fetch this chunk" and passes it along. It is also the wrong one.

A relay that handles application messages is a relay that can **read and alter
them**. Preventing that means encrypting and authenticating at the application
layer — which is a second cryptographic protocol, built to solve a problem the
first one already solved, in a codebase that would then have two.

Forwarding datagrams means the existing QUIC session simply runs through the
relay. Nothing above the transport knows the relay is there. The relay sees
ciphertext addressed to an identifier it cannot link to a person, and its only
powers are to pass bytes on or drop them. Dropping is a denial of service, which
is true of every router between any two computers.

The cost is a `AsyncUdpSocket` implementation that makes a relay connection look
like a UDP socket. That is a contained piece of work, paid once, against a second
protocol maintained forever.

## Why TCP, when everything else is UDP

The data plane is QUIC over UDP because it is faster and avoids head-of-line
blocking.

The relay is TCP because **it exists for networks where UDP does not work**. A
fallback that requires the thing being fallen back from is not a fallback. The
same reasoning puts it on port 443: the networks that block UDP are usually the
ones that allow very little else.

## Who may use it

A device registers under the identifier the rendezvous service already uses —
derived from the user's master key, so the relay learns nothing about whose
traffic it carries.

Registration is self-asserted, and that is a deliberate judgement rather than an
oversight. The identifier is not guessable: it is derived from a key only that
user's devices hold. Claiming someone else's therefore requires already knowing
it, which means being inside a group whose members have paired with each other.

**The residual risk, stated plainly:** anyone who learns a member identifier can
register as that member and receive its relayed packets. They cannot read them —
the QUIC session inside is pinned to certificates they do not have — but they can
prevent the real device receiving them. A denial of service against one device,
by someone who already had to be close enough to learn a derived secret.

Closing it properly means the relay verifying possession of a device key, which
means the relay knowing device identities, which is exactly what the design
avoids. Tickets minted by the signalling service are the likely answer if this
becomes worth fixing.

A connection that has not registered cannot forward anything, so the relay is
not an open proxy for whoever finds the port.

## Consequences

**This is the component that costs money per byte**, permanently. The
direct-connection rate is therefore a business metric, and the reason the Phase 3
kill criterion is set where it is.

**Latency is worse on the relayed path**, obviously: every packet makes two trips
instead of one. Nagle's algorithm is disabled because relayed traffic is many
small packets where latency is already the problem.

**An undelivered frame is dropped silently**, as a router would. QUIC already
copes with losing datagrams, and inventing delivery guarantees underneath it
would be both redundant and wrong.

## Not decided here

- **Relay selection.** Choosing a nearby relay, and moving between them.
- **Fairness and quotas.** Nothing stops one device using all the capacity.
- **Automatic fallback.** Reaching a peer reports failure; nothing yet turns that
  into a relay attempt.

## Reversibility

**Low cost.** The relay is a separate service and a socket implementation.
Nothing above the transport refers to it.
