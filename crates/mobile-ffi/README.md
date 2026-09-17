# qurb-mobile

The engine, as a phone can call it.

Android and iOS cannot call Rust directly. This crate is the seam: a small,
deliberately boring surface that [UniFFI](https://mozilla.github.io/uniffi-rs/)
turns into Kotlin and Swift. It contains no sync logic of its own — everything
delegates to `qurb-engine` — because logic behind an FFI boundary is logic that
cannot be tested from the rest of the workspace.

## Generating the bindings

```bash
./scripts/mobile-bindings.sh
```

Writes Kotlin to `target/bindings/kotlin` and Swift to `target/bindings/swift`.
Neither is committed: both platforms' build systems regenerate them, because
bindings that drift from the library they describe fail at runtime rather than
at compile time.

## Building for Android

```bash
./scripts/android-build.sh --release
```

All four architectures. The script finds the NDK; set `ANDROID_NDK_HOME` if it
cannot. SQLite is the only C dependency in the tree and the only reason the NDK
is needed at all.

Stripped, the library is 3.2–3.9 MB depending on architecture — roughly what the
engine adds to an app download, with SQLite, Zstd, BLAKE3, XChaCha20-Poly1305
and QUIC all inside it.

## What the surface looks like

Setup, then a handle:

```kotlin
if (!isSetUp(root)) {
    val setup = create(root)          // 24 words — show them once, then never again
}
val qurb = Qurb(root, null)
qurb.scan()                           // catch up with what changed while we were not running
```

Files by path, content by file:

```kotlin
qurb.list()                           // List<FileEntry>
qurb.export("album/photo.jpg", tmp)   // writes to tmp, returns bytes written
qurb.importFile(tmp, "album/photo.jpg")
qurb.remove("album/photo.jpg")
qurb.usage()                          // logical vs on-disk
```

The second device, from the words:

```kotlin
restore(root, "wheel push industry ...")
```

## Three constraints, all from the platforms

**Memory.** An iOS FileProvider extension is killed at a ceiling in the tens of
megabytes. Nothing here returns a file's contents: `export` writes to a path the
caller supplies, `importFile` reads from one. Peak memory is one chunk — at most
2 MiB — however large the file is. A test asserts this rather than a comment
claiming it, so a change that reintroduces buffering fails rather than ships.

**Threading.** The engine takes `&mut self`, so one lock guards it. Calls block.
The platform side is expected to make them off the main thread, which Kotlin
coroutines and Swift's `async` both do naturally.

**Errors.** A Rust error chain does not survive the crossing. Everything becomes
`QurbError`, which is flat, matchable, and short on purpose: it lists only the
cases a caller can *act* on — ask for the passphrase again, show the setup
screen, tell the user the file is gone, retry later. The original message is
kept as text for logs.

## Key protection on a phone

The desktop offers three options — a file, the OS keystore, or a passphrase.
Here the vault is an owner-only file inside the app's private storage, and
`Qurb::open` takes `null` for the passphrase.

That is weaker than it sounds on a desktop and stronger than it sounds here. An
app's private directory is enforced by the kernel, not by convention, and on
both platforms it is encrypted at rest by the device's own lock screen. The
realistic attacker against a phone is someone holding the phone, and they are
stopped by the lock screen rather than by anything this crate does.

The gap is a phone with no passcode set, where file-based protection provides
nothing. Passing a passphrase to `Qurb::open` works today and is the answer
until Keychain and Android Keystore are wired up — see below.

## Not built yet

- **Keychain and Android Keystore.** The `keyring` crate the desktop uses does
  not cover mobile; both platforms need their own binding through this crate.
- **Networking from the phone.** `qurb-peer` compiles for Android but nothing
  here exposes pairing or sync. `import`/`export` against a local store is the
  whole surface so far, which means this crate is a file store on the phone and
  not yet a syncing one.
- **Background sync.** Both platforms schedule background work on their own
  terms and kill anything that outstays it. This needs platform-side work, not
  more Rust.
- **Verification on a device.** The library builds for all four Android
  architectures and exports the right symbols, but has never been loaded by an
  Android or iOS process. See `docs/phases/phase-5-mobile.md`.
