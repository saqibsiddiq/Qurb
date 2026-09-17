# 0018 — File contents never cross the FFI

**Status:** Accepted
**Date:** 2026-09-17

## Decision

The mobile FFI ([`crates/mobile-ffi`](../../crates/mobile-ffi/)) exposes no
function that takes or returns a file's bytes. Content moves by path:

```
export(path, destination) -> bytes written     writes to a file the caller names
import_file(source, path)                      reads from a file the caller names
```

The engine's receive path follows the same rule internally. Content is streamed
a chunk at a time from the transport to disk, never assembled in memory.

## Why

An iOS FileProvider extension — the process that makes files appear in the Files
app — runs under a memory ceiling in the tens of megabytes. Android is more
forgiving but still kills processes under pressure, and does it more readily to
one that has been backgrounded.

A whole-file API cannot meet that ceiling. Worse, it *appears* to: it works for
documents and photos and fails for video, so the failure shows up late, on real
users' libraries, as an intermittent crash rather than as a design error.

Measured on Linux 7.2.2 (CachyOS), release build, warm cache, as growth in
`RssAnon` while adopting one file:

| file | buffered | streaming |
|---|---|---|
| 256 MiB | 256 MiB of heap | 0 MiB |
| 1024 MiB | 1024 MiB of heap | 1 MiB |

And on an Android 14 emulator, receiving over a real QUIC connection rather than
reading from a local store — which is the case that actually matters:

| file | heap |
|---|---|
| 32 MiB | 4 MiB |
| 128 MiB | 4 MiB |
| 512 MiB | 5 MiB |

Both shapes are kept in `crates/engine/examples/peak_memory.rs` so the
comparison stays checkable rather than becoming a claim in this document.

There is a second benefit, which was not the reason but is worth having: bytes
that never cross the boundary are bytes UniFFI does not copy through its own
serialisation. A whole-file API would have paid for the file twice.

## What this costs

**The API is less convenient.** A caller that wants bytes in memory must write
them to a file and read them back. That is the right trade — the caller then
chooses to pay the memory cost explicitly, at a size it knows, rather than
having it imposed by a function signature.

**Verification moves to the end.** Content is checked against its hash only once
the last byte has been written, because that is when the hash is known. So a
destination is not trustworthy until the call returns, and the engine writes to
a staging name beside the destination and renames on success. The rename is
within one directory and therefore atomic.

That staging file is inside the watched tree, which means the watcher must
ignore it. Without that, a half-written file is indexed as a real one, its
rename is read as a deletion, and both the phantom and its removal are sent to
every other device. A test in `crates/watcher/src/ignore.rs` ties the two
crates together, because nothing else does.

**A storage-only replica still buffers.** A replica holds content without a
directory, so there is no file to stream to and `adopt` takes a buffer. Replicas
are servers rather than phones, so the ceiling does not apply, but the gap is
real and is stated in `crates/engine/src/peer.rs` where it lives.

## Alternatives rejected

**Return bytes, and let the caller worry.** This is what the API did. It works
until it doesn't, and when it doesn't the process is killed rather than given an
error, so there is nothing to catch and nothing to report.

**Return a byte range at a time, and let the caller loop.** Correct on memory,
but it puts the loop — and therefore the hash verification, and the decision
about when content is complete — on the far side of an FFI boundary, in two
languages, written twice. The verification would be the first thing to be
skipped.

**Hand over a file descriptor.** Avoids the copy entirely and is what a mature
version of this might do. Rejected for now because descriptor passing differs
between the platforms and across the FFI, and the measured cost of writing to a
path the caller names is already flat in file size. Worth revisiting if
profiling on a real device says the extra write matters.

## Consequences

- `Store::read_file_into`, `Store::read_content_into`, `Store::adopt_file` and
  `PeerClient::fetch_content_into` exist alongside their buffering counterparts.
  The buffering ones remain because tests and the replica path use them.
- `ContentSource::fetch_into` has a default implementation that buffers, so a
  source that cannot stream still compiles. A source that can stream overrides
  it. This is a seam where a mistake is silent — a source that forgets to
  override gets correctness and loses the ceiling.

  **That happened, in the same week this was written.** `NetworkSource` — the
  one that matters, since it is how a phone receives a file — implemented only
  `fetch` and inherited the buffering default. The network path held whole files
  in memory for a release, and nothing failed, because buffering is correct and
  merely expensive.

  The lesson is not "write a better warning": the warning was here, in this
  document, and it did not help. A default that is correct and slow cannot be
  caught by review or by the compiler. It is caught by measuring, which is now
  what `crates/mobile-ffi/tests/syncing.rs` does — receiving a 128 MiB file and
  failing if the heap grows by more than a fixed bound. Any future source that
  forgets the override fails that test.
- The memory promise is asserted by a test in `crates/mobile-ffi`, not by a
  comment. A change that reintroduces buffering fails rather than ships.
