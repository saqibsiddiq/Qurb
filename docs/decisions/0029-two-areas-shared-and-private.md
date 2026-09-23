# 0029 — Two areas: one shared, one private per device

**Status:** Accepted
**Date:** 2026-09-23

## Decision

A path belongs to one of two places:

- **The shared area.** Every paired device converges on it. This is what every
  path was before this decision, and what every path still is unless something
  says otherwise.
- **A device's private vault.** Other devices may put content into it and may
  not read it back. Only the owning device may list it or fetch its bytes.

Recorded as a nullable `scope` column on `files`: `NULL` is shared, a device id
is that device's vault.

## Why both, rather than one

The product model asks for private per-device vaults. The engine implemented
one namespace shared by everything — a syncing device wants every path, the
tree request returns everything a device knows, and a chunk request serves any
hash to any paired peer with no check beyond pairing.

Those are two different products, and each is right about something.

The shared area is what qurb's own README promises: put a file in a folder on
your laptop and it appears on your phone. Removing it to make room for vaults
would delete the behaviour the project has been built and measured around.

Vaults are what a person actually wants for the rest: a phone's photographs
being on the phone, sent to the laptop when asked, and not silently readable
from every device in the house because they happen to share a key.

Keeping both costs one column and one concept. Choosing one would have cost the
other entirely.

## Privacy has to be a property, not a drawing

An interface that declines to show a vault changes nothing about what a peer
can ask for. A peer does not need a listing to fetch content: a hash is a name,
and a device that once saw a file — or guessed — can ask for the bytes
directly.

So the check is in the protocol, on all three of the requests that can reveal
something:

| request | check |
|---|---|
| `Tree` | answers with the shared area plus the asker's own vault |
| `Manifest` | refuses content the asker may not see |
| `Chunk` | refuses bytes the asker may not see |

A refusal is `NotFound`, indistinguishable from content this device does not
hold. "You may not have this" and "there is no such thing" must look the same
from outside, or the refusal itself becomes a way to enumerate what exists.

## Three audiences, not two

The first implementation took an `Option<&DeviceId>` — `Some` for a peer,
`None` for local use — and it was wrong in a way the test suite caught
immediately: seven existing tests broke, because a peer that authenticates by
pinned fingerprint but is not recorded in the `peers` table resolved to `None`
and was refused everything.

There are three cases and conflating any two is a bug with a security
consequence, so [`Audience`](../../crates/storage/src/db.rs) is an enum:

- **`Ourselves`** — this device. Sees everything it holds; an interface showing
  somebody their own files is not a peer.
- **`Device(id)`** — a peer this store recognises. The shared area and that
  device's vault.
- **`Unplaced`** — a peer that authenticated but whose device is not recorded
  here. The shared area only.

`Unplaced` is the interesting one. It is not refused outright, because the
connection already proved it is trusted and the shared area is exactly what
trust entitles a device to. Being stricter would break a paired device whose
bookkeeping is incomplete and buy nothing — it owns no vault here, so it is
shown none either way.

## Deduplication decides a case that looks like a leak

If the same bytes are in both the shared area and somebody's vault, they are
served to everyone. That looks like a hole and is not: the shared copy already
entitles every device to those bytes, and refusing them would deny content the
asker can obtain another way while revealing that a vault holds the same thing.

## A path is no longer a unique handle

The `files` table carried `path TEXT NOT NULL UNIQUE`, which was right when
there was one namespace and is wrong now: a device could not hold `photo.jpg`
for two different phones, nor hold one for a phone while having its own in the
shared area.

SQLite cannot drop a column-level UNIQUE, so the table is rebuilt. Three things
about that were easy to get wrong and are worth recording:

**`UNIQUE(scope, path)` would not have worked.** SQL treats NULLs as distinct,
so two shared rows with the same path would both have been allowed — the exact
bug the constraint exists to prevent. It is two *partial* unique indexes
instead: one on `path` where the scope is null, one on `(scope, path)` where it
is not.

**Foreign keys have to be off while a table is rebuilt.** `file_chunks`
cascades from `files`, and dropping the old table with them enforced would take
every chunk reference in the store with it. Migrations now run with them off —
they are the only place schema changes happen — and `foreign_key_check` runs
afterwards, so a migration that got a reference wrong fails at startup instead
of surfacing later as missing content.

**Every path-keyed query had to say which namespace it meant.** They all meant
the folder, which is the shared area, so saying so was the correct and minimal
change. The exception is content lookups: whether this device holds some bytes
has nothing to do with which namespace they sit in, and filtering those would
have hidden a device's own vault from itself. The vault tests caught exactly
that when the filters went in too broadly.

Verified against a copy of a real store: nineteen files and 1,294 chunk
references migrated from version 6 to 8, foreign keys intact, and
`qurb verify --deep` reading and hashing every chunk afterwards.

## What this does not yet do

**Nothing puts a path in a vault.** `Store::set_scope` exists and the
enforcement around it works, but no command, no sync operation and no interface
calls it. Sending content into another device's vault is the transfer primitive
this unblocks, and it is the next piece of work rather than part of this one.

**Vaults do not converge.** The reconciliation machinery still plans against a
tree; a vault-scoped path is simply absent from the tree a peer is shown. What
happens when two devices both hold a version of something in one vault is
undefined, because nothing can yet create that situation.

**Nothing migrates.** Every existing path stays in the shared area, which is
what it was and what its owner expects.
