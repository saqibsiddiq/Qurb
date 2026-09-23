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
`qurb signal` has to be reachable from both. Making it reachable is
[anywhere.md](anywhere.md), which includes a way to do it for nothing.

**A phone on mobile data has since been verified** — a file crossed to a laptop
at home with the phone's Wi-Fi off. But that path ran over an overlay network
(Tailscale), which is WireGuard doing the traversal rather than qurb's own hole
punching, so it answers "can somebody use this from a train" and **not** the
kill criterion. The direct-connection rate is what determines the relay
bandwidth bill, and it is still unmeasured.

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
as a shell user on a plugged-in, awake phone. The app (section 6) lives under
quite different rules once the system has backgrounded it, and none of that —
battery, suspension, what Android actually grants a background worker — is
measured.

## 6. The Android app

```bash
./scripts/android-app.sh install
```

Builds the native libraries, regenerates the Kotlin bindings, builds a 21 MB
APK and installs it on a connected device. Needs the NDK and a JDK 17; the
script says so if it cannot find them.

On first launch it offers to create an identity or restore from 24 words. The
key goes into the Android Keystore, where the app itself cannot read it.

The synced files also appear in the Files app, under **qurb**. Menu →
**Background sync** shows what the scheduler is doing and when it last ran.

To sync with a computer, the phone needs a rendezvous service to find it
through. There is no hosted one, so run one:

```bash
qurb signal 0.0.0.0:9000
```

In the app: menu → **Rendezvous service** → `ws://<your computer's LAN IP>:9000`.
Then `qurb pair <dir>` on the computer, and menu → **Pair a device** on the
phone with the code it prints. Press **Sync** on both.

Both devices have to be awake and running at the same moment — a QUIC
handshake's opening packets are the hole punch, so a device that is only
listening has punched nothing. The rendezvous service tells each side the
instant the other appears, so "at the same moment" means overlapping at all,
not being lucky with timing.

If pairing times out, the usual cause is a firewall on the computer: sync is
UDP, and a rule that allows ping will still drop it. On Linux:

```bash
sudo ufw allow from 192.168.0.0/16 to any proto udp
```

## 7. Syncing from somewhere else

Everything above is one network. To sync from a train, the rendezvous service
has to be reachable from outside the house — and only it: the files go directly
between the devices.

[anywhere.md](anywhere.md) has the recipe, including a free one that needs no
server and no port forwarding, and what it costs. For a host of your own,
[packaging/server/](../packaging/server/README.md) has the unit files.

## 8. What you cannot test yet

- **A day of unattended sync.** The background worker is scheduled and runs
  when asked, but nobody has left a phone alone for a day to see what Android
  actually grants it, or what that costs in battery. If you try it, menu →
  **Background sync** records the last run. This is the single most useful
  thing left to measure.
- **Raw NAT traversal between two networks.** A phone on cellular has now
  synced with a laptop at home — but through an overlay network, which does the
  traversal itself. What remains unmeasured is qurb punching through a carrier
  NAT unaided, which is the hard case Phase 3's direct-connection rate is
  actually about.
- **iOS, at all.** Building it needs Xcode, which needs a Mac. The Swift
  bindings generate and have never been compiled.
- **Battery.** `syncWithin(seconds)` is built for short background windows and
  nothing has measured what a sync actually costs.

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
