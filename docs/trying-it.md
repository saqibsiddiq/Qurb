# Trying it on your own hardware

Everything below has been run. Where something has *not* been verified, it says
so rather than pretending.

Build first:

```bash
cargo build --release
```

That puts `qurb` in `target/release/`. Add it to your path or use the full
path — the examples below assume it is on your path.

---

## 1. Two directories on one machine (5 minutes)

The quickest way to see the engine work. No network, no daemon, no pairing:
two directories treated as two devices, reading each other's stores directly.

```bash
cargo run --release --example sync_pair -- /tmp/dev-a /tmp/dev-b
```

Put files in either directory and run it again. Then try the interesting cases:
edit the same file in both between runs and watch it produce a conflict copy
rather than discarding either version; delete something on one side; rename a
large directory and notice that nothing is re-transferred.

Run it twice with no changes. The second run should store nothing and finish
almost instantly — that is the size-and-mtime fast path.

## 2. The whole system in one process (2 minutes)

Starts a rendezvous service and a relay, brings up two devices, pairs them,
connects them, and keeps running:

```bash
cargo run --release -p qurb-peer --example demo -- /tmp/device-a /tmp/device-b
```

Drop a file into either directory and watch it appear in the other. Everything
here is real except that it is one process: real sockets, real QUIC handshakes,
real encryption, real conflict resolution.

## 3. Two real daemons (15 minutes)

This is the actual program. Three terminals.

**Terminal 1** — the rendezvous service, which introduces devices to each other:

```bash
qurb signal
```

**Terminal 2** — set up the first device. It prints 24 words:

```bash
qurb init ~/qurb-a
```

**Terminal 3** — set up the second with those words. Both devices share one key;
that is what makes them *yours*:

```bash
qurb enrol ~/qurb-b "the twenty four words it printed"
```

Now introduce them. `qurb pair` prints a code and waits:

```bash
qurb pair ~/qurb-a
```

In the other terminal, hand that code over:

```bash
qurb join ~/qurb-b qurb1-XXXXX...
```

Both should report `Paired with ...`. Then run both daemons:

```bash
qurb run ~/qurb-a
```

```bash
qurb run ~/qurb-b
```

Put a file in `~/qurb-a` and it appears in `~/qurb-b`. `qurb status ~/qurb-a`
shows what is stored and who it is paired with.

**Verified**: this exact sequence was run on one machine, with both daemons
connecting through the rendezvous service over the real network stack.

## 4. Two machines on two networks

The one measurement the project still needs, and the only one that requires
hardware this codebase has not had access to.

Same as section 3, except the second device is a different computer and
`qurb signal` has to be reachable from both. The direct-connection rate is what
determines the relay bandwidth bill, and it is unmeasured.

Start with the cheap version:

```bash
qurb netcheck
```

Run it on every network you use — home, phone hotspot, office, café. The full
method is in [measuring-connectivity.md](measuring-connectivity.md).

## 5. On an Android phone

**Done, and it passed.** A Samsung Galaxy S23 (Android 16, arm64-v8a) runs all
35 binaries — 426 tests in 104 seconds — with a 512 MiB file over QUIC costing
6 MiB of heap. Worth repeating on other hardware, especially anything older,
slower, or from a different manufacturer.

On the phone: Settings → About → tap *Build number* seven times, then
Developer options → **USB debugging**. Plug it in and accept the prompt on the
screen. Check it is visible:

```bash
adb devices
```

It should say `device`. If it says `unauthorized`, the prompt was not accepted.
Then:

```bash
./scripts/android-test.sh aarch64
```

This builds the test binaries for ARM64, strips them (236 MB down to 10 MB
each — unstripped they will not fit), pushes them with `adb`, runs them, and
deletes each one after. It needs the Android NDK; the script says where to get
one if it cannot find it.

Expect around 426 tests across 35 binaries, taking a couple of minutes.

**If anything fails on your device that passes here, that is a real find** —
keep the output. Re-run that one binary with `STRIP=0` to get symbols in the
backtrace.

What this does *not* test, on any device: the tests run from `/data/local/tmp`
as a shell user on a plugged-in, awake phone. An installed app that the system
has backgrounded lives under quite different rules, and none of that is measured
because there is no app.

## 6. What you cannot test yet

- **An app on a phone.** There is no app. The engine runs on Android and syncs,
  but there is nothing installable and no screen.
- **iOS, at all.** Building it needs Xcode, which needs a Mac. The Swift
  bindings generate and have never been compiled.
- **The platform keystore.** `KeyStore` is a contract with a test against a
  fake. No Android or iOS implementation exists.
- **Battery and background behaviour.** `sync_within(seconds)` is built for it
  and nothing has measured what it costs on a real device.

---

## If something breaks

The test suite is the first thing to check:

```bash
cargo test --workspace
```

Then the store's own integrity check, which reads and verifies every chunk:

```bash
qurb verify ~/qurb-a --deep
```

For the daemon, `RUST_LOG=qurb=debug,qurb_peer=debug qurb run ~/qurb-a` says a
great deal more about what it is trying to do.
