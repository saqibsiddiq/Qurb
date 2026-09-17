# 0006 — Storage-only replicas, for offline availability

**Status:** Accepted
**Date:** 2026-09-16 (proposed 2026-09-07)

## The problem

In a pure peer-to-peer design a file is reachable only while a device holding it
is online. Laptop shut, desktop off, and the phone cannot fetch a file that
exists on neither.

This is the architecture working as designed, and a direct contradiction of what
the product promises. The Dropbox mental model — *my files are available
everywhere, always* — is what makes this comprehensible to anyone who is not
already thinking in terms of peers. *"I'm at work, where are my files?"* is a
support conversation without end.

## Decision

The engine supports **storage-only replicas**: a device that holds content
without a directory behind it, is always on, and originates nothing.

Where that replica runs is a **deployment** question, not an architecture one.
The user's own always-on machine ships first. Holding encrypted chunks for a fee
is the same software in a different place, and is deferred as a business
decision that needs no further engine work.

## The reasoning that settled it

The four options this record originally proposed looked like alternatives. They
are not. A user-provided node, a paid pin, and peer-assisted replication all need
the **identical engine capability** — replicating content to something that is
not a person's device. Only accepting the limitation avoids it.

So the architectural question was never *which option*. It was *do we want any of
them*, and the answer is obviously yes. Once the capability exists, choosing
between A and B is choosing where to run a container.

**Peer-assisted replication is rejected.** Incentives, abuse, capacity
accounting, and the fact that people dislike hosting strangers' data even
encrypted — each is a research project.

**Accepting the limitation is kept as the honest fallback**, not the plan. It is
a real product, just a much smaller one, and it should be chosen deliberately
rather than arrived at by never building the alternative.

## What it required of the engine

Two behaviours had to be switched off, and both would have been destructive
rather than merely wrong.

**Deletion must not be inferred from an empty directory.** A syncing device
decides a file is gone by walking the tree and not finding it. A replica has
nothing on disk by design, so the same inference would tombstone the entire
library and propagate those deletions to every device that trusted it. The most
dangerous thing in this decision is the one that looks like a no-op.

**Files must not be materialised.** Writing every file to disk as well as storing
its chunks costs roughly twice the space for a copy nobody reads, and makes a
filesystem the replica does not really have authoritative for what it holds.

Everything else already worked. A replica never edits, so it never advances its
own counter and can never win a conflict or push a change of its own. The trust
store already treats it as an ordinary paired device.

**Partial replication is built in from the start**, not deferred. "Hold
everything" is the expensive answer; the useful one is usually "hold what I
actually reach for". Retrofitting a selector later would have meant revisiting
every path that assumes a replica mirrors its peers.

## Why this is not a reversal of 0001

[Decision 0001](0001-hybrid-p2p-topology.md) rejected storing everyone's files in
the cloud, where cost scales with all bytes and we hold data whether the user
wants us to or not.

An opt-in replica is the opposite in both respects. It is chosen, so cost scales
with the people who asked for it; and it holds ciphertext, so zero-knowledge is
untouched. The thing 0001 rejected was the default, not the capability.

## Consequences

**It answers an unsolved business problem.** The project has costs — relays, the
control plane — and no revenue line except "billing" in Phase 6. Charging for
sync is hard to justify when the user supplies the storage. Charging for
always-on availability is a clean thing to sell, because it is a real cost being
absorbed on their behalf.

**A replica is also the natural place to repair from.** A device with a failing
disk no longer has to wait for another laptop to be opened.

**The messaging changes.** "Your files never touch our servers" becomes "unless
you ask us to hold an encrypted copy". Opt-in and honest, but it is a cost.

## The risk being accepted

Running replicas as a service makes us a storage provider, with everything that
follows: legal requests, abuse reports, deletion obligations, uptime
expectations, and a support burden that does not exist today. That is a business
change rather than a feature.

Separating the capability from the deployment is what keeps that optional. The
engine work commits us to nothing.

## Deliberately not decided

- **Whether to offer paid replicas at all**, and pricing.
- **Whether the DERP relay doubles as one.** Tempting, since both are always-on
  infrastructure, but relays are stateless and bandwidth-shaped while storage is
  neither. Probably separate services.
- **Retention on a replica.** It likely wants a longer window than a laptop, or
  none, since being the copy of last resort is the job.

## Reversibility

**Low cost.** A replica is a role on an existing engine, not a separate
codebase, and nothing is stored differently because of it.
