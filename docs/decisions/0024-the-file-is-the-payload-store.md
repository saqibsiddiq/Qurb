# 0024 — The file in the folder is the payload store

**Status:** Accepted
**Date:** 2026-09-22

## Decision

When a device materialises a file in the sync folder, the chunk store does
**not** keep a second encrypted copy of that content. The file the user can see
*is* the payload store for the chunks it holds. The chunk store keeps only what
the folder cannot supply.

The index still records every chunk — hash, size, order, reference count. Only
the payload is skipped, and its `stored_size` is recorded as `0`, which is
true: it occupies nothing of its own.

A store with no folder attached — a storage-only replica — keeps every payload,
because nothing else has them.

## The problem

Every synced file cost twice its size. Once as the file in `~/qurb`, once as
compressed, encrypted chunks in `~/qurb/.qurb/chunks`. A 10 GB allowance would
hold 5 GB of files.

Measured on the development laptop on 2026-09-22 before the change: 7.6 MB of
files in `~/qurb`, and 7.5 MiB of chunk payloads for exactly that content, in a
90 MB chunk store. (The other 82 MB is one deleted 2.2 GB file inside its
retention window — a separate matter, see "What this does not fix".)

This was about to get worse, not better: the next piece of work is a storage
cap, and a cap that counts each file twice is a cap on half of what the user
thinks they are limiting.

## How it works

Three changes, all in `crates/storage/src/store.rs`:

1. **`Store::in_tree(root)`** attaches the sync folder to a store. Absent, the
   store behaves exactly as before. `Store::open` deliberately does not infer
   it — `Store::root()` is the `.qurb` directory, not the folder above it, and
   guessing would be wrong for a replica.

2. **`put_manifest` skips the payload write** for a file whose logical path
   names a real file in the attached folder. Checked by path against the
   filesystem, not assumed: `put_file` accepts any source path, and skipping
   the write for content that is *not* in the folder would lose it entirely.

3. **`read_chunk` falls back to the folder** when the chunk store has no
   payload. `Db::locate_chunk` answers "which live file holds this chunk, and
   at what offset" — the offset is not stored but computed, as the running sum
   of the sizes of the preceding chunks, with a SQL window function. The bytes
   read are hashed before being returned.

That last step is what makes the whole thing safe. If the user edits the file,
the bytes at that offset no longer hash to the chunk that was asked for, and
the read **fails** rather than returning the wrong content. An unreadable old
version is a recoverable situation; a silently wrong one is not.

`verify` was taught the same distinction: a chunk absent from the chunk store
but readable from the folder is healthy, not missing. Without that, `verify`
would report almost every chunk as lost on exactly the devices people run it
on.

## What this costs

**Reads of tree-backed chunks are a seek and a hash, not a decrypt.** Cheaper
in CPU, but they touch the user's file rather than the store's, so a file open
elsewhere is read concurrently. That is safe to do and may return torn data
mid-write, which the hash check catches.

**Editing a file makes its old versions unreadable**, where before they were
recoverable for the retention window. This is a real reduction in what the
retention window guarantees, and it is accepted because `restore_file` is not
exposed to users — it exists and is covered by tests, but no command calls it.
If undelete is ever offered as a feature, this decision has to be revisited for
superseded versions specifically.

**A file moved or renamed outside qurb's knowledge takes its payloads with it.**
The chunks become unreadable until the next scan re-indexes the file at its new
path. Previously the chunk store would have covered the gap.

## Reclaiming existing stores

Nothing re-indexes a file that has not changed — that is what makes an
unchanged file cost a `stat` rather than a read — so a store written before
this change would keep its duplicates forever. `Store::reclaim`, exposed as
`qurb reclaim`, is the one-off pass that removes them.

It is safe to interrupt. Each chunk is read back out of the folder and hashed
**before** its payload is deleted, so a payload is only ever dropped once the
bytes are known to be readable elsewhere. A store with no folder attached
reclaims nothing, which is the guard against deleting the only copy.

Measured on the development laptop, 2026-09-22, on the real store described
above: 7.5 MiB freed across 15 chunks, `qurb verify --deep` clean afterwards.

## What this does not fix

**The 82 MB of retained chunks** for the deleted 2.2 GB video. Those belong to
a tombstoned file inside its retention window and are correctly kept. They are
not duplicates and `reclaim` does not touch them.

**`Store::gc` has no callers outside tests.** Collection has never run in
practice, so superseded and expired chunks accumulate indefinitely. This is
unrelated to double storage but has to be fixed before the storage cap means
anything — a cap that cannot free space is a cap that stops working.

**There is no way to read a file out of the index from the command line.** Deep
`verify` reads and hashes every chunk, which is the per-chunk check, but
whole-file reassembly on real data is only covered by tests. The storage cap
needs fetch-on-demand anyway, and that is when the command should appear.

## Alternatives rejected

**Store chunks only, and materialise files on demand.** This is what a
content-addressed store wants to be, and it is wrong here: the folder is the
product. A user who cannot open their files in a file manager has not been
given private cloud storage.

**Hard-link chunk files to file contents.** Chunks are compressed and
encrypted; the bytes are not the same bytes. Even uncompressed, chunk
boundaries do not align with anything a filesystem can share.

**Keep both copies and tell the user the cap is half.** Honest, and a worse
product.
