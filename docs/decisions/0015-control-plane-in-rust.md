# 0015 — Signalling and the relay are written in Rust

**Status:** Accepted — revisit for accounts and billing
**Date:** 2026-09-16
**Revises:** the Go control plane described in
[architecture.md](../architecture.md) section 3, which was a plan rather than a
recorded decision.

## Decision

The signalling service and the relay are written in Rust. Whether accounts,
authentication and billing are also Rust is left open and revisited in Phase 6,
when that work starts and its libraries matter.

## Why this is being revisited at all

The architecture chose Go for the control plane on grounds of development
velocity and cheap concurrency. That was written before any of this existed. The
evidence has since changed: there are now six Rust crates, and a signalling
service needs to speak the same vocabulary as the client — endpoints, device
identity, the error types that describe why a connection failed.

The choice also never became a decision record, so it was a plan rather than
something settled.

## Reasoning

**Shared vocabulary, defined once.** Signalling exchanges identifiers derived
from the key hierarchy that already exists in `qurb-keys`. In Go those
derivations would be reimplemented and kept in step by hand, and a drift between
two implementations of a key derivation is a bug that presents as "this device
cannot find its peers" with nothing obviously wrong on either side.

**One toolchain.** For a solo developer this is not a small thing. One build,
one test command, one CI configuration, one set of lints, one dependency audit.
A second language is a second everything, paid for permanently.

**The relay is throughput work**, and it will share the transport concepts the
client already uses. Splitting it across a language boundary buys nothing.

## What Go is still better at, and where that matters

Go's libraries for the things Phase 6 needs — Stripe, WebAuthn — are more
mature and more widely used than Rust's. That is a real advantage, and it
applies to accounts and billing rather than to rendezvous and packet forwarding.

Services can be separate processes in separate languages. Nothing decided here
prevents an account service in Go later, and pretending the whole control plane
must be one language would be the actual mistake.

## Consequences

The `backend/` directory that once held an empty Go module is not coming back in
that form. It was removed during the merge as scaffolding that had never been
filled in.

Deployment gains a Rust service rather than a Go one: a larger binary and slower
builds, against no runtime to install and no second toolchain in CI.

## Reversibility

**Moderate.** The protocol is JSON over websockets, which any language speaks,
so a server could be rewritten without touching a client. What would be lost is
the shared derivation code — which is precisely the thing that motivated this.
