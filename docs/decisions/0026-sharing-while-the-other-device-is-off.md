# 0026 — Sharing while the other device is off

**Status:** Accepted
**Date:** 2026-09-22

## Decision

A person can share a file into qurb from anywhere on their phone, at any time,
with every other device switched off. There is **no outbox and no retry
queue**, because there is nothing to queue: the file is written into the synced
folder and indexed like any other file, and the index already knows that no
other device holds it. It reaches the other devices whenever one is next
reachable, without anyone being asked to do anything.

Three pieces:

1. **A share target.** `ShareActivity` handles `ACTION_SEND` and
   `ACTION_SEND_MULTIPLE` for any type. It needs no network and does not wait
   for one.
2. **A delivery report.** A device that finishes receiving content tells the
   device it got it from: `Got { content }`, the only message in the protocol
   that asks for nothing.
3. **A question, not a list.** "What is waiting to be delivered" is answered by
   asking the index which files this device made that no other device is known
   to hold — not by keeping a list of things that failed.

## Why no outbox

An outbox is the obvious design and it is the wrong one here. It would be a
second record of what needs to happen, alongside the index, which already knows
everything relevant: which files exist, which device made each one, and which
content other devices hold.

Two records of the same fact disagree eventually, and the ways they disagree
are bad in both directions. An outbox entry for a file since deleted retries
forever. A file the outbox forgot is never sent, and looks fine — the worst
kind of sync bug, because nothing reports an error.

So the question is asked of the index each time:

```sql
live, made by this device, and no row in `replicas` for its content hash
```

A file appears in that answer the moment it is shared and leaves it the moment
somebody takes delivery. Nothing has to be enqueued, dequeued, or cleaned up,
and the answer cannot drift from the truth because it *is* the truth, read
fresh.

## Why the receiver reports delivery

Before this, a device could not tell "delivered" from "tried". Every message in
the protocol was a device fetching what it wanted, so the device being fetched
*from* learned nothing: it served some chunks, and whether they added up to a
file that got written was not its business.

That is fine for syncing and useless for the promise this feature makes. "Your
photo will reach your computer" is worth saying only if the phone can later say
whether it did.

`Got { content }` is sent by the receiver after the content is committed —
after the rename, not after the last byte arrives. A report of a delivery that
then failed is worse than no report, because it is evidence another device may
drop its own copy on.

Which device sent it is taken from the connection's certificate, never from the
message. A peer cannot claim delivery on another device's behalf, which matters
because [0025](0025-a-storage-cap-that-cannot-lose-data.md) lets a storage cap
drop a local copy on the strength of exactly this record.

Failures are swallowed. It is a courtesy to the other end, and a sync that
worked must not be reported as failed because the closing remark did not get
through.

## What actually carries the file across

Nothing new. The phone's background worker already runs about every fifteen
minutes — WorkManager's floor, and in practice hours on a dozing phone — and
retries with exponential backoff when nothing answers. A share additionally
asks for one pass immediately, which usually succeeds and makes the whole thing
feel instant; when it does not, the periodic schedule is the backstop and
nothing is lost.

The transfer itself is the phone announcing and serving for the length of a
pass, and the desktop — which watches for peers appearing — dialling it and
pulling. That mechanism predates this work.

Measured, two devices through a rendezvous service on loopback: a file shared
with the desktop **not running at all** was saved, correctly reported as held
only by the phone, survived a sync attempt that reached nobody, and arrived
byte-for-byte once the desktop came up — with no further action on the phone.
The phone then correctly reported nothing outstanding.

## What this does not do

**It is not instant when the other device is asleep.** A phone cannot be woken
by another device without push infrastructure, which qurb does not have and
which would mean a third party learning when your devices talk. So the phone
decides when to look, and Android decides how often to let it. Shares made
while a laptop is shut can take as long as the laptop takes to come back plus
one background window.

**Nothing is retried faster after a failure than the platform allows.**
WorkManager's backoff is the whole retry policy. This is deliberate: the
alternative is an app that drains a battery looking for a computer that is off.

**Android only.** iOS has no app at all yet, and its share-extension model is
different enough that this design should be revisited rather than copied.

**Files land at the top level** of the synced folder, named as the sharing app
named them. A name already in use gets a numeric suffix — `IMG_0001 (2).jpg` —
rather than being treated as a new version of the file already there, which
under single-copy storage would destroy it. The cost is that re-sharing the
same photo twice leaves two copies; the alternative costs someone an unrelated
file, and only one of those is recoverable by the person it happens to.

There is no way to share *into* a subfolder, and no way for other apps to see
the synced folder at all except through the DocumentsProvider.
