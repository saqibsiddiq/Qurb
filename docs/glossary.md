# Glossary

Terms used throughout this project, defined plainly. Where a term has a
standard meaning in distributed systems, the definition here is the one that
matters for qurb specifically.

---

**BLAKE3** — The cryptographic hash function used to name chunks. Chosen over
SHA-256 because it is several times faster and parallelises well. In practice
hashing is not our bottleneck (disk I/O is), so the speed matters less than
expected — but it costs nothing to have.

**CAS — Content-Addressable Storage** — A store where the name of a piece of
data is derived from the data itself, specifically its hash. Storing the same
bytes twice is a no-op. Corruption is detectable by re-hashing. See
[CODEBASE.md](CODEBASE.md) section 2.1.

**CDC — Content-Defined Chunking** — Splitting a file at boundaries determined
by the content rather than by byte offset, so that inserting or deleting data
does not shift every subsequent boundary. The reason a one-byte edit to a large
file costs kilobytes rather than gigabytes. We use the FastCDC algorithm.

**Chunk** — A piece of a file, between 128 KiB and 2 MiB, averaging 512 KiB. The
unit of deduplication, storage, transfer, and integrity checking. Chunks are
compressed and encrypted individually.

**Chunk GC — Garbage Collection** — Reclaiming disk space by deleting chunks no
file references any more. Hard because deduplication means one chunk can be
referenced by many files and many versions, so deletion requires reference
counting. Not yet implemented; the first correctness problem of Phase 1.

**Cone NAT** — See *NAT*. A router whose external port mapping is the same
regardless of destination. Hole punching works. The good case.

**DERP — Designated Encrypted Relay for Packets** — Tailscale's name for relay
servers that forward encrypted packets between peers who cannot reach each other
directly. Runs on port 443 so it survives restrictive corporate firewalls.
Relaying costs bandwidth, so the fraction of connections needing it is a
number with a direct dollar cost.

**E2EE — End-to-End Encrypted** — Data is encrypted by the sender and decrypted
by the recipient, with no intermediate party able to read it. Our relays forward
bytes they cannot interpret.

**FastCDC** — The specific content-defined chunking algorithm we use. Faster
than the older Rabin fingerprint approach for the same quality of boundaries.

**FFI — Foreign Function Interface** — The mechanism by which Swift and Kotlin
call into the shared Rust core on mobile. Planned via UniFFI, which generates
the binding code.

**Hole punching** — The technique that lets two devices behind NAT connect
directly. Both send packets to each other simultaneously; each outbound packet
opens a hole in its own router's filter through which the other's packet can
return. Fails against symmetric NAT.

**Manifest** — The record mapping a file path to its ordered list of chunk
hashes. Reconstructing a file means fetching its manifest, then fetching each
chunk it names.

**Merkle DAG** — A structure where each node names its children by hash, so any
change to a leaf changes every hash on the path to the root. Lets two devices
compare an entire directory tree by exchanging one hash, then descending only
into the parts that differ.

**NAT — Network Address Translation** — What your home router does: many
devices share one public IP address. The reason two devices on different home
networks cannot simply dial each other, and therefore the reason STUN, hole
punching, and relays exist.

**Noise Protocol Framework** — A toolkit for building cryptographic handshakes.
We plan to use the `IK` pattern, which authenticates both parties using static
Curve25519 keys already known to each other and provides forward secrecy.
WireGuard uses the same framework.

**QUIC** — A transport protocol over UDP that includes encryption and supports
many independent streams on one connection. A stalled stream does not block the
others, unlike TCP. We use the `quinn` Rust implementation.

**Symmetric NAT** — See *NAT*. A router that assigns a different external port
per destination, which breaks hole punching because the address a peer learns
from STUN is not the address it will be contacted on. Connections involving
symmetric NAT usually need a relay. The bad case.

**STUN — Session Traversal Utilities for NAT** — A tiny protocol for asking a
public server "what address do my packets appear to come from?" The answer is
what peers exchange in order to attempt hole punching. Querying two different
STUN servers from the same local socket and comparing the answers is how we
classify a NAT as cone or symmetric.

**Tombstone** — A record marking a file as deleted, propagated to peers so they
delete their copies too. Necessary because the absence of a file is not
self-describing: without a tombstone, a peer cannot distinguish "deleted" from
"not yet received."

**Vector clock** — A per-device counter map, like `{laptop: 42, phone: 12}`,
used to determine whether one change happened before another or whether they
were concurrent. Concurrency means conflict. See
[decisions/0005-conflict-resolution.md](decisions/0005-conflict-resolution.md).

**Zero-knowledge** — Our servers hold no information that would let them read
user data. The strong version of the privacy promise, and the source of the
hardest product constraint: a lost key means unrecoverable data.

**Zstd** — The compression algorithm applied to chunks before encryption. Fast,
with a good ratio on text. Skipped for already-compressed formats where it would
burn CPU for nothing.
