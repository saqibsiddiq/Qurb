# qurb-storage

Local storage: content-defined chunking, an encrypted content-addressable
store, and the SQLite index that ties them together.

This is the first crate of Phase 1 and the foundation everything else sits on.
Conceptual background is in [../../docs/CODEBASE.md](../../docs/CODEBASE.md)
sections 2.1–2.2 and 2.5.

## Layers

| module | responsibility |
|---|---|
| `chunker` | split a file at content-defined boundaries, hash each piece |
| `format` | compress with zstd, encrypt with XChaCha20-Poly1305 |
| `cas` | move opaque payloads to and from `chunks/<2 hex>/<full hex>` |
| `db` | SQLite index: paths, chunk lists, reference counts |
| `gc` | reclaim chunks nothing references any more |
| `store` | the public API, and the ordering rules that keep the above consistent |

Each layer knows only about the one below it. `cas` does not know payloads are
encrypted; `db` does not know where payloads live.

## The two invariants

Everything else in this crate is in service of these.

**1. A chunk referenced by the index always exists on disk.**

Writes therefore go payload-first: the chunk is written and fsynced before the
index is told it exists. A crash in between leaves an orphaned payload, which
wastes space until swept. The opposite order would leave a reference to a
payload that was never written, which is unrecoverable without a peer to
re-fetch from.

Orphaned data is a cost. A dangling reference is a corruption. When only one is
avoidable, prefer the cost.

**2. Reference counts equal the links that exist.**

Deduplication means one chunk can belong to many files, so deletion has to be
counted rather than decided. The counts are maintained by SQL triggers, not by
Rust code, so they update in the same transaction as the row that caused the
change and cannot drift because a code path forgot. `Db::audit_refcounts`
re-derives them from scratch and is asserted in tests.

## Concurrency

SQLite permits one writer at a time, and this system has several: the engine
writing, the peer server reading from its own connection, and garbage collection
taking the write lock for its deletions.

A write re-establishes every chunk it needs **inside** its own transaction. The
obvious implementation checks first and references later, which loses a race:
collection removes only chunks nothing references, and a chunk about to be
referenced looks exactly like one until the reference exists. The write lock
covers the collector's work, not a check made earlier by someone else.

The foreign key on `file_chunks` is what caught that, refusing the insert rather
than allowing a reference to nothing — worth remembering when it looks like a
constraint that exists only for tidiness.

## Deletion and retention

Deleting a file writes a tombstone: the row stays, its chunk references stay,
and the content remains restorable. Garbage collection runs in two stages —
expire tombstones past the retention window, then reclaim chunks nothing points
at any more.

The second stage also catches chunks released by *editing* a file, which is what
makes previous versions recoverable.

The two windows compose rather than overlap, so a deleted file's payloads can
survive for up to twice the configured retention before the space comes back.
That errs toward keeping data, which is the right direction here, but it is a
real disk cost when choosing the window.

## Testing

```bash
cargo test -p qurb-storage
```

`tests/crash.rs` kills a real writing process with `SIGKILL` at seven different
moments and checks what survives. The invariant is the one the write ordering
exists for: a chunk referenced by the index always exists on disk. A crash may
orphan a chunk — wasted space — but must never leave a reference to a payload
that was never written.

The garbage collection tests carry the rest of the weight. `gc_never_removes_a_chunk_a_live_file_still_needs`
is the one that matters most: two files share content, one is deleted and
collected, and the other must still read back byte-exact. Getting that wrong
destroys data in a file the user never touched, without raising an error.

## Version vectors

Since schema v2 the index records, per path, the version vector it carries and
which device last changed it — plus this store's own identity and the counter it
stamps onto local changes.

The distinction that matters is between a change made *here* and a version
received from a peer. A local change advances this device's counter; an adopted
version keeps the vector it arrived with. Stamping a received version as local
would claim this device had seen changes it has not, and would make its history
dominate versions it should have conflicted with. `Store::put_file` takes the
first path, `Store::adopt` the second.

Writing identical bytes is deliberately not a change and does not advance the
clock, or a touched file would start beating a peer's genuinely newer version.

## Reading without holding the file

`read_file` returns the bytes. `read_file_into` and `read_content_into` write
them out a chunk at a time, so peak memory is one chunk — at most 2 MiB —
however large the file is. `adopt_file` is the same idea for the other
direction: it maps a file the caller has already written rather than taking a
buffer.

The buffering forms remain, because tests and the storage-only replica path use
them. Which to use is not a matter of taste:
[decision 0018](../../docs/decisions/0018-file-contents-never-cross-the-ffi.md)
says why anything a phone can reach must stream.

All of them verify the whole-file hash — but only once the last byte is written,
which is the earliest it can be known. A destination is therefore not
trustworthy until the call returns.

## Not yet built

- **Per-file keys.** One `ChunkKey` encrypts everything. Sharing a single file
  with someone else would mean sharing the key to all of them, so sharing needs
  this first.
- **Key rotation.** There is no way to change the master secret without
  re-encrypting every chunk, and nothing does that.
- **Streaming reads.** `read_file` builds the whole file in memory. Fine for the
  desktop, not acceptable inside an iOS FileProvider extension, which will need
  a chunk-at-a-time API.
