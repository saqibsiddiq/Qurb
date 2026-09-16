# 0007 — On-disk chunk format

**Status:** Accepted
**Date:** 2026-09-09

## Decision

Chunk payloads are stored as:

```text
offset  size  field
0       4     magic "QRB1"
4       1     format version
5       1     flags: bit 0 = zstd compressed
6       2     reserved, must be zero
8       24    XChaCha20-Poly1305 nonce
32      ..    ciphertext, 16-byte authentication tag appended
```

Compress first, then encrypt. The 32-byte header is passed as associated data,
so it is authenticated but not encrypted. Compression is kept only when it saves
at least 5% of the chunk.

## Reasoning

**The format is fixed before key management exists.** This is the point of the
decision. Changing a stored format after users have data means rewriting every
chunk they own; changing where a key comes from does not. So the expensive half
is settled now and the cheap half is deferred, rather than the other way round.

**Compress before encrypting**, because ciphertext is incompressible by
construction. The ordering is not optional. It does leak a little information —
an attacker observing stored sizes learns roughly how compressible a chunk was —
which is accepted here: the alternative is giving up compression entirely, and
the leak is over chunks of a user's own files rather than across a trust
boundary.

**Compression is decided by measurement, not by file extension.** Trying zstd
and keeping the result only if it helps handles JPEG, MP4, and every future
already-compressed format without maintaining a list. The wasted CPU on
incompressible data is small next to the disk read that produced it, which
Phase 0 established is the actual bottleneck.

**XChaCha20-Poly1305 with a random nonce.** The 192-bit nonce is wide enough
that random generation is safe without a counter, which matters because chunks
are written from several call sites and a shared counter would be a
synchronisation point. ChaCha20 rather than AES-GCM because it is fast in
software on phones and older hardware without AES instructions.

**The header is authenticated.** Without that, flipping the compression flag
would make a reader mis-decode a valid payload. Passing the header as associated
data makes tampering a decryption failure instead.

## Consequences

Every chunk carries 32 bytes of header plus a 16-byte tag: 48 bytes of overhead.
Against a 512 KiB average chunk that is under 0.01%, and it is a further reason
not to shrink chunks much below the current size.

Identical plaintext produces different ciphertext on each device, since nonces
are random. This does not harm deduplication, which operates on plaintext hashes
throughout — the ciphertext is only a storage and transport representation.

## Reversibility

**Low cost now, high cost after launch.** The version byte allows a future format
to coexist, but converting existing chunks means rewriting a user's entire store.
Settle any remaining doubts before there is real data.
