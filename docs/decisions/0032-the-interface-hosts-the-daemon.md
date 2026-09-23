# 0032 — The interface hosts the daemon, and asks it nouns

**Status:** Accepted
**Date:** 2026-09-23

## Decision

Two parts, and they belong together.

**One process.** A graphical interface runs the daemon inside itself rather
than talking to one over a socket. `qurb-tray` already did this;
[`qurb-desktop`](../../crates/desktop/README.md) does the same.

**Two mechanisms for two kinds of question.** The daemon publishes its live
state on a `watch` channel ([`status`](../../crates/qurb/src/status.rs)).
Everything else — what devices, what files, what is available where, what
happened, what is still on its way — is a query against the index, and those
queries live in [`view`](../../crates/qurb/src/view.rs).

## Why one process

The alternative is a local socket and a protocol over it, which means a wire
format, a version negotiation, an authentication story for "who may connect to
my daemon", and a second place for every bug to hide. Nobody has asked for a
headless daemon with a separate interface attached, and the cost of building
one before anybody wants it is paid immediately.

The advisory lock on the store ([`lock`](../../crates/qurb/src/lock.rs))
already means one daemon per folder, so "the interface hosts the daemon" and
"the terminal hosts the daemon" are alternatives rather than things to run at
once — which is the honest shape of it either way.

What this does *not* prevent is a read-only interface alongside a running
daemon. SQLite in WAL mode allows exactly that, `qurb status` has relied on it
from the beginning, and every query in `view` is read-only for the same reason.
So a window can show a folder while another process syncs it. It simply cannot
*act* on it.

## Why two mechanisms rather than one

They change at different rates and are wanted in different shapes.

"Syncing, three devices, 40% through" changes many times a second, and only the
latest value is ever useful — a display that fell behind and had to catch up
through a queue of stale summaries would be showing the past. That is a `watch`
channel, and it already exists.

"What files are in this folder" changes rarely and is wanted in full, in order,
a page at a time. Pushing that through the same channel would either flood the
interface or make the status wait behind a listing.

So: hold the status channel open, and poll the view for detail. The daemon
bumps a generation when anything changes, which is the interface's cue to ask
again.

## Why availability is three values, not two

`Here`, `Elsewhere`, `OnlyHere`.

A listing that knows only whether bytes are on disk cannot tell the difference
between a file that is here and also on the phone, and a file that is here and
nowhere else in the world. They look identical, and a storage screen that
offered to free the second would be offering to delete it.

This is the same distinction the storage cap enforces
([decision 0025](0025-a-storage-cap-that-cannot-lose-data.md)); making it
visible is what lets an interface be honest about what "free up space" will do.

## Search is by name

Not by content. An index of what every file says is a second database and a
much larger promise than this product has made; searching names is what people
do most of the time and is answerable from what the index already holds.

Two limits, stated rather than papered over: SQLite folds case for ASCII only,
so `CAFÉ` does not find `café`; and what somebody typed is treated as text, so
`report_final` does not match `reportXfinal`.

## What this does not do

- **No notifications from the view.** It is pull-only. The push side is the
  status channel, which carries the daemon's state and not the index's.
- **No transfer progress.** A transfer in flight is live state, not history.
- **No writes.** Acting on a file — fetch, send, evict — goes through the store
  and the daemon as it always did.
- **No vault listing.** `files()` is the shared area. What a device holds in
  its own vault is a different list with a different meaning, and putting
  content somebody sent you in the middle of your own folder listing would be
  wrong in both directions.
