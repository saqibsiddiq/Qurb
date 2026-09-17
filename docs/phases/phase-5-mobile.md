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
| files without holding them in memory | ✅ on the phone: 512 MiB → 6 MiB |
| filenames that survive macOS and iOS | ✅ NFC normalisation |
| **running on an actual phone** | ✅ **426 tests on a Galaxy S23, ARM64** |
| pairing and syncing from the phone | ✅ end to end, over QUIC |
| a sync that fits a background window | ✅ `sync_within(seconds)` |
| the key kept outside the app's files | ✅ Android Keystore, verified on a device |
| **an Android app** | ✅ [`android/`](../../android/) — installs, sets up, syncs |
| background scheduling in the app | ⬜ `syncWithin` exists; nothing schedules it |
| a FileProvider | ⬜ the files are invisible to the rest of the phone |
| iOS, at all | ⬜ blocked: needs Xcode, which needs a Mac |

433 tests pass across ten crates on Linux, 426 of them on a Galaxy S23;
clippy is clean.

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

And on the phone itself — a file arriving over a real QUIC connection from
another device rather than read from a local store:

| file received over the network | heap (Galaxy S23) | heap (emulator) |
|---|---|---|
| 32 MiB | 8 MiB | 4 MiB |
| 128 MiB | 5 MiB | 4 MiB |
| 512 MiB | 6 MiB | 5 MiB |

**A half-gigabyte file costs 6 MiB of heap on a real phone**, and the number does
not move as the file grows sixteen-fold. That is what the FileProvider concern
was always about, measured on hardware rather than argued from a desktop.

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

## Running it on Android

`./scripts/android-test.sh` builds the test binaries for an Android target,
pushes them with `adb`, and runs them. There is no app, no Gradle and no JVM
involved: the FFI is a C ABI, and a test binary exercises the same Rust an app
would reach through it.

**On a real phone**: a Samsung Galaxy S23 (SM-S911B, Snapdragon 8 Gen 2,
Android 16, API 36, arm64-v8a). All 35 binaries pass — 426 tests in 104 seconds.

Also on an Android 14 (API 34) x86_64 emulator, where the same 35 binaries pass
in 74 seconds. The emulator is worth keeping for the fast loop; the phone is
what the claim rests on. The networking crates are included deliberately: they open real
sockets, complete a real QUIC handshake and punch through to each other on
loopback. Excluding them would have meant the answer to "does it work on
Android?" quietly left out the interesting half.

Two things failed on the first attempt, and both were the harness rather than
the code:

- The crash tests spawn `crash_writer` and kill it with a real `SIGKILL`. The
  script pushed test binaries and not examples, so all five failed looking for
  a helper that was not on the device.
- The script piped each binary's output into `grep -q`, which exits on its
  first match and closes the pipe. Most of the results were lost *and* the run
  still reported a pass — the worse of the two failure modes, since a suite
  that reports green while showing nothing is indistinguishable from one that
  works.

### Why ARM mattered, and what it showed

Running on ARM was worth insisting on. Architecture-independent Rust is
architecture-independent, but two things here are not: BLAKE3 takes a NEON code
path on ARM rather than AVX2, and ARM's memory model is weaker than x86's, so a
concurrency bug that x86 hides can surface there. This workspace has real
concurrency — a garbage collector running against a live writer, a worker pool
holding a database connection each — and until now none of it had run anywhere
but x86.

It found nothing. Every test passes on ARM64 exactly as on x86_64, including the
crash-injection suite, the property tests with their 320 random seeds, and the
concurrent-collector test. That is a negative result and worth recording as one:
it does not prove the code is free of memory-ordering bugs, it proves that this
suite on this device did not expose any.

A note for anyone repeating this: the emulator cannot substitute. Emulator 37.x
refuses an `arm64-v8a` image on an x86_64 host outright —

    Avd's CPU Architecture 'arm64' is not supported by the QEMU2 emulator
    on x86_64 host. System image must match the host architecture.

Cross-architecture emulation is gone, so ARM verification needs ARM hardware:
a phone, a Raspberry Pi, an ARM CI runner, or an Apple Silicon Mac.

### What this still does not establish

**The phone was plugged in, awake and unthrottled.** Tests ran from
`/data/local/tmp` as a shell user, not as an installed app in the background.
Nothing here measures battery cost, what survives a suspend, what happens under
memory pressure from other apps, or how the platform treats a process it has
stopped caring about. Those need an app.

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

[`crates/mobile-ffi`](../../crates/mobile-ffi/) — no sync logic of its own.
Logic behind an FFI boundary is logic the rest of the workspace cannot test, so
there is none there: everything delegates to `qurb-engine` and `qurb-peer`.

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

### Syncing, and what it turned up

The phone pairs out of band, finds the other device through the rendezvous
service, connects over QUIC and syncs — all through the FFI, with a test that
checks the file arrives byte-exact rather than that the calls returned `Ok`.

The mobile-shaped entry point is `sync_within(seconds)`. Both platforms grant
background work a window and kill anything that outstays one, so "sync until
finished" is how an app loses its background privileges. Running out of time is
reported, not raised: each file is committed as it lands, so a pass that stops
early leaves work done rather than work lost. A connector is built per pass
rather than kept, because a phone's address changes every time it moves between
Wi-Fi and cellular.

Two defects fell out of this, both outside the new code and both invisible until
something used the library the way a phone does.

**`Connector::start` announced `0.0.0.0`.** It passed `socket.local_addr()`
straight into the endpoints it advertised, and a socket bound to every interface
reports `0.0.0.0:port` — true about the socket, useless to a peer, which has no
"here" to resolve it against. The identical mistake was found and fixed in
pairing invites earlier; it survived here because every existing test binds
`127.0.0.1` explicitly, and in ordinary use STUN supplies a public address that
works instead. What it broke is the case with no STUN: two devices on a network
with no route to the internet, which is exactly when the local address is the
only one there is.

**`NetworkSource` was not streaming.** It implemented only the buffering half of
`ContentSource` and inherited the trait's default for the streaming half — and
that default buffers. So the network path, the one a phone uses to receive a
large file, held the whole file in memory however carefully the layers beneath
it streamed. Nothing failed, because buffering is correct and merely expensive.

That second one is worth dwelling on. [Decision
0018](../decisions/0018-file-contents-never-cross-the-ffi.md) names this exact
hazard — "a new source that forgets to override gets correctness and loses the
ceiling" — and it was then made by the same hand that wrote the warning, in the
same week. A default implementation that is correct and slow is a default that
nothing will ever catch. The answer was not a better comment but a test that
measures.

**STUN discovery is now a setting.** It had been unconditional, which meant the
new tests would contact Google's and Cloudflare's STUN servers on every `cargo
test` — slow, broken offline, and telling a third party the address of every
machine that runs the suite.

## Keeping the key off the disk

The desktop can put the master key in Keychain, DPAPI or the Secret Service
through one crate. A phone cannot: Android's keystore is a Java API needing a
`Context`, and iOS's needs entitlements that belong to an app bundle. Both are a
few lines from the platform side and unreachable from Rust.

So the app supplies it. `qurb-keys` gains a `SecretStore` trait and a
`Protection::Platform` that uses it; `qurb-mobile` exposes that as a UniFFI
callback interface, which generates a Kotlin `interface KeyStore` and a Swift
`protocol KeyStore`. Two traits rather than one because `qurb-keys` is used by
the daemon, the CLI and the tests, none of which should know that a phone
exists.

What is stored is the key itself, 32 bytes, rather than something wrapping it.
Both platforms handle small secrets well, and a wrapping layer would mean
running Argon2id at every app launch to derive a key from a value that is
already full entropy — a second of phone CPU for nothing.

Tested against a fake keystore, which checks the contract rather than the
platforms: that the key reaches the store and *not* the vault file, that it
comes back on a second open, that two stores on one device get separate slots,
that a keystore which refuses produces an error rather than a panic across the
FFI boundary, and that opening without the keystore says what is missing rather
than looking like corruption.

Adding it broke a test, correctly. `a_newer_format_is_refused_rather_than_misread`
wrote format byte `FORMAT_PASSPHRASE + 1` to check that a file from the future is
refused rather than guessed at — and `FORMAT_PLATFORM` then took that number, so
the "future" format became a real one. The test now anchors to the last format
rather than to a particular one.

**The platform implementations do not exist.** The contract is built and tested
and nothing fills it in. `crates/mobile-ffi/README.md` has a starting point for
each platform; neither has been compiled.

## The app

[`android/`](../../android/) — Kotlin, classic Views, about 700 lines. It
installs, sets up an identity, keeps the key in the Android Keystore, lists
files, pairs with another device, and syncs. The APK is 21 MB, of which roughly
7 MB is the engine.

`scripts/android-app.sh` builds the native libraries, strips them, copies them
into `jniLibs`, regenerates the Kotlin bindings, and then runs Gradle. Cargo is
deliberately not wired into Gradle: a Rust change should be an explicit step
rather than something that happens invisibly inside an IDE, and the app then
builds from a clean checkout on a machine with no NDK.

### The bug only an app could find

`Qurb` had its methods in two `#[uniffi::export] impl` blocks. **UniFFI keeps
only the last one and silently discards the rest.** The Rust compiled. The
bindings generated without a warning. Eight methods — `scan`, `list`, `export`,
`importFile`, `remove`, `usage`, `contains`, `root` — were simply absent from
the Kotlin and Swift.

Every Rust test kept passing, because they call these functions directly rather
than through the generated bindings. The binding-generation step "succeeded".
The first sign anything was wrong was the Kotlin compiler saying `Unresolved
reference 'scan'`.

The cause was self-inflicted: an earlier edit split the impl block to move a
private helper out and put the closing brace in the wrong place, so every
instance method fell into the non-exported block.

`crates/mobile-ffi/tests/bindings.rs` now reads the generated text and checks
every method an app needs, in both languages. It also refuses to run against a
library older than `src/lib.rs` — because its own first run passed against a
stale one, which would have been a false pass exactly as easily as a false
failure.

The general lesson is the same one [decision
0018](../decisions/0018-file-contents-never-cross-the-ffi.md) already recorded
about a buffering default: **a failure that produces working-looking output
cannot be caught by review or by a compiler.** It has to be caught by something
that inspects the artefact.

### The keystore, on hardware

[Decision 0021](../decisions/0021-the-platform-supplies-the-keystore.md) was
tested against a fake. It now has a real implementation, and the result is
visible from outside the app. After setup, the vault file is five bytes:

```
$ adb shell run-as com.qurb od -An -c files/qurb/.qurb/master.key
   Q   R   B   K 004
```

Magic, then `FORMAT_PLATFORM`. The key is nowhere in the app's files: the
ciphertext and its IV sit in SharedPreferences and the AES key that opens them
lives in the Keystore, where the app cannot read it. Force-stopping and
relaunching reopens the store, so the round trip works and not merely the write.

## What is not verified

The honest state of this phase.

**No real phone.** Everything on-device ran on an x86_64 emulator. The ARM64
library that would ship is compiled and never executed, and an emulator imposes
none of a phone's memory pressure, thermal limits or battery behaviour. It also
never suspends the process the way a backgrounded app is suspended, which is the
single most important thing about running on a phone.

**iOS has not been built at all.** It needs Xcode, which needs a Mac. The
`staticlib` crate type is declared and the Swift bindings generate, so the
preparation is done; the build is not. Nothing about iOS in this document is
measured — the memory work was done *because* of the FileProvider ceiling, and
whether it clears that ceiling in practice is unknown.

**No iOS app, and the Swift has never been compiled.** The Kotlin bindings are
now compiled and run by a real app; the Swift ones are generated and checked as
text by a test, and nothing has put them through a Swift toolchain.

**Background scheduling is half-built.** `sync_within(seconds)` is the Rust side
and it works, and the app calls it from a button. Nothing *schedules* it:
`WorkManager` on Android, `BGTaskScheduler` on iOS, and the policy about when to
ask for a window at all, do not exist.

**The app has never run unattended.** Syncing happens when someone presses Sync
with the app in front of them. What the platform does to it in the background —
and what that costs in battery — is unmeasured.

## Kill criterion

From the roadmap: *sync that works on a phone without destroying the battery, or
without the platform killing it.*

**Still open, and now open for a narrower reason.** The half about the platform
killing it is partly answered: `sync_within` exists precisely so a pass ends
when the window does, and a pass that runs out of time reports it rather than
failing. What is unmeasured is everything about a real device — battery cost,
what happens across a suspend, and whether a FileProvider extension survives its
ceiling. That needs hardware and an app.

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
  question, not a coding one, and it has not been answered. The machinery is
  half there — a storage-only replica can already hold a subset via
  `PinSet::under` — but deciding *what* a phone keeps, and what happens when a
  user opens something it does not have, is untouched.
- **Two phones that are both asleep.** Sync needs both devices awake and
  announced at the same moment, because a QUIC handshake's opening packets are
  the hole punch. Two desktops manage this by being on all the time. Two phones,
  each awake for a few seconds a day at the platform's discretion, may simply
  never meet. This is the strongest argument for a storage-only replica in the
  picture, and it is the reason the roadmap plans iOS as a good viewer rather
  than a peer equal to a desktop.
- **Background scheduling.** `sync_within` is the Rust half and it works. The
  platform half — `WorkManager`, `BGTaskScheduler`, and the policy about when
  to ask for a window at all — is platform-side work that needs an app to live
  in.
- **Upgrading a relayed connection back to direct.** Inherited from Phase 3 and
  worse on a phone, which changes network several times a day: a connection that
  fell back to the relay while on cellular stays relayed after it reaches Wi-Fi.
  Since a per-pass connector is built fresh each time, the next pass does get a
  fresh chance — so on mobile this is less severe than on the desktop daemon,
  which holds one connector for hours.
- **Conflict resolution on a small screen.** The engine never discards an edit,
  so conflicts appear as extra files. On a desktop that is tolerable. On a phone
  it is confusing, and nothing has been designed for it.
