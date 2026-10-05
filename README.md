# qurb

Private cloud storage. Dropbox-like sync where your files stay on your own
devices — they move directly between them, encrypted end to end, and are never
stored on our servers.

**Status (2026-10-03): working on Linux and Android, and both apps designed;
the owner's review of the design, the relay on a server, and a release are
next, in that order.**

- The engine is complete and hardened: two devices sync end to end over QUIC,
  with encryption, conflict handling that never discards an edit, and key
  management — verified at 100,000 files, and tested against crashes, wrong
  clocks, long absences, damaged disks and hostile peers.
- The **desktop window** does everything the command line does: setting up,
  pairing by QR code, browsing and freeing files, sending to one device,
  recently deleted, settling conflicts, sharing a folder with chosen devices,
  keeping a folder only remotely, and a passphrase on the key — designed to the
  owner's direction, in light only. There is an Arch package.
- The **Android app** syncs with a laptop both ways, from home Wi-Fi or mobile
  data, is woken by push within seconds, takes shares from any app, and shows
  its files in the system file picker — verified on a Galaxy S23. Its screens
  were rebuilt to the same design on 2026-09-29 and walked on the phone on
  2026-10-03.
- Not built yet: dark mode, the relay running on a server, a formal release,
  iOS, macOS and Windows. Unmeasured: battery over a day on a phone, how often
  a direct connection works across other networks, and what the design costs
  the Android app in size and startup.

**[docs/features.md](docs/features.md)** lists everything that exists, and how
far each piece has been checked.

---

## Start here

**[docs/CODEBASE.md](docs/CODEBASE.md)** — everything needed to understand this
project from scratch. No prior context assumed.

Then, depending on what you want:

| you want | read |
|---|---|
| what it can do today | [docs/features.md](docs/features.md) |
| the vocabulary | [docs/glossary.md](docs/glossary.md) |
| what comes next, and what the product brief asks | [docs/product-plan.md](docs/product-plan.md) |
| the phases, and the honest risks | [docs/roadmap.md](docs/roadmap.md) |
| the full target design | [docs/architecture.md](docs/architecture.md) |
| why a particular choice was made | [docs/decisions/](docs/decisions/) |
| what each phase produced and measured | [docs/phases/](docs/phases/) |
| how to try it on your own hardware | [docs/trying-it.md](docs/trying-it.md) |

## Layout

```
docs/           documentation — start with CODEBASE.md
crates/         the engine, the program, the desktop window, the services
android/        the Android app, a thin Kotlin layer over the FFI
packaging/      installing: an Arch package, an install script, server units
scripts/        building for Android, generating bindings and icons, the desktop smoke test
experiments/    throwaway spikes, clearly marked as such
website/        the website (Next.js), in the apps' design, independent of the engine
```

The split between `crates/` and `experiments/` is deliberate. Experimental code
may cut corners provided its README says which. Code in `crates/` is meant to
last. Nothing migrates silently between them.

## Trying it

On Arch, a package of this checkout — the window, the tray icon and the
command line:

```bash
cd packaging/arch && makepkg -si
```

Anywhere else, for this user only:

```bash
cargo build --release -p qurb-cli -p qurb-tray -p qurb-desktop
./packaging/install.sh
```

Then open qurb from the applications menu: it sets the device up, shows the 24
words that are your key, and pairs another device by QR code. From a terminal
instead:

```bash
qurb init ~/qurb                    # prints the 24 words
qurb enrol ~/qurb "wheel push ..."  # on the second device, with those words
qurb pair                           # on one, then `qurb join ~/qurb <code>` on the other
qurb run                            # on both
```

The Android app: `./scripts/android-app.sh install`. Syncing from outside the
house needs the rendezvous service reachable from both devices —
[docs/anywhere.md](docs/anywhere.md). Step by step, including the phone:
[docs/trying-it.md](docs/trying-it.md).

To watch the whole system work in one process, servers included:

```bash
cargo run --release -p qurb-peer --example demo -- /tmp/device-a /tmp/device-b
```

## Building and testing

```bash
cargo build --release
cargo test --workspace         # 766 tests in 90 binaries (2026-10-05)
./scripts/desktop-smoke.sh     # the real window, driven end to end
```

The Phase 0 measurements — chunking throughput, boundary stability, NAT
classification — are in [experiments/phase0-spike](experiments/phase0-spike/)
and [docs/phases/phase-0-spike.md](docs/phases/phase-0-spike.md). `quictest
stun` there contacts public STUN servers, revealing your public IP to them as
any video-call client does.
