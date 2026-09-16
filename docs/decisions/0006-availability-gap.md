# 0006 — Always-on node for offline availability

**Status:** Proposed — open question, not yet decided
**Date:** 2026-09-07

## The problem

In a pure peer-to-peer design, a file is reachable only while at least one
device holding it is online. If a user's laptop is closed and their desktop is
off, their phone cannot fetch a file that exists on neither.

This is the architecture working exactly as designed. It is also a direct
contradiction of what the product promises. The Dropbox mental model — "my files
are available everywhere, always" — is what makes the product comprehensible to
non-technical users, and this design breaks it in a way that will generate
support load indefinitely: *"I'm at work, where are my files?"*

## Why this is urgent rather than deferrable

Every plausible solution touches the storage engine, not merely the networking
layer. Whichever is chosen affects how chunks are replicated, how the engine
decides what to keep where, and what the metadata must track. Retrofitting any
of them after the engine is built is substantially more expensive than designing
for one now.

## Options

**A. User-provided always-on node.** The user runs the headless daemon on a NAS,
a Raspberry Pi, or a cheap VPS. Preserves the privacy story completely; we store
nothing. Only viable for users who own or will rent such hardware, which is a
minority.

**B. Optional paid encrypted pin.** We store encrypted chunks for users who opt
in and pay. Solves the problem for everyone, keeps zero-knowledge intact since we
hold only ciphertext, and creates a natural revenue line. But it reintroduces
recurring storage cost — the thing the architecture was designed to avoid — and
it makes us a storage provider for the subset who use it.

**C. Peer-assisted replication.** Chunks are replicated to other users' devices,
encrypted. No infrastructure cost, but it introduces a large set of hard
problems — incentives, abuse, capacity accounting, and the fact that users
generally dislike hosting strangers' data even encrypted.

**D. Accept the limitation.** Position the product honestly as device-to-device
sync rather than cloud storage. Narrows the market considerably but keeps the
design pure and the costs at zero.

## Current thinking

**A and B together** look strongest: A for the technical audience who will be
the earliest users, B as the answer for everyone else and a revenue line that
scales with value delivered rather than with all stored bytes. C is a research
project. D is a real option that should not be dismissed, but it is a different
and much smaller product.

## What must be decided before Phase 1 ends

Not necessarily which option ships, but whether the engine must support
replicating chunks to a node that is not the user's own device. If yes — and A,
B, and C all require it — then chunk placement and replication policy need to
exist in the data model from the beginning.
