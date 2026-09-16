# Phase 0 spike

**Throwaway code.** It exists to answer whether the core technical assumptions
hold, not to become the engine. Nothing here should be copied into `crates/`.

Results, measurements, and what they changed about the plan:
[../../docs/phases/phase-0-spike.md](../../docs/phases/phase-0-spike.md).

## What is here

| file | purpose |
|---|---|
| `src/lib.rs` | FastCDC chunker and content-addressable store |
| `src/bin/chunkbench.rs` | measures the five Phase 0 kill criteria |
| `src/bin/sweep.rs` | compares chunk size configurations on a real corpus |
| `src/bin/quictest.rs` | NAT classification and QUIC throughput |

## Running

```bash
cargo build --release
```

```bash
# chunking, dedup, round-trip integrity, boundary stability
./target/release/chunkbench ~/Downloads
```

```bash
# chunk parameter comparison
./target/release/sweep ~/Downloads
```

```bash
# NAT classification — run on every network you care about
./target/release/quictest stun
```

```bash
# QUIC throughput between two machines
./target/release/quictest serve 0.0.0.0:5000       # machine A
./target/release/quictest send <A-addr>:5000 512   # machine B
```

Running `chunkbench` with no argument generates a 2 GiB synthetic corpus. Set
`SPIKE_SCRATCH` to a disk-backed path first — it defaults to `/tmp`, which on
many Linux systems is a RAM-backed tmpfs the corpus will fill.

Prefer real files over the synthetic corpus. The synthetic deduplication figure
is manufactured by a planted duplicate region and means nothing.

## Deliberate shortcuts

These are why this code is disposable:

- **No reference counting, so no chunk deletion.** The store can only grow.
  Deduplication means a chunk may be referenced by many files and versions, so
  safe deletion needs refcounting. This is the first correctness problem Phase 1
  has to solve.
- **No encryption or compression.** Chunks are stored in the clear. The real
  engine compresses with Zstd and encrypts with ChaCha20-Poly1305 before writing.
- **`mmap` without handling concurrent truncation** — SIGBUS if another process
  truncates a file mid-read.
- **QUIC peer verification disabled.** Real peer identity comes from Noise IK
  over static Curve25519 keys, not the web PKI. The self-signed certificates
  here only satisfy QUIC's requirement that TLS exist.
- **Deduplication figures are corpus-specific** and say nothing about anyone
  else's files.
