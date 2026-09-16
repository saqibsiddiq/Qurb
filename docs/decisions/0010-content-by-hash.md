# 0010 — Content is requested by hash, never by path

**Status:** Accepted
**Date:** 2026-09-16

## Decision

When the engine needs file content it does not have, it asks for it by BLAKE3
hash and size:

```rust
trait ContentSource {
    fn fetch(&mut self, hash: &[u8; 32], size: u64) -> Result<Vec<u8>>;
}
```

Not by path. Before asking at all, it checks whether any live path on this
device already holds that content.

## Reasoning

**A path is not a stable name for content.** The content a plan calls for
routinely lives somewhere else, or nowhere, on the device that has it:

- A conflict writes the losing version to a new name. No device has ever had a
  file at that path, so no device could serve it by path.
- A file renamed locally has the same bytes under a different name.
- The same content can arrive at two paths at once, from a copy.

Asking by path fails in every one of those cases, while asking by content
succeeds. The chunk store is already content-addressed; making the transfer
interface match it removes a translation that could only ever introduce bugs.

**It makes renames and copies free.** Checking local content first means
adopting a peer's version of a file already held under another name costs an
index lookup rather than a transfer. Over a real network that is the difference
between renaming a 4 GB video costing nothing and costing 4 GB.

**It is the shape the transport wants anyway.** Peers will exchange chunks by
hash, because that is how deduplication works at all. A path-keyed interface
would have to be unwound before the network could implement it.

## Consequences

The index needs to answer "does any live path hold this content", which is a
partial index on `content_hash`. Small, and it pays for itself the first time a
large file is renamed.

`ContentSource` is the seam where the network goes. Everything above it —
planning, conflict resolution, writing files, recording vectors — is finished
and tested. Below it there is currently one implementation that reads another
local store, which is what the two-device tests use.

## What this does not yet do

Content is fetched whole. The chunking that makes a small edit to a large file
cheap is in the store but not in the transfer path, so a one-byte change to a
2 GB file currently moves 2 GB between devices. Fixing that means fetching by
*chunk* hash rather than file hash, and belongs with the network work where the
saving actually appears.

## Reversibility

**Low cost.** One trait with one method, and a single implementation behind it.
