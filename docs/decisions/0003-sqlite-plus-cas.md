# 0003 — SQLite index alongside a content-addressable store

**Status:** Accepted
**Date:** 2026-09-07

## Decision

File metadata — paths, chunk lists, version history, sync state, search indexes
— lives in SQLite in WAL mode. Chunk payloads live in a separate
content-addressable store on the filesystem, at
`chunks/<first 2 hex of hash>/<full hash>`.

## Reasoning

These are two different problems and they want two different tools.

Metadata is relational and is queried in relational ways: resolve a path to a
chunk list, find every file referencing a chunk, order changes by generation,
walk a directory tree. SQLite does this in-process, with real transactions, in a
few megabytes of memory, on every platform we target including iOS.

Chunk payloads are large opaque blobs that are written once, read many times,
and never queried by content. A filesystem is already an excellent store for
that, and keeping blobs out of the database keeps the database small enough to
stay fast and to be feasible to sync.

The two-hex-character fan-out directory exists because filesystems degrade when
a single directory holds millions of entries. Splitting by the first byte of the
hash caps any one directory at roughly 1/256th of the chunk population.

## Rejected

- **RocksDB / LMDB** — key-value only, so the hierarchical and relational parts
  of the metadata would need a hand-built index layer on top. That layer is
  where the bugs would live.
- **Embedded PostgreSQL** — a separate server process on a consumer desktop is
  a deployment and support burden that cannot be justified.
- **Blobs inside SQLite** — bloats the database, slows every query, and makes
  incremental backup of metadata impractical.

## Tradeoff accepted

SQLite permits one writer at a time. All mutations must funnel through a single
writer task, which is a real constraint on the engine's internal design rather
than an implementation detail — it has to be designed in from the start, not
discovered later.

## Reversibility

**Medium.** Database access should sit behind a repository interface so the
storage layer can be replaced without the engine noticing. That interface does
not exist yet and should be established early in Phase 1, while it is cheap.
