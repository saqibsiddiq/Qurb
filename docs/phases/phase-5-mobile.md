# Phase 5 — Mobile

**Status:** in progress
**Target:** months 11–14

Getting the engine onto a phone. The roadmap recommended cutting this from year
one; it was kept deliberately, and the recommendation is [still on
record](../roadmap.md).

## Progress

| area | status |
|---|---|
| the core cross-compiles for Android | ✅ all four architectures |
| an FFI a phone can call | ✅ [`qurb-mobile`](../../crates/mobile-ffi/) |
| Kotlin and Swift bindings generate | ✅ `./scripts/mobile-bindings.sh` |
| files without holding them in memory | ✅ measured, 1024 MiB → 1 MiB |
| filenames that survive macOS and iOS | ✅ NFC normalisation |
| running on an actual device | ⬜ **not done — no device or emulator here** |
| iOS build | ⬜ blocked: needs Xcode, which needs a Mac |
| networking from the phone | ⬜ compiles, not exposed |
| background sync | ⬜ not started |
| Keychain and Android Keystore | ⬜ not started |
| an app | ⬜ not started |

420 tests pass across ten crates; clippy is clean.

## What the library costs

Measured on this machine, release build, stripped with the NDK's `llvm-strip`:

| architecture | unstripped | stripped |
|---|---|---|
| `aarch64-linux-android` | 61 MB | **3.7 MB** |
| `armv7-linux-androideabi` | 46 MB | **3.2 MB** |
| `x86_64-linux-android` | 60 MB | **3.9 MB** |
| `i686-linux-android` | 48 MB | **3.9 MB** |

Stripped is the number that matters: an APK ships one architecture per device
through an app bundle, so this is roughly what the engine adds to a download.
SQLite, Zstd, BLAKE3, XChaCha20-Poly1305, QUIC and the FFI scaffolding are all
inside it. The unstripped figures are debug information, which Android does not
need at runtime.

## The memory ceiling

The largest correctness problem mobile exposed, and it had nothing to do with
mobile code.

An iOS FileProvider extension — the thing that makes files appear in the Files
app — runs under a memory limit in the tens of megabytes. The receive path held
whole files: fetch into a `Vec`, write it to disk, hand the same buffer to
`adopt`. On a desktop that is merely wasteful. Inside an extension it is fatal,
and fatal in the worst way: it works for small files and kills the process for
large ones, so it looks like an intermittent bug rather than a design error.

Measured on this machine (Linux 7.2.2, CachyOS, warm page cache, release build),
as the growth in `RssAnon` — anonymous resident memory — across adopting one
file:

| file | before | after |
|---|---|---|
| 256 MiB | 256 MiB of heap | 0 MiB |
| 1024 MiB | 1024 MiB of heap | 1 MiB |

`RssAnon` rather than total resident size, on purpose. A memory-limited platform
counts *dirty* pages against a process; pages backed by a file on disk are clean
and the kernel can drop them under pressure. Measuring the total counts both
alike, which makes a memory-mapped read look exactly as dangerous as a 1 GiB
`Vec` — the opposite of true. Four earlier versions of this measurement were
wrong, the last of them because the buffer was dropped before the reading was
taken, which made holding a whole file in memory appear to cost nothing.

Both shapes are kept so the comparison stays checkable:

```bash
cargo run --release -p qurb-engine --example peak_memory -- buffered 1024
cargo run --release -p qurb-engine --example peak_memory -- stream 1024
```

The streaming path also changed where files land. Content cannot be verified
until its last byte arrives, so a file is now assembled under a staging name
beside its destination and moved into place once the hash checks out. Two things
follow: unverified bytes are never visible at the real path, and a transfer
interrupted halfway leaves no truncated file that looks complete. The rename is
within one directory, so it is atomic.

That required the watcher to learn the staging name. Without it the half-written
file would be indexed as a real one, its rename read as a deletion, and both the
phantom and its removal sent to every other device. A test in `ignore.rs` ties
the two crates together, since nothing else does.

## Filenames that mean the same thing

`é` can be one code point (U+00E9) or two (`e` + U+0301). They render
identically and they are different strings.

Filesystems disagree about which to keep. Linux and Windows store the bytes they
are given. macOS and iOS decompose on the way in, so a file created as NFC is
read back as NFD.

Left alone, that disagreement duplicates files without limit. Sync `café` from
Linux to a Mac: the Mac writes the composed name, the filesystem stores the
decomposed one, the Mac's next scan finds a path it has no record of, reports
the composed one deleted and the decomposed one created, and sends both back.
Linux, which keeps the two apart, now has two files. Then it happens again.

Paths are now normalised to NFC at the single seam where a disk path becomes a
logical path — the same place the separator is already normalised, and for the
same reason. NFC is the canonical form because it is what the web and most
sources of filenames already produce, so on Linux and Windows the change costs a
scan and alters nothing.

Where both spellings exist at once, which only a non-decomposing filesystem
allows, one is indexed and the rest are reported. Renaming a user's file is not
a decision to make on their behalf.

The first version of that resolution was wrong, and the test caught it. Both
spellings normalise to the *same* logical path, so sorting by logical path left
the tie to directory-read order — which differs between machines. Two devices
resolving the same collision could keep different files under one name. The rule
now depends only on the filenames: the spelling already in normalised form wins,
and byte order breaks anything left.

## The FFI

[`crates/mobile-ffi`](../../crates/mobile-ffi/) — about 350 lines, no sync logic
of its own. Logic behind an FFI boundary is logic the rest of the workspace
cannot test, so there is none there.

UniFFI generates both languages from the Rust, including the doc comments, which
means the Kotlin and Swift carry the same explanations as the source rather than
a separate set that drifts.

Two bugs came out of writing tests for it, both of the kind that only appears
when you use an API rather than design it:

- **`import_file` could destroy the file it was adding.** An app handed it a
  source already inside the synced tree — an entirely reasonable call — and it
  became `std::fs::copy` from a path onto itself, which truncates. Now the two
  are compared after canonicalisation and an in-place file is indexed where it
  lies.
- **`usage` reported the wrong "logical" number.** It summed chunks, which are
  deduplicated, so three copies of a photo were reported as taking up one
  photo's worth of space. True of the disk, false of the library, and precisely
  backwards for a screen whose job is to show what deduplication saved.

## What is not verified

The honest state of this phase.

**Nothing here has run on a phone.** The Android libraries build for all four
architectures and `libqurb_mobile.so` exports the expected 27 UniFFI symbols,
which is real evidence that the toolchain and the FFI scaffolding are right. It
is not evidence that the code runs under Android's libc, its filesystem
semantics, its background-execution rules, or its memory limits. This machine
has no device connected, no emulator installed, and no `qemu-user` to run the
binaries under. The gap closes with an emulator or a phone and not before.

**iOS has not been built at all.** It needs Xcode, which needs a Mac. The
`staticlib` crate type is declared and the Swift bindings generate, so the
preparation is done; the build is not.

**The phone cannot sync.** `qurb-peer` cross-compiles, but nothing in the FFI
exposes pairing or transfer. Today this crate makes the phone a local encrypted
file store, which is a real thing and not the thing the project is for.

## Kill criterion

From the roadmap: *sync that works on a phone without destroying the battery, or
without the platform killing it.* Unmeasurable here for the same reason as the
rest — no device. It stays open.

Phase 3's kill criterion (≥70% direct connections) also remains open, waiting on
a second machine on a different network. See
[measuring-connectivity.md](../measuring-connectivity.md).

## Deliberately left undone

- **Keychain and Android Keystore.** The `keyring` crate the desktop uses does
  not cover mobile. On a phone the vault is an owner-only file in app-private
  storage, which the kernel enforces and the device's lock screen encrypts —
  weaker than a keystore, stronger than the same arrangement on a desktop, and
  worth nothing on a phone with no passcode. A passphrase works today.
- **Selective sync.** A phone cannot hold a desktop's library, so it will need
  to choose what to keep locally and fetch the rest on demand. That is a design
  question, not a coding one, and it has not been answered.
- **Background sync.** Both platforms schedule background work on their own
  terms and kill anything that outstays its welcome. Platform-side work.
- **Conflict resolution on a small screen.** The engine never discards an edit,
  so conflicts appear as extra files. On a desktop that is tolerable. On a phone
  it is confusing, and nothing has been designed for it.
