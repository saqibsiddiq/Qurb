# 0030 — Sending a file to one device

**Status:** Accepted
**Date:** 2026-09-23

## Decision

A device can put a file into one other device's private vault
([decision 0029](0029-two-areas-shared-and-private.md)) and that file goes to
that device and to nowhere else.

Four rules, which together are the whole feature:

1. **The sender keeps the bytes.** They live in the sender's chunk store, not
   in its folder, until the recipient confirms they arrived.
2. **A delivery is taken once.** Keyed by content, counting tombstones, so that
   a file the recipient deleted does not come back.
3. **The recipient files it privately.** It lands in their folder like any other
   file, but its index row is scoped to them, so it is never advertised onward.
4. **The sender releases it first.** Once the recipient confirms, the sender's
   copy is the first thing dropped when the storage cap bites — before any of
   the sender's own files.

Rule 4 is the user's explicit instruction: *keep it, but evict it first.*

## Why the sender keeps its copy at all

The alternative — hand the bytes over and forget them — only works if the
recipient is awake. A phone in a drawer is the ordinary case, not the
exceptional one, and a feature that requires both devices to be on at the same
moment is a feature that mostly does not work.

Holding the bytes also means the sender can delete their own copy of the
original immediately. The file was picked from a share sheet, somewhere outside
the synced folder; the user's next action is often to tidy it up. A send that
evaporated when they did would be a promise the product did not keep.

## Why a vault entry is never backed by the sender's folder

Single-copy storage ([decision 0024](0024-the-file-is-the-payload-store.md))
says a materialised file *is* its own payload store: the chunk store holds
nothing for it, because the file on disk can be read instead.

A vault entry must not work that way, even when a file of the same name and the
same content happens to be sitting in the sender's folder. If it did, deleting
your own `report.pdf` would quietly destroy the copy you sent somebody — and
that is not a say the sender should have over another person's data. Vault
content is always chunked.

The recipient's own vault is the exception, and not really an exception: on the
recipient the file *is* theirs, written into their folder and read from there.

## Why "delivered", not "reconciled"

Reconciliation asks which of two histories of a shared path should win. A file
somebody sent you has no shared history and no counterpart to lose to. Running
deliveries through the same machinery would have the recipient offer the sender
their own file back, and a deletion on either side argue with the other.

So vault entries are filtered out of reconciliation and turned into a separate
list of things to collect. The recipient takes each one once and then owns it;
what they do with it afterwards is not the sender's business, and a tombstone
on the sender's side is not the recipient's.

## Not every copy elsewhere is a copy you can ask for back

This is the subtle part, and it was found by a test rather than by reasoning.

The storage cap is allowed to drop a local file when another device is recorded
as holding the same content. That record — a row in `replicas` — used to mean
one thing: somebody has these bytes.

With vaults it can mean two. A device that collected content into its *private
vault* holds the bytes and will never hand them back, because this device may
not read another device's vault. Counting that as "the content exists
elsewhere" would let the cap drop a shared-area file whose only other copy sits
behind a door this device cannot open. That is data loss wearing eviction's
clothes, and deduplication makes it reachable: one payload can serve both a
vault entry and a shared file.

So `replicas` gained a `private` column (migration `V9`). Ordinary deliveries
are unchanged and existing rows default to `0`, which is both true and the
conservative reading. A private row is enough to stop holding content *for* a
device, and never enough to drop anything of this device's own.

The releasable-chunk rule that falls out of it:

> A chunk may be released if it belongs to a vault entry the recipient has
> collected, **and** no live file anywhere still needs it from the chunk store.
> "Still needs it" means not materialised in this device's folder and not
> recorded as reachable — where reachable is an ordinary replica, or a vault
> delivery to the very device whose vault the entry is in.

The recipient side has the mirror of the same hazard. When it takes delivery it
records the sender as holding the content — but as a *vault* record, because
the sender is about to release it. Two devices each treating the other as their
fallback, and each releasing on that basis, would lose the file between them.

## Naming collisions are ordinary

The sender names the file the way they think of it. Two people can both have a
`report.pdf`, and neither is wrong, so an arriving file whose path is already
taken is filed beside the existing one as
`report.from-<device>-<when>.pdf` — the same machinery as a conflict name, with
a different word because nothing went wrong.

## Protocol version

Tree entries gained a `private` flag on the wire, and the ALPN identifier went
from `qurb/0` to `qurb/1`.

That byte is not optional. A build that ignored it would adopt content sent to
another device's vault as ordinary shared content and advertise it to the whole
fleet — a silent, immediate, irreversible leak. Refusing to talk to an older
build is the correct outcome, and the reason the identifier is versioned.

**Consequence:** every device must be rebuilt together. A phone running an
older APK will not connect until it is updated.

## What this does not do

- **Vault content is not selectively synced.** The recipient takes everything
  put in its vault, whole, the next time it syncs. There is no "accept this
  one" step.
- **The sender cannot withdraw a send** once the recipient has it. Deleting the
  sender's row stops future deliveries; it does not reach into the recipient's
  folder.
- **Replicas still cannot hold vault content usefully.** A replica has no
  folder, so a delivery routed through one would be held as chunks and never
  released, because no replica is anybody's vault owner. Sending to a device
  whose content a replica is meant to carry is not yet a supported shape.
- **There is no user interface for it yet.** `qurb send <file> to <device>` on
  the command line is the whole of it. The desktop and Android front ends are
  the next piece of work.
