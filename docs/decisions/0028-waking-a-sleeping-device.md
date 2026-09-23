# 0028 — Waking a sleeping device

**Status:** Accepted
**Date:** 2026-09-23

## Decision

The rendezvous service may hold a **push token** for a device and use it to
poke that device when another has something for it. On Android this is Firebase
Cloud Messaging; on iOS it would be APNs.

The poke carries **nothing**: no filenames, no sizes, no counts, not even which
peer has something. A woken device syncs with the peers it already knows.

It is off by default. A service with no credentials, and a build with no
`google-services.json`, behave exactly as they did before this existed.

## Why there is no alternative

A phone cannot hold a socket open in the background. Android stops a background
app's connection within minutes of the screen going off, and iOS never allowed
one at all. So the device most in need of being told something is precisely the
device that cannot be told.

Everything else was tried first and is already built:

- **[0027's `Waiting` message](../../crates/signal/README.md)** tells a peer
  there is work for it, and the service keeps the note if the peer is away. But
  a note can only be delivered when the peer next connects, and what decides
  *that* is the phone's own schedule.
- **The background worker** asks about every fifteen minutes — WorkManager's
  floor — and longer while dozing. Measured on a real phone: a file shared at
  18:01 was still there at 18:06 with the laptop running beside it.
- **A foreground service** would keep a connection alive, at the cost of a
  permanent notification and, since Android 14, a declared type that "syncing
  files" fits only awkwardly. It is also capped at six hours a day.

Push is what every sync product uses, for this reason.

## What it costs, plainly

**Google learns that a device was poked, and when.** Not what changed, not
whose, not how much — the message is a constant. But the timing is real
metadata: somebody with access to it could tell when a person's devices are
active.

That is a genuine compromise in a product whose pitch is not depending on
anyone's servers, and it is worth being exact about what is and is not given
up:

- Files, filenames and keys never reach Google, and cannot: the poke has no
  room for them and the devices transfer directly.
- The rendezvous service still learns only blinded identifiers —
  [0016](0016-what-signalling-learns.md) is unchanged.
- What is new is that a third party sees a timing signal it did not see before.

**It is optional, and honestly so.** Not a checkbox that quietly does nothing:
the Android build genuinely omits the Firebase SDK when there is no project
configured, through a separate source set rather than a runtime branch. A
person who objects can build qurb without it and lose only latency.

## When a device is woken

Only when it could not simply be told, and only because the device asking is
there to sync with.

That second condition is the one worth stating. Waking a phone for a peer that
is not itself online spends the phone's battery to find nobody — so the poke
happens at the moment another device says it has something, which is by
definition a moment that device is connected.

## What the service now holds

A token per device that offered one, outliving both the connection and the
group. That last part is deliberate and was got wrong first: a laptop shut
overnight beside a sleeping phone empties the group, and dropping the token
then would mean the laptop could never wake the phone when it came back.

So they are bounded by count instead, oldest evicted first, and devices
re-register on every connection — an eviction costs at most one missed wake-up
and heals itself.

## Verified against the live service

Measured on 2026-09-23, a Galaxy S23 asleep with its screen off, a laptop, and
a rendezvous service holding real credentials:

```
13:22:18.579412  laptop   local changes stored=1
13:22:18.579876  service  waking a device that is not connected
13:22:19.281     phone    woken by another device
13:22:35         phone    sync: reached=1 adopted=1
```

**Seven hundred milliseconds** from the change to the phone waking. The file
arrived complete and the laptop then reported nothing outstanding, meaning the
phone acknowledged holding it.

The credential half — reading the key, signing the RS256 assertion, and Google
accepting it — is exercised at startup rather than on the first device that
needs waking. A key that does not work is a thing to find out then, not months
later when somebody's phone quietly stops being prompt.

## Tokens are held in memory, and that is survivable

Restarting the service forgets every token, so the first change after a restart
wakes nobody. Devices re-register on their next connection, so it heals itself
and costs at most one delayed sync. Observed during testing and left as it is:
persisting them means a database, and a database is the thing this service is
valuable for not having.
