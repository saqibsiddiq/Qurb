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
| syncing unattended | ✅ WorkManager, every 15 minutes — hourly once push has worked in the last week |
| the files visible to other apps | ✅ a DocumentsProvider, verified in Files |
| sharing into qurb from any app | ✅ a share target, works with no network |
| knowing what has not been delivered | ✅ asked of the index, not kept as a queue |
| being woken by another device | ✅ push, when configured — 0.7s on hardware |
| **a phone syncing with a laptop, both ways** | ✅ verified on hardware |
| syncing from mobile data | ✅ directly, through a rendezvous service on the laptop |
| recently deleted, and settling a conflict | ✅ verified between the S23 and the laptop |
| files by folder, search, rename, move | ✅ on the emulator; ◻ not yet on the phone |
| showing a pairing code on the phone | ✅ on the emulator; ◻ not yet paired that way |
| the share sheet sending to one device | ◻ built, not yet tried |
| which devices have each folder, and keeping one only remotely | ✅ on the emulator; ◻ not yet between real devices |
| removing a device | ✅ on the S23 on 2026-09-29, from its history: the laptop removed, then paired again |
| designed to the owner's direction: four tabs, Private Vault inside Files | ✅ on the S23, every place walked, 2026-10-03 ([below](#the-designed-app-on-the-s23)) |
| adding a file into the area on screen | 🧪 in the FFI's tests ([0049](../decisions/0049-adding-a-file-puts-it-where-you-are-looking.md)) |
| a large file collected from the phone | ◻ fetched eight chunks at a time, resumed, served in the foreground — built and tested, not yet measured on the phone ([below](#an-800-mb-video-and-what-stopped-it)) |
| iOS, at all | ⬜ blocked: needs Xcode, which needs a Mac |

765 tests pass in 89 test binaries on Linux (2026-10-05, debug build, the
development laptop); the last run on a Galaxy S23 was 426 of them, on
2026-09-17 — the suite has grown since and has not been run there again.
Clippy is clean.

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

### Syncing without being asked

`SyncWorker` is the half of [decision
0020](../decisions/0020-sync-takes-a-deadline.md) that Rust cannot do.
`sync_within(seconds)` makes a sync that ends when its window does; the worker
decides when to ask for a window and what to tell the system afterwards.

WorkManager rather than an alarm or a foreground service: an alarm survives
neither Doze nor a reboot, and a foreground service means a permanent
notification plus, since Android 14, a declared type that "syncing files" does
not cleanly fit.

The outcomes are mapped deliberately, and one of them was mapped wrongly for a
release. Running out of time is a *retry*, so another window comes sooner than
the next period. A locked keystore is a failure, because retrying in fifteen
minutes reaches the same conclusion.

**Nothing answering used to be a retry too**, on the reasoning that the other
device being asleep is ordinary and the backoff would keep it from draining the
battery. That was exactly backwards. Every unanswered attempt doubled the
delay, so a phone whose laptop had been off scheduled its next attempt three
hours out — and the moment the laptop came back was the moment the phone had
stopped looking. Caught on hardware, not on paper: a file shared at 18:01 was
still sitting on the phone at 18:06 with the laptop running beside it and
`jobscheduler` reporting `earliest=+3h5m58s`.

Backoff is for transient errors. "Nobody is awake yet" is the steady state, and
the answer to it is the ordinary fifteen-minute period, which only applies if
the worker reports success — which also resets the attempt count, so an
accumulated backoff unwinds by itself.

Fifteen minutes is not a choice: it is the shortest period WorkManager accepts,
and asking for less silently becomes fifteen anyway. In practice it is a floor
rather than a promise, since Doze batches background work and an idle phone may
go hours between runs. The app says so rather than implying otherwise.

The first version was silent, which for background work is a defect rather than
an omission: nobody is watching when it runs, so with no trace there is no way
to tell a sync that works from one that has quietly stopped — and quietly
stopping is the failure mode a sync app actually dies of. It now logs and
records its last result.

### Sharing when nothing is listening

The app is a share target: `ACTION_SEND` and `ACTION_SEND_MULTIPLE` for any
type, so anything on the phone can be sent into qurb from the system share
sheet. The point of the screen is what it does *not* need. Saving a photo
writes it into the folder and indexes it here and now, with every other device
switched off and no network in sight; reaching them is a separate question,
answered whenever one is next awake.

There is **no outbox and no retry queue**, because nothing is queued. An outbox
would be a second record of what needs to happen, and two records of one fact
disagree eventually — an entry for a file since deleted retries forever, a file
the outbox forgot is never sent and reports no error. Instead the index is
asked each time: *live files this device made, whose content no other device is
known to hold*. A file joins that answer when it is shared and leaves it when
somebody takes delivery, with nothing to enqueue or clean up.

For that to be answerable at all, the protocol gained its first message that
asks for nothing: `Got { content }`, sent by a device that has just finished
receiving content, to the device it came from. Before it, a device being
fetched *from* learned nothing — it served some chunks, and whether they added
up to a file someone wrote was not its business. That is fine for syncing and
useless for telling a person their photo arrived. It is credited to the
certificate the connection authenticated with, never to anything the message
says, because [decision 0025](../decisions/0025-a-storage-cap-that-cannot-lose-data.md)
lets a storage cap drop a local copy on exactly this record.

Measured, two devices through a rendezvous service on loopback: a file shared
with the desktop **not running at all** was saved, correctly reported as held
only by the phone, survived a sync attempt that reached nobody, and arrived
byte-for-byte once the desktop came up — with nothing further done on the
phone. The phone then reported nothing outstanding.

One thing worth fixing that was found writing this: importing over a name
already in use records a *new version* of that file, and since content is
stored once, the old version's bytes go with it. Two apps both producing
`IMG_0001.jpg` is not unusual, so a share now takes the next free name instead.
Re-sharing the same photo leaves two copies, which a person can delete; the
other way round costs them a file they never touched.

See [decision 0026](../decisions/0026-sharing-while-the-other-device-is-off.md).

### The files, from the rest of the phone

`QurbDocumentsProvider` puts qurb in the system file picker and the Files app.
Without it the synced directory is private to the app, which makes a file sync
product no other app can open.

Verified on a device, from DocumentsUI's own log:

```
Matched roots: [... Root{authority=com.qurb.documents, rootId=qurb,
title=qurb} @ content://com.qurb.documents/root/qurb]
```

Browsing it shows folders and files with sizes and dates, and the breadcrumb
works into nested folders. Querying the provider from a shell is refused with a
`SecurityException`, which is correct — only the system's DocumentsUI may open
it.

Read-only. `openDocument` accepts mode `r` and nothing else, because supporting
writes means deciding what a partial write means to a sync engine mid-transfer.
`deleteDocument` and `renameDocument` are absent for a sharper reason: a
deletion here becomes a tombstone that propagates to every device, and letting a
file manager do that by accident is not a risk worth taking before there is any
undo.

## Putting a phone and a laptop together

The app existed, the engine ran on hardware, and both had been tested against
themselves. Putting a real phone and a real laptop on one network found five
more things — none of which any test had caught, because each needed two
machines, a router, and a person trying to use them.

### `qurb pair` waited forever on a dead invite

Found on a terminal that had been sitting for two and a half hours, still
printing `Waiting...` under a code that had stopped working five minutes in.
Worse than failing, because the person reading the code aloud has no way to know
it is dead.

Two causes, and the first is the interesting one. `wait` took `now` as a
parameter and checked `is_expired(now)` *inside* the accept loop — so the clock
it compared against was captured before the wait began, and the check could
never fire however long the wait lasted. It was dead code that looked like a
safeguard. Nothing else bounded the loop, so with nobody connecting it blocked
until the process was killed.

The wait is now bounded by the invite's remaining lifetime, computed from the
caller's clock so it stays injectable for tests. Both new tests *hang* against
the old code rather than failing, which is the shape of the bug.

### 16 KB page alignment

Android 15 introduced devices with 16 KB memory pages, and a library whose LOAD
segments are aligned to the old 4 KB will not load on one **at all**. Rust
defaults to 4 KB. The app installed, warned on the Galaxy S23, and would have
failed outright on a 16 KB device.

`-Wl,-z,max-page-size=16384` in both Android scripts; the segments went from
`0x1000` to `0x4000` and `zipalign -c -P 16` verifies the APK. JNA was already
at `0x10000`, which satisfies the requirement — a larger alignment is a valid
one.

Only a real device surfaced this. The emulator never complained.

### The toolbar was under the status bar

Android 15 draws apps edge to edge whether they ask or not, and the app never
handled window insets. The toolbar sat beneath the status bar: it looked wrong,
and the overflow button's top half was unreachable because taps in that strip go
to the status bar instead.

Invisible in a screenshot until you try to press something — which is how it was
found, after several attempts at opening a menu that would not open. The first
fix padded the toolbar, which pushes its contents down inside a box that does not
grow, so the title clipped instead. The padding belongs on the `AppBarLayout`.

### Errors that read like a struct dump

UniFFI generates `message = "detail=${detail}"`, so every dialog led with a field
name: `detail=connection lost: timed out`. `QurbException.readable()` replaces
it, and pairs each case with the first thing worth trying — a timeout on a home
network almost always means a firewall dropping UDP, and a rule that allows ping
will still drop it, so the message says so. That was the actual cause here.

### A hole punch that cried wolf

The one that cost the most time, and the lesson is about logging rather than
networking.

`knock` opens a handshake it *wants* to fail: the packets are the entire point,
and it pins its own fingerprint so nothing can complete. Every punch therefore
logged `WARN rejected an unrecognised peer` — which reads exactly like a device
being refused for real.

It sent an investigation through the peers table, the certificate on the phone,
and a BLAKE3 hash of that certificate to prove it matched what the laptop
trusted, while the actual failure sat two lines above at `DEBUG`. The rejection
is now `TRACE` when it is expected, and the genuine warning carries the full
fingerprints on both sides — the four-byte short form cannot show a mismatch
that starts later, which is precisely the case the message needs to prove.

**A log line's severity is an assertion.** A `WARN` that fires on a successful
code path trains people to ignore warnings, and costs more than the silence
would have.

### And the one that made sync one-directional

See [decision 0022](../decisions/0022-the-service-announces-arrivals.md). The
rendezvous service knew the moment a device appeared and told only that device,
so a phone awake for twenty-odd seconds could never be found by a laptop polling
on a two-minute backoff. Sync worked one way and appeared symmetrical in design.

Measured: a laptop retrying every 120s against a phone announcing for 25s never
once caught it. After the fix, `peer appeared; syncing now` within a second, and
a 4.7 MB photo crossed inside the window, byte-identical by SHA-256.

## What is not verified

The honest state of this phase.

**iOS, entirely.** It needs Xcode, which needs a Mac. The `staticlib` crate type
is declared, the Swift bindings generate, and a test checks their text — but
nothing has put them through a Swift toolchain, there is no app, and no iOS
device has run any of this. Nothing about iOS in this document is measured. The
memory work was done *because* of the FileProvider ceiling; whether it clears
that ceiling in practice is unknown.

**Background sync has never been observed happening on its own.** The worker is
scheduled and was verified by running one through WorkManager rather than by
calling the engine directly — so the path is the same one the schedule uses.
What has not been watched is a phone left alone for a day, syncing on Android's
timetable. That takes a day and a phone nobody is using.

**Battery is unmeasured.** The whole design of `sync_within` is about not
spending more of a window than granted, and nothing has measured what a sync
actually costs in power.

**One phone, one laptop, one network.** Everything verified on hardware was
verified on a single Galaxy S23 and a single Linux laptop on one home Wi-Fi
network, with the rendezvous service running on that laptop. Nothing has been
tried across networks, through carrier-grade NAT, over cellular, or against a
hosted service — which is also why Phase 3's kill criterion is still open.

**The sync was hand-driven.** A person pressed Sync, or a script did. The
end-to-end path of "change a file and have it appear elsewhere without anyone
asking" has been seen once, on the laptop's arrival-triggered sweep, not run for
long enough to call reliable.

## Kill criterion

From the roadmap: *sync that works on a phone without destroying the battery, or
without the platform killing it.*

**Still open, and now open for a much narrower reason.** There is a real phone
running a real app that syncs with a laptop in both directions, and
`sync_within` exists precisely so a pass ends when its window does. What remains
unmeasured is *cost over time*: battery across a day, what Android actually
grants the worker rather than what it was asked for, and what happens across a
suspend. Closing it needs a phone left alone for a day, not more code.

The iOS half of the criterion — whether a FileProvider extension survives its
memory ceiling — is untouched, and the memory work that exists was done on its
behalf without ever being tested against it.

Phase 3's kill criterion (≥70% direct connections) also remains open. Today's
work was two devices on *one* home network, which says nothing about the rate
across different ones — and a phone on cellular, behind carrier-grade NAT, is
exactly the hard case that number is about. See
[measuring-connectivity.md](../measuring-connectivity.md).

## Being woken, rather than looking

A phone cannot hold a socket open in the background, so the device most in need
of being told something is exactly the one that cannot be told. Telling a peer
there is work for it — the Phase 4 change — only reaches a phone that happens
to be connected.

A push notification is the way through, and on both mobile platforms it is the
only way through. With Firebase configured, the rendezvous service pokes the
phone the moment another device has something.

Measured on a Galaxy S23, asleep with its screen off:

```
13:22:18.579412  laptop   local changes stored=1
13:22:18.579876  service  waking a device that is not connected
13:22:19.281     phone    woken by another device
13:22:35         phone    sync: reached=1 adopted=1
```

**Seven hundred milliseconds** from the change to the phone waking. The file
arrived complete and was acknowledged.

The poke carries nothing: no filenames, no sizes, not even which peer. What
Google learns is that a device was poked and when, which is metadata this
cannot avoid and [decision 0028](../decisions/0028-waking-a-sleeping-device.md)
does not pretend to.

It is optional in a way that is more than nominal. The Android build switches
on the presence of a `google-services.json`, through a separate source set, so
a checkout without a Firebase project does not link the SDK at all and behaves
exactly as before.

### The protocol bump, on hardware

**2026-09-24, Galaxy S23 (SM-S911B) against the laptop, same Wi-Fi.**

Tree entries gained a private flag when vaults arrived, and the wire protocol
went from `qurb/0` to `qurb/1` — a change that makes an older build refuse to
connect rather than mishandle content sent to somebody's vault. That refusal is
the correct behaviour and it means every device has to be rebuilt together, so
it is worth recording that both halves were actually rebuilt and actually met.

What was done: the engine cross-compiled and the debug APK rebuilt and
installed; the laptop daemon restarted, because the running one had been
started from a binary since replaced and was still speaking `qurb/0`. The two
then synced — `after-restart.bin`, which the laptop had been holding as the
only copy, reached the phone, and both ended on 18 files.

```
a peer is reachable and has news; syncing now  peer=7a4ebf0c
connected  peer=7a4ebf0c  candidate=192.168.1.2:46931
```

A connection at all is the evidence: ALPN is negotiated during the TLS
handshake, so two builds that disagreed about the protocol would never have got
as far as being connected.

Two things this did *not* establish, both worth saying:

- **It was tested over the local network, not through the tailnet.** The
  phone's Tailscale was offline, so its configured rendezvous
  (`wss://<host>.ts.net`) would not resolve — the failure was
  "No address associated with hostname", which is a DNS problem and not a
  protocol one. It was pointed at the laptop's LAN address for the test and put
  back afterwards.
- **The ALPN string cannot be confirmed by inspecting a binary.** A six-byte
  literal is materialised as immediates rather than stored as bytes, so it
  appears in neither the `.so` nor the desktop binary. Watching the two connect
  is the check.

### Local discovery: working, after three wrong guesses

**2026-09-24, Galaxy S23 (SM-S911B) and the laptop, same Wi-Fi, no rendezvous
service running anywhere.** Both directions, sixty-eight milliseconds from the
phone's sync starting to a connection:

```
no rendezvous service; devices on this network can still find each other
listening for beacons  interfaces=[127.0.0.1, 192.168.1.2]
a device is on this network  peer=7b543b5d
connected  peer=410cac55  candidate=192.168.1.4:36589
reached on the local network  peer=410cac55
```

Both devices ended on twenty files, and the two files that had been stranded on
the laptop arrived. Nothing else was running: no rendezvous, no relay, no
overlay network.

**What actually fixed it was the third guess, and the first two were not
wasted.** In order:

1. **The multicast lock.** Android drops multicast before it reaches an
   application unless one is held. Necessary — it is held for the length of a
   sync now — and on its own it changed nothing.
2. **The interface the group is joined on.** `if_addrs` was suspected of
   returning nothing under Android's NETLINK restrictions. It does not: the log
   above shows it enumerating `127.0.0.1` and `192.168.1.2` perfectly. The
   fallback that asks the kernel which interface it would use for the group is
   still there and still right for platforms where enumeration is restricted.
3. **Asking, and waiting for the answer.** This was it. A phone builds a fresh
   connector for every sync pass, so its address book is always empty at the
   moment it wants to reach somebody, and the answers to its own arrival probe
   were arriving a few hundred milliseconds *after* it had given up. `reach`
   now probes and waits up to a second before falling through to the rendezvous.

The diagnosis only became possible once the engine could speak on Android at
all — `tracing-subscriber` had been a dev-dependency, so every log line in the
whole engine was being discarded. Two of the three guesses above were made
blind, from the laptop's side, and cost far more than the fix did.

### Local discovery: the phone sends and does not receive

**2026-09-24, Galaxy S23 (SM-S911B) and the laptop, same Wi-Fi, no rendezvous
service running anywhere.**

Beacons ([decision 0034](../decisions/0034-finding-each-other-with-no-server.md))
work in one direction on this phone and not the other.

The laptop hears the phone every time. Its log, with nothing listening on the
rendezvous port:

```
no rendezvous service; devices on this network can still find each other
a device is on this network  peer=022722a4  news=false
a peer is reachable and has news; syncing now  peer=7a4ebf0c
connected  peer=7a4ebf0c  candidate=192.168.1.2:57199
reached on the local network  peer=7a4ebf0c
```

The phone hears nothing. Its sync ends in "No device answered. They have to be
awake and running at the same time," which is what the engine reports when no
peer could be reached.

**What was tried.** Android drops multicast before it reaches an application
unless it holds a `WifiManager.MulticastLock` — the radio would otherwise wake
for every packet on the network. That is exactly the shape of this failure, so
the lock is now held for the length of a sync, released in a `finally`, with the
`CHANGE_WIFI_MULTICAST_STATE` permission it needs. It did not change the result.

**What is still open.** The likeliest remaining cause is which interface the
group is joined on. `if_addrs` is used to enumerate interfaces and join the
group on each; Android 11 and later restrict the NETLINK access that needs, so
the list is probably empty and the code falls back to letting the kernel choose.
If the kernel picks something other than Wi-Fi, the IGMP membership never
reaches the access point, and an access point doing IGMP snooping will not
forward the group to a client that never joined. That would produce exactly this
asymmetry: sending needs no membership, receiving does.

*(Superseded by the section above; kept because the reasoning in it is what
led there.)*

**A diagnosis gap, now half closed.** The engine's `tracing` output reached
nowhere on Android: `tracing-subscriber` was a dev-dependency only, so no
subscriber was ever installed and every log line in the entire engine was
discarded. Every conclusion above had to be inferred from the laptop's side and
from the app's own Kotlin logging.

A `tracing-android` layer is now installed on the first call into the engine,
filtered by the same `RUST_LOG` the desktop uses. It has not yet produced a line
on hardware, so either the layer is not reaching logcat or the sync under test
did not run — which is itself the next thing to find out, and is a far better
place to be stuck than having no channel at all.

**A third attempt, not yet tested on hardware.** Interface enumeration is now
backed by asking the kernel directly which address it would send to this group
from — a throwaway socket connected to the group address, no packet sent, its
local address read back. That needs no permission and no enumeration, which is
exactly what a platform that restricts NETLINK requires. Whether it fixes the
phone is unknown.

Desktop-to-desktop local discovery is verified and has tests, including a full
sync between two devices with no rendezvous service in existence.

## A file sent to the phone came straight back

**Found 2026-09-24 on the Galaxy S23 and the laptop. Fixed 2026-09-25 and
verified on both the same day.**

The laptop sent the phone a file, which the phone filed privately, as
[decision 0030](../decisions/0030-sending-a-file-to-one-device.md) says it
should. The phone's next scan of its folder then found the file, asked the
index about it, and was told nothing: every "what do you know about this path"
looked only in the shared area. So the scan indexed the file as new and shared,
advertised it, and the laptop wrote it into its synced folder. The one device
that was meant to have it had published it to every device, starting with the
one that sent it.

**The cause was one assumption, repeated.** A received file lives in the same
folder as the shared ones, because somebody asked for a file and should find a
file. Every piece of code that walks the folder or looks a path up had been
written when the folder held only the shared area, and each still assumed it.

**Looking for the other places turned up ten more**, each now with a test:

| | what went wrong | consequence |
|---|---|---|
| 1 | the scan's deletion sweep did not see received files | deleting one went unnoticed; it stayed live in the index for ever |
| 2 | reading one by name answered "not found" | the app's *Save a copy* failed on exactly the files most likely to be saved |
| 3 | the app's list was the shared area only | received files were invisible in the app, though the system picker showed them |
| 4 | deleting one kept its chunk references | the index claimed bytes that had left with the file, which `verify` reports as missing data |
| 5 | a shared *tombstone* at the same name was taken for the file | a received file whose name had once been used and deleted could not be deleted |
| 6 | a tombstone chose its row by name | another device's deletion of an unrelated shared file could tombstone the private one |
| 7 | another device's deletion removed whatever was at the path on disk | **the received file was deleted** |
| 8 | a shared file arriving at a received file's name was written over it | **the received file was destroyed** — its bytes lived nowhere else here |
| 9 | "stays private when edited" also applied to versions from other devices | a shared version could be re-filed as private on this device alone |
| 10 | a send's check for "unchanged" compared against the sender's own file of the same name | **sending a file from the folder under its own name recorded nothing, and nothing was sent** — present since sending existed |

Rows 7 and 8 lose a file outright, and row 10 is a send that silently does not happen. Row 8 is now refused, as a case collision
is ([decision 0013](../decisions/0013-case-collisions.md)): the shared file waits,
recorded as a failure saying why, and arrives once the received one is renamed
or deleted.

None of this was reachable by any earlier test, for a plain reason: no test
ever gave a received file a name that also meant something in the shared
area. It is the Phase 2 shape again — every component right on its own, wrong
where two of them meet.

**Verified on Linux** — the laptop (CachyOS, kernel 7.2.6), debug build,
2026-09-25. Twelve tests in `crates/engine/tests/received_files.rs`, one in
`crates/mobile-ffi/tests/lifecycle.rs` and one in
`crates/storage/tests/vault_send.rs`. The tests for rows 2 to 5 and 10 failed
before their fix was written, and rows 7, 8 and 9 were each checked by removing the
fix and watching the test fail. Rows 1 and 6 are covered by tests that were not
separately seen failing. The whole workspace: 611 tests in 72 test binaries
pass, and clippy is clean.

**Verified on the phone** — 2026-09-25. Galaxy S23 (SM-S911B) running the debug
APK, and the laptop running `qurb run ~/qurb` from a release build, on the same
Wi-Fi with no rendezvous service anywhere: they found each other by local
discovery. A 67-byte file, `private-check-0925.txt`, sent from the laptop with
`qurb send`:

| check | result |
|---|---|
| the phone collects it | the app's count went from 21 files to 22; the laptop recorded *sent* and *collected* |
| it does not come back | three syncs from the phone, each with a scan; no *received* on the laptop, and nothing new in `~/qurb` |
| the app lists it | listed, 67 B |
| *Save a copy* | written to the phone's Downloads through the system picker; SHA-256 identical to the original |
| deleting it sticks | back to 21 files after two syncs; the laptop did not deliver it again |

The app has no way to delete a file — deliberately, see *The files, from the
rest of the phone* — so the deletion was made by removing the file from the
app's folder with `adb shell run-as`. That exercises the scan noticing a
deletion (row 1 above). The FFI's own `remove`, the path a future delete button
would take, is verified on Linux only.

**What yesterday's bug left behind is still there.** `sent-to-phone.bin`, the
file that came back on 2026-09-24, is a shared file on both devices now,
because that is what the phone turned it into. The fix stops it happening
again; it does not guess which shared files were once private. Deleting it is
safe.

**Still open, and stated here so nobody assumes otherwise:**

- **One folder, two namespaces.** The refusal in row 8 trades data loss for a
  shared file that does not arrive until somebody renames something, with only
  an activity entry saying why. On a desktop,
  [decision 0037](../decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md)
  removes the problem by taking received files out of the folder. On a phone,
  [decision 0036](../decisions/0036-a-phone-keeps-its-own-files.md) has to solve
  it, and says so.
- **Version vectors on received files are bookkeeping, not history.** An edit or
  deletion of one is stamped from the shared area's history for that name —
  usually none — rather than from the version that was delivered. Nothing
  compares them today. 0036's propagation to a holder will.
- **`Store::restore_file` matches by name alone**, so undeleting would revive
  every tombstone at that path, whichever area it is in. It has no caller
  outside tests yet; it needs the same treatment before it gets one.
- **`Store::evict` still looks only at the shared area.** Nothing asks it to
  free one chosen file yet. When *Free local space* is built, a received file
  must be refused with the real reason — the sender's copy is one the phone may
  not count on — not "not found".
- **Case collisions are checked against the shared area only.** A received
  `Report.pdf` beside a shared `report.pdf` is not reported. Both of this
  project's platforms are case-sensitive, so it is latent rather than live.

## A phone's own files, and somebody to keep them

**2026-09-27. Built in the engine and verified between two desktops; not on a
phone yet.**

[Decision 0036](../decisions/0036-a-phone-keeps-its-own-files.md) makes a
phone's own files private to it, with a device it chooses keeping a copy it
never shows. That is what lets the phone free space and have a file back —
the brief's acceptance steps 20 to 23 — without its photographs appearing on
the desktop. The engine half is built: four areas on the wire (`qurb/2`), the
holder keeping and dropping, and the phone freeing and fetching back. The
decision records how it was verified and what tracing it found.

Found on the way, and fixed separately because it was live in the shared area
already: a file freed for the storage cap and fetched back stayed marked as
freed — fetched again on every sync, and a later deletion of it never passed
on. See decision 0025.

**What the phone still needed**, all of it step 6: the app setting its files
private, a way to name the device that keeps them, and *Free local space* and
*Download* on a file. Built the same day — see [the rebuilt app](#the-rebuilt-app).
Every device must be rebuilt for `qurb/2`, the phone included, or they will not
connect.

## Light and snappy, measured

**2026-09-27, Galaxy S23 (SM-S911B), Android 16.** Asked which interface
toolkit the rebuilt app should use, the project owner said only that it should
be light and snappy. So the question became what makes it so, answered with
measurements — see
[decision 0039](../decisions/0039-a-light-android-app.md), which has the table.

The release build had never been made lean: R8 was off, and it carried an
emulator's engine no phone uses. With R8, arm64 only and the engine built with
link-time optimisation, the APK went from 26.6 MB to **10.7 MB** (the debug
build people had been installing was 46.7 MB), and the code held in memory from
16.5 MB to **8.1 MB**. Cold start was about **175 ms** either way: the gain is
in size, not in startup, which was already fast in any release build.

**How the first attempt at the numbers went wrong** is worth keeping. The
phone's screen turned off during the runs, and launches behind the lock screen
finish much faster or much slower than real ones, so the first batches clustered
around 160 and 350 ms and suggested a difference that was not there. The
measuring script now wakes the screen before every launch and aborts if the lock
screen is showing.

**Found, and fixed the same day:** the phone reported 100.7 MB on disk for
30.9 MB of files, because nothing on a phone ever ran garbage collection or
reclaimed the copies older builds kept. The routine the desktop daemon runs is
now one engine function, `housekeep`, and the phone runs it after every
background sync and once per launch, after the list is drawn.

On the S23 the first run freed about 19.5 MB — the chunk store went from
100.7 MB to 81.2 MB. The index, copied off the phone with a debug build and
read on the laptop, says what the rest is: all of it belongs to one deleted
file, a 2.27 GB video of which the phone had fetched 81 MB, deleted on
22 September. Deleted content is kept for seven days, so it is collected on the
29th. Keeping part of a file that was never fully here cannot help restore it;
harmless, and recorded rather than special-cased.

The number the phone showed was also misleading. "On disk" counted the chunk
store alone and was worded as a saving ("100.7 MB on disk, from 30.9 MB"); it
now counts the folder's files as well, and the screen says what qurb takes on
the phone only when that is more than the files themselves.

## The rebuilt app

**2026-09-27, Galaxy S23.** The app the product brief asks for, on platform
views as [decision 0039](../decisions/0039-a-light-android-app.md) chose: five
tabs where there was one list and an overflow menu.

- **Home** — which devices the phone knows, a card shown only while some files
  exist on this phone and nowhere else (with the next step for each of three
  reasons that can be so), and what happened lately. *Connect a device* is the
  big button until one is connected, *Sync now* after.
- **Vault** — every file with where its bytes are: a green dot for here and on
  another device, amber for only here, grey for on another device and not here.
  Tapping offers what that allows, including *Free phone space* and
  *Download*. Read two hundred at a time.
- **Devices** — who the phone knows, and which of them keep its files; choosing
  one is decision 0036's holder, with the consequences said before it is done.
  Files picked from anywhere on the phone can be sent to one device.
- **Transfers** — sends waiting to be collected, each of which can be stopped,
  and the history. The brief lists Activity as a sixth place; a tab bar holds
  five, and what happened and what is moving are one question asked at two
  times.
- **Settings** — the phone's name, *Keep new files private* (on by default, per
  decision 0036), space, background sync, the rendezvous service, the version.

The screens are plain classes holding their views rather than Fragments, and
none reads anything on the main thread. Installed over the existing app with
the same key, so its data — 21 files, one paired desktop — was the phone's
real data, and Home, Vault, Transfers and Settings were checked against it on
screen, then all five again after fixes. Three faults found that way:

- Home's history rows were indented twice, because Android ignores a negative
  `layout_marginHorizontal`; and its big button said *Connect another device*
  while a device was already connected. Both fixed and seen fixed.
- The Devices screen said *"Not reached yet"* about the desktop the phone had
  been taking files from for days. Only the desktop daemon ever recorded when a
  device was last reached; the phone's sync never did. It does now, once the
  other device has answered, and the end-to-end pairing test checks it — seen
  to fail first. On the phone this shows at the next sync that reaches the
  desktop, which was switched off.

**Measured**, against the one-list app it replaced, back to back on the same
phone — the table is in
[decision 0039](../decisions/0039-a-light-android-app.md#the-rebuilt-app-measured).
Startup did not change (medians of 171 and 173 ms against 186 and 171 ms); five
screens cost on average 0.2 MB of heap and 3.5 MB in total; the APK is 10.8 MB
against 10.7 MB.

**Found by looking at the screens:** Settings showed the background worker's
last record as *"no paired devices"* on a phone paired with a desktop. The
desktop was switched off; waiting for it used the worker's whole twenty-second
window, and a pass that ran out of time while waiting was reported as time
running out, which the worker answers with a retry — exponential backoff, for a
device that will not answer until someone switches it on. Writing the test for
that found that connecting to a rendezvous service that never answers had no
bound at all. Both fixed and recorded in
[decision 0020](../decisions/0020-sync-takes-a-deadline.md#found-on-a-phone),
with what was observed and what was only reproduced.

**The phone and the laptop keeping each other's files**, the same afternoon,
through the phone's screens: chosen, kept, freed, fetched back byte-identical
and let go on deletion, with nothing named on the desktop. The details are in
[decision 0036's progress](../decisions/0036-a-phone-keeps-its-own-files.md#progress).
To do it the laptop ran the current build against `~/qurb`, whose index moved
from schema version 10 to 12; a copy of the version-10 index was kept, and the
`qurb` and `qurb-tray` installed on 22 September predate `qurb/2` and should not
open that folder again.

Later the same day a pass also stopped trying its devices one after another:
it reaches them all at once and syncs each as it answers, so a switched-off
device no longer uses the window a working one needed (decision 0020, with the
test that showed it).

It found three more things, all fixed or recorded:

- **Each pass left its discovery running** — six sockets and three beacon
  listeners a minute after the third pass — and announcing addresses that no
  longer answered. Fixed in the connector.
- **A pass ended before the desktop could collect from it.** A pass with
  something waiting now stays open up to ten seconds. Both are in
  [decision 0020](../decisions/0020-sync-takes-a-deadline.md#a-pass-that-waits-to-be-collected-from),
  with what was and was not measured.
- **`big-from-laptop.bin` shows as only on this phone** while the laptop has
  it: the phone received it on 18 September, and recording where a received
  file came from began on the 22nd. Nothing asks for it afterwards. Safe —
  such a file is never offered for freeing — and recorded rather than fixed.

The share sheet's confirmation, seen when a test share failed for reasons of the
test's own, had not taken the new design: square corners, the old background, a
title the theme was meant to hide. It has now, and its message follows the
privacy setting — it used to promise that "your other devices will get it" of a
file that, private by default, goes only to a device chosen to keep it.

**The phrase is confirmed on the phone**, by the rules decision 0033 set for the
desktop: three words typed back, checked by the engine against the key, and the
words shown again from the key rather than kept. The matching rule moved into
`RecoveryPhrase::matches` so the two cannot differ. Walked through on the
emulator (a throwaway key): a wrong word kept the dialog open and said so; the
app force-stopped half-way came back to the words, the same 24; the right
words, one in capitals with a trailing space, went on to the app, and a
relaunch went straight there. Screenshots of those dialogs come out blank —
`FLAG_SECURE` — so the check was driven through the accessibility tree.

**Not done here:** the storage question during setup (decision 0038, which is
for desktops), the system picker asking the engine, sending straight to a
device from the share sheet, and a phone showing a code rather than only
scanning one.

## The file picker asks the engine

**2026-09-27, on the emulator**, paired with a throwaway laptop device in
`target/live-check/` that shares the emulator's throwaway key, through a
rendezvous service on the laptop at the address the app uses by default
(`ws://10.0.2.2:9000`, the emulator's way to its host). The laptop device ran
with its own `HOME` and config directory, so nothing reached the real folder
registry or Downloads.

The provider used to walk the folder. It now lists what the engine knows:
`browse` gives a directory's folders and files, `entry` one file, `search`
matches anywhere in a path — one index query each. Checked end to end: a text
file made on the laptop synced to the emulator, freed there, **was listed in the
system Files app**, and **opening it downloaded it** — the viewer showed the
laptop's line and the copy back in the folder had the same SHA-256. Saving a
copy into qurb's root through the system save dialog left a 63-byte file with
the right contents, which the next sync indexed as the phone's own.

Getting there found four things:

- **A freed shared file asked for on a phone was never downloaded.** The
  daemon added the step that turns "asked for back" into a download after
  planning; the phone planned with the same function and never added it. The
  first attempt to open the freed file reached the laptop and came back empty.
  Moved into `plan_with`, which both use;
  `a_freed_shared_file_comes_back_when_asked_for` fails without it. The Vault's
  *Download* had the same bug for shared files — the hardware check earlier
  brought back a *private* file, which takes a different path.
- **Saving into qurb from another app could only make an empty file.** The
  provider created the file and then refused to open it for anything but
  reading. Writing is now allowed onto a file on the phone; a created file gets
  a free name rather than the name of one it would overwrite.
- **A folder name with `_` or `%` in it matched other folders**, in two index
  queries — one behind the desktop's Files screen. Escaped, and tested with
  folders called `a_b` and `axb`.
- **Paths from other processes were used as given.** A document ID with `..`
  in it now cannot leave the folder, nor one starting `.qurb` enter the store.

Not checked: the same on the S23, and opening a freed file when no device that
has it is reachable, which should fail with the message it was written to give.

## A phone shows a code

**2026-09-28.** Connect a device now offers three ways: scan the other
device's code, **show a code on this phone**, or type one. Showing one is what
lets two phones connect with no computer between them (brief §20): until now a
phone could only scan, so the other device always had to be one that could
show a code.

The code is drawn from a QR matrix the engine computes (`qr_code` in the FFI,
the `qrcode` crate the command line already used, without its image features)
into a bitmap in Kotlin — no image library in the app. The screen counts down
to the code's expiry, keeps the display on, and closing it in any way stops the
code working.

Two faults in the FFI's pairing, fixed with it: `cancel` did nothing once
`wait` had started — the host had already been taken out to wait on — so a
phone would have listened until the code expired five minutes later; and the
pairing host bound the sync port, which a background sync starting meanwhile
would have found taken. It binds port zero now, as the desktop does. Tests:
`giving_up_on_a_code_stops_the_wait_already_blocking` and
`a_pairing_code_is_drawn_as_a_qr_code`. Built and compiled; **not yet run on a
phone**, and phone-to-phone needs a second Android device to verify.

## The Vault by folder, and the share sheet's choice

**2026-09-28.** The Vault lists a folder at a time from the engine's index —
the calls the system file picker already used, so the two cannot disagree —
with search across every folder, sorting by name, newest or largest, and Back
going up a folder before it leaves the app. ⋯ saves everything in the folder
to a folder on the phone in one go (Android's folder picker, then each file
exported and copied), and opens Recently deleted. Deleting says the file is
kept in Recently deleted for 30 days.

Sharing into qurb from another app now asks where it should go — **Save to My
Vault**, or **Send to** any paired device (brief §39) — and asks only when a
device is paired to send to. Sent that way, a file goes to that device alone
and is not added to the phone's Vault.

**Renaming and moving** a file from the Vault, and making a folder, came with
it. A file keeps its area when it moves: a phone files new things privately
(decision 0036), and a shared file renamed there must not quietly become
private — which to every other device would look like a deletion.
`Store::rename_file` writes the new path in the old one's area, and
`crates/engine/tests/renaming.rs` checks both directions across two devices.
A folder made on the phone is on disk only until something is put in it: the
index knows files, not folders.

Built and compiled; **not yet run on the phone**.

## Run on the emulator, and what that found

**2026-09-28.** The new screens run on the `qurb-test` emulator, headless,
upgraded in place over the previous build — so the index migrations from
schema 12 to 15 ran on a real phone's data. Walked through: the Vault by
folder (new folder, into it and out with Back, moving a shared file into it
and seeing it stay shared), the three ways to connect, the phone showing a
code — its QR decoded with `zbarimg` to exactly the code printed under it —
Settings' new rows and Version, choosing which devices have a folder, and
deleting a file then restoring it from Recently deleted.

It found a fault the engine tests had not, because they all ran as desktops:
**on a phone, a file qurb writes itself went into the phone's own vault.** A
phone files new things privately (decision 0036), and the sharing rule, a
restored file, and the results of settling a conflict are all new paths — so
a sharing rule made on a phone was invisible to every other device, and a
shared file restored or kept there would have vanished from them. The store
now writes those in the area already decided (`put_file_in`), retiring a row
left in the wrong area by the earlier build. Four engine tests with a phone
fail without it. And two small things: a file's size was shown twice in the
Vault, and the rule file was counted in "of files".

Not on the emulator: anything needing a second device to answer — a conflict
arriving, a removal, the share sheet sending — and the real phone.

## Checked between the Galaxy S23 and the laptop

**2026-09-28, evening.** The new build installed over the old one on the S23,
keeping its data; the index migrated to schema 15 and every file and the
history were intact. Between the phone and the laptop, on home Wi-Fi:

- **Sync** after the upgrade: the laptop reached the phone over the local
  network at once.
- **Wake tokens kept**: the phone's token was in the rendezvous service's
  `wake-tokens.json` (mode 600) after its first sync, and the service could be
  restarted without losing it.
- **Recently deleted across devices**: a file deleted on the laptop went into
  the phone's Recently deleted, "deleted on saqib just now"; restored from the
  phone, it was back on the laptop two seconds later with its contents.
- **A real conflict**: the same file edited on both before either saw the
  other's change; both devices got both versions, the phone's Home showed the
  card, and **Keep both** chosen on the phone gave both devices the two files,
  the renamed one still shared — the fault the emulator found, fixed, holding.
- **Pushes**: three changes on the laptop woke the idle phone within two to
  three seconds, with the rendezvous service logging each push. Two earlier
  changes, made 30–40 s after a sync from the phone, woke nothing, and the
  service was not logging at debug then to say why; the service is left
  logging pushes at debug (`RUST_LOG` in `~/.config/qurb/rendezvous.env`) so
  the next miss says whether the service sent one.

Found and fixed on the way: a conflict version the phone itself made was named
by its id ("4b91ac75") rather than "this phone", and "Keep both" named the
file "(other version)" rather than after the phone. The same on the desktop.

Not checked on the phone: removing a device (it would have removed the
laptop), the share sheet (it needs another app driven), and a pairing with the
phone showing the code (the laptop is already paired).

## The designed app

**2026-09-29** (commit `fad2d9b`). The app rebuilt to the owner's design
direction ([design/direction.md](../design/direction.md)), the second half of
[decision 0048](../decisions/0048-the-design-direction.md) — the desktop was
done the same day. Still the platform's own views, as
[decision 0039](../decisions/0039-a-light-android-app.md) chose; no UI library
was added.

**Four tabs where there were five** (direction §25): Home, Files, Devices,
Settings, under a floating glass bar. The places reached from them go on a
small stack that Back leaves: Private Vault from Files, Activity from Home's
*See all*, Recently deleted from Files and from Settings. Transfers stopped
being a tab: a bar above the tabs, there only while the phone is syncing or
has sent something not yet collected, opening a sheet with what is active,
what is waiting to be collected (each stoppable) and the last eight that
finished.

- **Home** answers whether the phone's Qurb space is okay, built from what a
  phone can know, since there is no daemon to ask: whether a sync is running,
  whether files made here have reached no other device, and when it last
  reached one. That last is new — `last_reached_at`, written by the app's own
  sync and by the background worker whenever a device answered. Its states
  are *Add your first device*, *Syncing…*, *N files are waiting to reach your
  devices* (saying which of the reasons it is), *Not synced yet* and
  *Everything is synced*. One action — *Send to device*, or *Add a device* —
  a line of facts, *Sync now* and pull to sync; a file with two versions as
  attention, opening a sheet with both and the three choices; five recent
  events. A ring of light turns while syncing (§32) and the mark settles with
  one pulse when everything becomes synced (§33).
- **Files** is the shared area a folder at a time, from the index: folders as
  tiles two to a row, breadcrumbs, search across every folder a quarter of a
  second after the last key, sorting, and each file's state in the direction's
  words — *On this phone*, *Available elsewhere*, *Only copy here*,
  *Downloading*. Tapping a file opens a sheet with its details and what can be
  done. A folder made on the phone that holds nothing yet is listed from the
  disk, since the index knows only files. *Downloading* is remembered for the
  life of the app, not stored.
- **Private Vault** is the same browser over the phone's own vault. Each lists
  only its own area, and *Add files* in each adds into it, whatever *Keep new
  files private* says —
  [decision 0049](../decisions/0049-adding-a-file-puts-it-where-you-are-looking.md),
  with `browse_in`, `search_in` and `import_into` added to the FFI for it.
- **Devices** shows this phone and each paired device as cards, with when each
  was last here. A device's sheet holds the choice to keep a backup of the
  Private Vault (decision 0036's holder), sending it files, and removing it
  after saying what that does. A device that has just paired fades in (§34).
- **Settings** is grouped lists in the direction's order (§23): this phone,
  devices, storage, privacy, notifications, recovery, appearance, advanced.
- **The share sheet** offers *Save to Private Vault*, *Save to Files, on all
  your devices*, or a paired device to send to; with no device paired it saves
  where the setting says and asks nothing.

Underneath: one file of components, `Kit.kt` — rows, file states, groups,
toggles, attention, empty states, sheets — the Android counterpart of the
desktop's `core.js`. The colours, type and glass are resources, named by role;
Inter is bundled in three weights with its licence in the app's assets; 53
Lucide icons are generated as vector drawables by `scripts/android-icons.py`;
the new mark is the launcher icon. Screens come forward a little as they
appear, and with the phone's animations turned off nothing moves and sheets do
not blur. The night palette was removed: light only until dark is designed.

**How it was checked.** The FFI additions have a test,
`each_area_lists_its_own_and_adds_into_itself`, and on 2026-10-03 all 754
tests passed on the laptop. The app was built: a debug APK on the laptop is
dated 2026-09-29, a little under two hours before the commit. **Nothing records it running**
— not on the S23, not on the emulator — and no screen of it has been looked
at in this record. (It had run: the phone's own history, read on 2026-10-03,
says so — [below](#the-designed-app-on-the-s23).) Nor has it been measured against decision 0039's table, so
whether it is still light and snappy is, for now, unknown.

**Found while writing this up**, 2026-10-03, from the code:

- Settings → Notifications said that what was sent to the phone *"is in
  Transfers, from Home"*. Transfers is reached from its bar, which shows only
  while the phone syncs or has a send waiting; what arrived is in Activity,
  from Home's *See all*. Corrected the same day to say Activity.
- Settings → Key protection said the key was *"behind this phone's own lock.
  Nobody without the phone unlocked can use it."* The Keystore key is
  deliberately not tied to unlocking, so background sync can use it while the
  phone is locked; the screen lock guards the app, not the key. Corrected the
  same day to say that. Both compiled (`compileDebugKotlin`); not seen on a
  device.
- `Qurb.kt` had `ownFilesPrivate`'s description stranded above the new
  `lastSynced`; moved back to the function it describes.

**Left undone:** the owner's review; dark mode; moving a file between areas
(brief §2); notifications on the phone; comparing a conflict's two versions
beyond who, when and how big; and everything the earlier sections list as not
yet tried on the phone.

## The designed app on the S23

**2026-10-03, Galaxy S23 (Android 16), over wireless debugging.** The build
from that day installed over the one on the phone, keeping its data and key;
then every place walked by driving the phone from the laptop: Home, its
syncing state and the Transfers bar, Files, a file's sheet, the ⋯ sheet,
Private Vault, Recently deleted, Devices, a device's sheet, *Add a device*,
Settings and its sheets, Activity, and the share sheet's question — opened
and cancelled, nothing saved. Read from the accessibility tree and from
screenshots; no file was deleted, freed or moved and no device removed.

**It had been on the phone since 2026-09-29.** The phone's package record
puts the previous install at 03:17 that night, a minute after a debug build of
the redesign, and the phone's history shows what followed: the laptop removed
from the phone at 03:18, paired again at 03:46, and fourteen sends to it
between 03:46 and 04:54, each collected. So removing a device, pairing and
sending from the phone ran on hardware, through the new screens, before the
section above was written. Which way the pairing went, and which screen sent,
the history does not say.

**Then nothing synced for four days.** From 2026-09-30 the laptop's desktop
had been running a different, empty folder — the cause and the fix are in
[phase 4](phase-4-product.md#a-folder-typed-without-a-slash). With it back on
`~/qurb`, *Sync now* reached the laptop directly over the home Wi-Fi and Home
said *Everything is synced … synced just now*.

Found, and fixed the same day:

- **Home said *Not synced yet*.** True of this pairing: re-pairing at 03:46
  reset the engine's record of when the laptop was last reached, and nothing
  had reached it since. But a phone that merely upgraded would have said the
  same, because the note Home read is new with the design while the engine's
  own record is older. Home now takes the later of the two.
- **Recently deleted cut off the days left** — "Expires in 2…", the part a
  person opens the screen for. Now "5 days ago on saqib · 25 days left", which
  fits two lines beside *Restore*. Seen on the phone.
- **The share sheet's question was Material's own lilac dialog**, the one
  screen outside the design: its theme never named the app's dialog style. It
  does now. Seen on the phone.
- **A send from the app waited for *Sync now*.** Reported by the owner the same
  evening. Sending (from Home, a file or a device), adding, renaming, moving
  and deleting files, choosing a device to keep the vault, and pairing by
  scanning only redrew the screen; a phone has no daemon to pass a change on,
  so it waited for the next background pass, up to an hour away with push.
  Each now starts a sync at once (`MainActivity.madeChange`), another runs
  after one already under way, and a send says *Sending to …*, then *Sent to
  …* or *Waiting for …*. Installed on the S23 and seen working there by the
  owner: a send reached the laptop with no *Sync now*.

Seen, and as designed: Home's states and the ring that turns while syncing,
the Transfers bar, each file's state in words, a file's actions, sorting and
*Save 21 files to this phone*, Private Vault apart from Files, devices as
cards with when each was last seen, the two Settings lines corrected that
morning, Background sync's record of its last run, and Activity.

Two things true of the owner's files, recorded rather than changed:

- **18 of the 21 files are on the phone only.** The laptop freed its copies on
  2026-09-29 — its history says "local copy freed; another device keeps it"
  for each — and the phone rightly marks them *Only copy here*. *Keep on this
  device* on the laptop brings any back.
- **Nothing keeps a backup of the phone's Private Vault.** Removing the laptop
  ended that, as [decision 0041](../decisions/0041-removing-a-device.md) says
  it should, and pairing again does not restore it.

Not done here: measuring the designed app against
[decision 0039](../decisions/0039-a-light-android-app.md); the share sheet
actually saving or sending; pairing by the phone's own code, watched; and the
owner's review.

## An 800 MB video, and what stopped it

**Reported 2026-10-03 by the owner**: an 800 MB video added on the phone was
"very slow and also it failed". The laptop's log has it. The phone, paired
again at 17:58 UTC, was reached over the home Wi-Fi; the laptop began pulling
the video into `~/qurb` and at 18:01:18 the fetch ended with *connection lost*,
two minutes and thirty-two seconds in. Nothing fetched it again.

Three causes, each enough on its own:

- **Chunks were fetched one at a time**, each waited out before the next was
  asked for. 800 MB had not crossed in 152 seconds: under 5.3 MB/s, on a link
  that carries several times that.
- **An interrupted fetch started again from nothing.** The partial file was
  opened fresh on every attempt.
- **The phone stopped answering.** A pass waits at most ten seconds for a
  device to collect and never past its window, and Android freezes an app
  nobody is looking at; a connection already open lasted until then.

Fixed on 2026-10-04 —
[decision 0050](../decisions/0050-large-files-from-a-phone.md): every device
fetches eight chunks at once; a fetch that was cut off chunks what it has and
carries on from the last chunk that matches; and a pass with 32 MiB or more
waiting to be collected runs in the background worker as a foreground service,
under a notification with the bytes sent so far, answering for as long as a
device collects, up to thirty minutes. *Sync now* hands such a pass to the
worker.

Tested: the network tests, now fetching eight at a time; a 12 MiB file cut
two-thirds through arrives intact with only the rest crossing — and with
resuming switched off, all 12,582,912 bytes cross and the test fails; a partial
file that is not the content is not kept; and the rule for when a pass goes on
answering, on its own.

### Measured on the S23, 2026-10-05

The Galaxy S23 (SM-S911B) served and the development laptop collected. Both
were on the home Wi-Fi, 5 GHz channel 36, 80 MHz: the phone's link at 468/526
Mbit/s (RSSI −54), the laptop's at 351/243. Two files the phone had sent to the
laptop were collected, an 872,548,335-byte video and a 1,729,528,613-byte
video, both held on the phone as encrypted chunks. Release builds were used.
Rates come from the partial file's size, sampled every five seconds or taken
from the laptop's log and the file's modification time. Times below are the
phone's, IST (UTC+5:30).

| laptop fetching | phone | bytes | seconds | rate |
|---|---|---:|---:|---:|
| eight at once | app open, build before 0050 | 822,544,151 | 154.3 | 5.33 MB/s |
| one at a time (`IN_FLIGHT = 1`) | foreground pass, app open | 262,870,136 | 55.1 | 4.77 MB/s |
| one at a time | foreground pass, app left | 248,372,214 | 65.2 | 3.81 MB/s |

**Eight at once barely helped.** It ran 12 to 40 per cent faster than one at a
time, and every rate was a small fraction of what either link carries. So the
waits between requests were not what held the transfer back, and that
contradicts what decision 0050 expected of them. The conditions were not
clean, but they don't account for it. Google Photos was reading 85–107 MB
every four seconds on the phone through the first one-at-a-time run (its
`io_stats` lines), but only in the first two seconds of the second. For the
rest of the second run, qurb was the phone's busiest reader, at 12–18 MB every
four seconds, and that run was the slowest. The eight-at-once run had the older
phone build, whose serving is the same code. Where the limit is, the phone's
serving or QUIC on this path, is not known. A plain TCP baseline could not be
taken: this network lets neither device accept a TCP connection from the
other, and ADB over Wi-Fi manages only 3.1 MB/s of its own. The laptop now logs each fetch of 8 MiB or
more with its rate and the connection's round trip, congestion window and
losses (`report_fetch` in `crates/peer/src/client.rs`), so the next large
transfer will say more.

**A transfer survives leaving the app.** *Sync now* at 12:56:07 IST started
the foreground service (the system log's `am_foreground_service_start`) and the
notification. At 12:57:38 the phone was swiped to its home screen by hand,
not by the test, and at 12:57:44 Samsung's freezer logged *stop trying to
freeze com.qurb*. The phone went on
serving: the last row of the table is that stretch.

**Started from the background, it is refused, as decision 0050 expected.** At
12:51:25, after an install, a background wake had more than 32 MiB to send and
asked for the foreground service. Android refused it
(`am_foreground_service_denied`), and that pass ran as an ordinary one.

**A pause of more than ten seconds ends the pass.** The pass started at
12:56:07 ended at 12:59:29, 202 seconds into the thirty minutes it was allowed.
The cause was this test. The laptop's app was stopped mid-transfer at 12:59:17
and a new one started. It connected at 12:59:27 and spent two more seconds
re-reading 942 MB of partial file before it asked for a chunk. By then the
phone had seen no chunk go out for ten seconds (`COLLECTING`). It closed the
pass, dropped its notification, and was frozen ten seconds after that. The new
connection fetched until the freeze and was lost at 13:00:14. Anything that pauses a collector
for ten seconds will do the same: a laptop restarting or waking from sleep, or
a Wi-Fi drop.

**Resuming worked on hardware twice**: *carrying on from an earlier attempt
kept=114864514*, then *kept=942721024*, each followed by the file growing from
there.

**The laptop's notifications had died.** Each failed fetch was followed by a
panic in the desktop's notification watcher, which is not the transfer path:
[phase 4](phase-4-product.md#notifications-stopped-at-the-first-failure). Fixed
the same day.

**Both arrived the same afternoon**, with the fixed laptop build and its new
log line. *Sync now* at 13:38:33, the app open throughout. The laptop carried
on from the partial files and each file was checked whole on arrival:

| file | fetched this pass | seconds | rate | round trip | packets lost |
|---|---:|---:|---:|---:|---:|
| the 1.7 GB video, from 964,393,025 | 765,135,588 | 154.6 | 4.95 MB/s | 13 ms | 0 |
| the 873 MB video, from 822,544,151 | 50,004,184 | 10.1 | 4.97 MB/s | 15 ms | 0 |

The phone's foreground service ran from 13:38:33 to 13:41:33 and stopped ten
seconds after the last chunk went, as it should.

**That narrows where the limit is.** A 13 ms round trip with eight chunks
(about 4 MiB) asked for at a time leaves room for hundreds of megabytes a
second. The laptop lost nothing and saw MTU 1452. So the path is short and
clean, and the requests are not what holds the rate. The limit is in the
phone's sending. Either it serves each chunk slowly, or its QUIC sender's
congestion window holds it back. Serving a chunk, under the store's one lock,
means an index query on whether the asker may have it, then reading the
sealed chunk, decrypting it and checking its hash. Only the phone can report
either, and it doesn't yet.

**Home lied during the pass.** *Sync now* handed the pass to the background
worker and returned at once, and nothing redrew the screen when the worker
finished. For the three minutes 815 MB was leaving, Home said *Everything is
synced … synced 38 minutes ago*, and it still said so afterwards. The same
hand-off had a worse edge. `SyncWorker.runNow` enqueued with `REPLACE`, so a
second *Sync now*, or the sync after any change made in the app, would have
cancelled a long pass part-way. Fixed the same day: the app watches the
worker's passes, the schedule's and *Sync now*'s, while it is on screen. It
shows them as syncing, redraws when they end, and holds its own syncs until
then, running any asked for meanwhile afterwards. `runNow` appends behind a
running pass instead of replacing it.

Watched on the S23 at 16:38 the same day, with Home left on screen
throughout. A 64 MiB file added through *Add files* started a pass in the
worker (foreground service 16:38:43–16:39:03). Screenshots show Home reading
*Syncing… Bringing your devices up to date* while it ran, and redrawing itself
to *Everything is synced · synced just now* when it ended. Not exercised:
a second sync asked for during a worker's pass. *Sync now* is disabled
while one runs, so only a change made in the app can ask, and that was not
tried.

**Adding into Files, on the phone** ([decision 0049](../decisions/0049-adding-a-file-puts-it-where-you-are-looking.md)),
was done there for the first time: three 64 MiB test files, picked with *Add
files* in Files from the system picker. Each went into the shared area and
reached the laptop's `~/qurb` within seconds:

| file | seconds | rate | round trip | laptop's packets lost |
|---|---:|---:|---:|---:|
| first | 11.5 | 5.86 MB/s | 12 ms | 0 |
| second | 10.6 | 6.33 MB/s | 15 ms | 14 |
| third | 9.1 | 7.36 MB/s | 10 ms | 27 |

The rates are the same few megabytes a second. Deleting each in the app
removed it from the laptop within seconds. *Delete for good* in the phone's
Recently deleted removed them there only. The laptop keeps its copies for 30
days, as [decision 0042](../decisions/0042-recently-deleted.md) intends, each
device's list being its own.

**The laptop then synced thirteen times in half a second.** Arrival events
had queued while it spent 2½ minutes on one sync, and each found nothing to
do. Harmless, but each is a tree fetched from the phone; not changed.

## Deliberately left undone

- **Keychain, on iOS.** The Android half is done and verified on a device —
  the app supplies the keystore across the FFI, which is
  [decision 0021](../decisions/0021-the-platform-supplies-the-keystore.md). The
  same seam is what iOS would use, and nothing has been written against it.
- **Selective sync, automatically.** A phone cannot hold a desktop's library.
  A person can now say a folder is kept only remotely
  ([decision 0045](../decisions/0045-a-folder-kept-remotely.md)) — listed,
  fetched when opened — and free single files; what is not built is the phone
  deciding for itself, by space or by age, and nothing yet warns before a
  large download on mobile data.
- **Two phones that are both asleep.** Sync needs both devices awake and
  announced at the same moment, because a QUIC handshake's opening packets are
  the hole punch. Two desktops manage this by being on all the time. Two phones,
  each awake for a few seconds a day at the platform's discretion, may simply
  never meet. Push narrows it — one phone can now wake the other — but only
  while the waking one is itself awake to ask. This is still the strongest
  argument for a storage-only replica in the picture, and the reason the
  roadmap plans iOS as a good viewer rather than a peer equal to a desktop.
- **Being told without Firebase.** Push is built and measured, and it is a
  dependency on Google. A deployment without one falls back to the phone
  deciding when to look and Android deciding how often to allow it — about
  fifteen minutes, longer while dozing. UnifiedPush would remove the
  dependency at the cost of asking people to install a distributor app, and
  has not been attempted. (`BGTaskScheduler` and APNs, the iOS halves, are
  untouched along with the rest of iOS.)
- **Upgrading a relayed connection back to direct.** Inherited from Phase 3 and
  worse on a phone, which changes network several times a day: a connection that
  fell back to the relay while on cellular stays relayed after it reaches Wi-Fi.
  Since a per-pass connector is built fresh each time, the next pass does get a
  fresh chance — so on mobile this is less severe than on the desktop daemon,
  which holds one connector for hours.
- **Comparing a conflict's two versions.** A conflict is attention on Home
  that opens a sheet and settles it — keep this version, the other, or both
  ([decision 0043](../decisions/0043-settling-a-conflict.md)); settling was
  verified on the S23, before the design. The sheet says who made each version,
  when and how big, and whether the other is on the phone yet. The preview for
  images and text the brief designs (§2) is not built.
