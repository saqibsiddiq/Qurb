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

### And reporting what is merely held

A report sent when a transfer finishes covers everything from then on and
nothing from before. A device that received a file last week holds it just as
truly and never said so — and because a file both devices already have is never
transferred again, that gap could never close on its own. Every such file would
be counted as delivered nowhere for as long as it existed.

Found by testing rather than by reasoning: on real hardware both devices held
all nine files and each still claimed several were only on itself.

So a device also reports content it is *holding*, not only content it has just
received. On each sync it tells the peer about content the peer made that is
sitting here, a few dozen at a time, each one only once — a `reported` table
records what has been said to whom, because the statement is worth making once
and not on every sweep for every file.

The statement is identical in kind to the one sent after a transfer, and just
as true: "I have these bytes." It is the *timing* that differs.

One thing this exposed, worth stating because it is the sort of thing that
looks like it works: the reports have to be sent **before** the check for an
empty plan, not after. Two devices that agree about everything produce an empty
plan every single time, and those are exactly the devices that have holdings to
report. Sending afterwards means never sending at all in the one case it is
for.

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

**Nothing is retried faster than the platform allows** — about fifteen minutes,
WorkManager's floor for periodic work, and longer on a dozing phone.

It was very much worse than that until hardware testing caught it. "Nothing
answered" was reported to WorkManager as a *retry*, on the reasoning that the
other device being asleep is ordinary and exponential backoff would keep it
from draining the battery. That is exactly backwards: every unanswered attempt
doubled the delay, so a phone whose laptop had been off for a while scheduled
its next attempt **three hours out** — measured on a Galaxy S23, with a file
shared at 18:01 still sitting there at 18:06 with the laptop running beside it.
The moment the other device came back was the moment this one had stopped
looking.

Backoff is for transient errors. "Nobody is awake yet" is the steady state, and
its answer is the ordinary period, which only applies when the worker reports
success. Retrying is now reserved for a sync that ran out of time with work
still to do, and its backoff is linear rather than exponential.

Two supporting fixes came from the same finding. A share asks for an
**expedited** one-time sync and waits for that request to be written down
before the screen closes — a process with no remaining components can be killed
immediately afterwards, and a share that silently schedules nothing is the
exact failure this feature exists to not have. And the periodic schedule is
**versioned by name**, because one registered with `KEEP` survives reinstalls
and so does the backoff it accumulated: a phone would otherwise carry its
three-hour delay across the update that fixed it.

Measured afterwards on the same phone: a file shared through the system share
sheet reached the laptop in under five seconds, unattended.

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
