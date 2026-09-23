# 0031 — What happened is written down

**Status:** Accepted
**Date:** 2026-09-23

## Decision

Events go in a table in the index — one row per thing that happened, with the
path, the size, the other device and a sentence where those are not enough.
`qurb activity` reads it.

The daemon's account of itself used to be its log. That is the right thing for
a terminal and answers nothing after a restart, which is a problem because
every question a person actually asks about sync is a question about the past:

> Why is this file not here?

A log cannot answer it. A table can.

## What is recorded

`stored`, `deleted`, `received`, `sent`, `collected`, `evicted`, `restored`,
`conflicted`, `paired`, `failed`. Each is a moment where the system did
something a person could be surprised by. An unchanged file is not one of them:
re-reading a file that has not changed is the hot path and runs on every
reconciliation.

Written at the layer that does the work, not the layer that asks for it.
`Store::put_file` and `Store::delete_file` record their own events, because the
bulk pass stores through worker threads with their own handles — recording in
the engine would mean each of those remembering to, which is a rule nothing
enforces. Transfers and conflicts are recorded in the engine, because only the
engine knows the other end.

## Best-effort, always

Every recording site logs and continues rather than failing the operation it
was describing. History is worth having and is never worth failing a sync over.
The one helper in `qurb-engine` exists so that decision is made once rather
than at each of eight call sites.

## Kinds are stored as text

Not as integers. Most of this project's debugging happens by opening the
database by hand, and a table that reads as sentences beats one that reads as a
legend to look up.

The consequence is that a newer build can write a word an older one does not
know. Those rows are kept as `Event::Other(word)` and displayed as the word,
because losing history to a downgrade is worse than showing one unfamiliar
term.

## Paging is by id, not by time

Two events in the same second are indistinguishable by time, and a page
boundary landing between them would repeat or skip one. `activity(limit,
before)` takes the id of the oldest row already shown.

## Two retention limits

Ninety days, and ten thousand rows, whichever bites first. They fail
differently: age alone lets a busy week grow the table without bound, a count
alone lets a quiet device keep rows from years ago. Pruned in the same pass as
garbage collection, because it is the same question — how far back does this
device remember — and one place to look is better than two.

Ninety days is longer than content retention (a week) because history costs
bytes where content costs megabytes.

## Privacy

This adds no exposure the index did not already have. `files` has held every
path in plaintext since the first migration; `activity` holds no content at
all, and no more paths than `files` does. It is bounded, and it is deleted with
the store.

What it must never hold — and does not — is anything derived from the recovery
phrase, key material, or file contents. The `detail` column takes error
messages and conflict filenames, both of which the index already contains.

## What this does not do

- **No progress.** A transfer in flight is not represented; only its outcome.
  A progress bar needs the daemon's live state, which is the watch channel's
  job, not this table's.
- **No subscription.** An interface polls, or reads the daemon's status
  channel. A notify-on-write layer over the same table is the next piece of
  work, not this one.
- **No cross-device history.** Each device records what it saw. Two devices
  will not agree on the order of everything, and reconciling their histories
  would be a sync problem of its own for no benefit anybody asked for.
