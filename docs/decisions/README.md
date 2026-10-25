# Decision records

One file per significant technical decision. Numbered sequentially, never
deleted.

A decision belongs here if reversing it later would be expensive — a language, a
protocol, a storage format, a rule that data correctness depends on. Routine
implementation choices do not need a record.

## Format

Each record states the decision, the reasoning, what was rejected and why, the
tradeoff accepted, and how hard it would be to reverse. That last field is the
most useful one in practice: it tells you which decisions you are allowed to be
wrong about.

## Status values

- **Accepted** — in force.
- **Superseded by NNNN** — replaced. The file stays, with a pointer.
- **Proposed** — under consideration, not yet acted on.

Superseded records are never removed. The record of what was believed, and why,
is worth more than a tidy directory.

## Index

| # | Decision | Status |
|---|----------|--------|
| [0001](0001-hybrid-p2p-topology.md) | Hybrid P2P with central coordination | Accepted |
| [0002](0002-rust-core-with-native-shells.md) | Rust core, native UI shells | Accepted |
| [0003](0003-sqlite-plus-cas.md) | SQLite index alongside a CAS on disk | Accepted |
| [0004](0004-chunk-parameters.md) | 128 KiB / 512 KiB / 2 MiB chunk bounds | Accepted |
| [0005](0005-conflict-resolution.md) | Conflict resolution never discards edits | Accepted, extended by 0009 |
| [0006](0006-availability-gap.md) | Storage-only replicas, for offline availability | Accepted |
| [0007](0007-chunk-format.md) | On-disk chunk format, fixed before key management | Accepted |
| [0008](0008-watcher-delivery-guarantee.md) | Watcher delivers changes at least once | Accepted |
| [0009](0009-conflict-edge-cases.md) | Conflict cases 0005 did not cover | Accepted |
| [0010](0010-content-by-hash.md) | Content is requested by hash, never by path | Accepted |
| [0011](0011-peer-identity-pinning.md) | Peer identity is a pinned certificate fingerprint | Accepted, completed by 0014 |
| [0012](0012-key-hierarchy-and-recovery.md) | One root secret, derived keys, 24-word phrase | Accepted — "the phrase is the only way to recover" superseded by 0052 |
| [0013](0013-case-collisions.md) | Case collisions are refused, not resolved | Accepted |
| [0014](0014-pairing.md) | Pairing transfers a full fingerprint out of band | Accepted — the token also guards the key since 0052; a device is let in only on approval since 0053; six digits fetch the code since 0066 |
| [0015](0015-control-plane-in-rust.md) | Signalling and the relay are written in Rust | Accepted — billing open |
| [0016](0016-what-signalling-learns.md) | What the signalling server is allowed to learn | Accepted |
| [0017](0017-relay.md) | The relay carries datagrams, not messages | Accepted |
| [0018](0018-file-contents-never-cross-the-ffi.md) | File contents never cross the FFI | Accepted |
| [0019](0019-filenames-are-nfc.md) | Filenames are normalised to NFC | Accepted |
| [0020](0020-sync-takes-a-deadline.md) | Sync takes a deadline, and running out is not an error | Accepted, amended by 0050 |
| [0021](0021-the-platform-supplies-the-keystore.md) | On mobile, the app supplies the keystore | Accepted |
| [0022](0022-the-service-announces-arrivals.md) | The rendezvous service announces arrivals | Accepted |
| [0023](0023-one-person-per-account.md) | One person per operating-system account | Accepted, default location amended by 0037 |
| [0024](0024-the-file-is-the-payload-store.md) | The file in the folder is the payload store | Accepted |
| [0025](0025-a-storage-cap-that-cannot-lose-data.md) | A storage cap that cannot lose data | Accepted — its rule for replicas confirmed by 0058 |
| [0026](0026-sharing-while-the-other-device-is-off.md) | Sharing while the other device is off | Accepted |
| [0027](0027-plaintext-stops-at-the-local-network.md) | Plaintext rendezvous stops at the local network | Accepted |
| [0028](0028-waking-a-sleeping-device.md) | Waking a sleeping device, and what it costs | Accepted |
| [0029](0029-two-areas-shared-and-private.md) | Two areas: one shared, one private per device | Accepted, extended by 0036 |
| [0030](0030-sending-a-file-to-one-device.md) | Sending a file to one device | Accepted, amended by 0037 and 0059 (a send, not its bytes, is taken once), extended by 0036; a phone lets go of delivered sends only when asked by name, since 2026-10-07 |
| [0031](0031-what-happened-is-written-down.md) | What happened is written down | Accepted |
| [0032](0032-the-interface-hosts-the-daemon.md) | The interface hosts the daemon, and asks it nouns | Accepted — availability's fourth value in 0055 |
| [0033](0033-the-phrase-on-a-screen.md) | The recovery phrase on a screen | Superseded by 0052 for setup: no phrase shown or checked; showing it in Settings stands |
| [0034](0034-finding-each-other-with-no-server.md) | Finding each other with no server | Accepted |
| [0035](0035-a-rendezvous-on-a-bare-address.md) | A rendezvous on a bare address | Accepted |
| [0036](0036-a-phone-keeps-its-own-files.md) | A phone keeps its own files, and another device holds them for it | Accepted — built, verified between a phone and a laptop; amended by 0049 |
| [0037](0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md) | A file sent to a desktop is an ordinary file in Downloads | Accepted — built |
| [0038](0038-the-storage-question-during-setup.md) | The storage question is asked during setup | Accepted — built |
| [0039](0039-a-light-android-app.md) | A light Android app, on the platform's own views | Accepted |
| [0040](0040-the-menu-opens-the-window.md) | The applications menu opens the window | Accepted |
| [0041](0041-removing-a-device.md) | Removing a device | Accepted — built on the desktop, the phone and the command line; what only it kept reads as on no device since 0055 |
| [0042](0042-recently-deleted.md) | Recently deleted | Accepted — built on the desktop, the phone and the command line |
| [0043](0043-settling-a-conflict.md) | Settling a conflict | Accepted — built on the desktop, the phone and the command line |
| [0044](0044-sharing-with-chosen-devices.md) | Sharing a folder with chosen devices | Accepted — built on the desktop, the phone and the command line |
| [0045](0045-a-folder-kept-remotely.md) | A folder kept only remotely | Accepted — built on the desktop, the phone and the command line |
| [0046](0046-the-window-asks-for-the-passphrase.md) | The window asks for the passphrase, and has a Security section | Accepted — amends 0033; built on the desktop |
| [0047](0047-versions-and-upgrades.md) | Versions, installing, and upgrading | Accepted — no automatic updater, by decision |
| [0048](0048-the-design-direction.md) | The design direction | Accepted — revised the same day for the owner's direction, and to build it in the apps rather than in Figma; built on the desktop and Android, light only |
| [0049](0049-adding-a-file-puts-it-where-you-are-looking.md) | Adding a file puts it where you are looking | Accepted — amends 0036; built on Android |
| [0050](0050-large-files-from-a-phone.md) | Large files from a phone: fetched in parallel, resumed, and served in the foreground | Accepted — amends 0020; built, and measured on the S23, where fetching in parallel gained little |
| [0051](0051-bbr-not-cubic.md) | QUIC paces by measured bandwidth (BBR), not by loss (Cubic) | Accepted — built; 2.5× from the phone over Wi-Fi, measured with the spike |
| [0052](0052-the-key-travels-with-the-code.md) | The key travels with the pairing code; nobody writes 24 words down | Accepted — built on the command line, the desktop and Android; the S23 rejoined the laptop with a code |
| [0053](0053-approval-same-key-and-safe-copies.md) | Pairing is approved, checks the key, and a phone's copy is not a safe last one | Accepted — built and tested on all three; watched on the S23 with the command line, not in the desktop window; the device joining is told who shows the code since 0074 |
| [0054](0054-a-file-the-other-device-does-not-hold.md) | A file the other device does not hold is listed, not failed every sync | Accepted — built, tested, and watched on the S23; extended by 0055 |
| [0055](0055-a-file-on-no-device-says-so.md) | A file on no device says so | Accepted — built on the desktop, the phone and the command line |
| [0056](0056-dark-mode.md) | Dark mode, on the same tokens | Accepted — built on the desktop and Android |
| [0057](0057-moving-a-file-into-or-out-of-private-vault.md) | Moving a file into or out of Private Vault | Accepted — built on the desktop, Android and the command line; watched with the command line |
| [0058](0058-a-full-replica-says-so.md) | A full replica keeps everything and says so | Accepted — the existing behaviour, decided |
| [0059](0059-a-send-is-not-its-bytes.md) | A send is not its bytes: the same file sent again arrives again | Accepted — built on all three; amends 0030 rule 2 |
| [0060](0060-a-computer-keeps-private-folders-for-several-people.md) | A computer keeps private folders for several people, and qurb keeps no copy of what it sends | Accepted — steps 1–5 built, step 6 (watched on hardware) not yet; supersedes 0023 for guests, amends 0030 rules 1 and 4 |
| [0061](0061-the-relay-carries-meetings.md) | The relay carries meetings: one connection, several names | Accepted — amends 0017; closes 0060's relay gap |
| [0062](0062-backing-up-whatsapp.md) | Backing up WhatsApp to a computer, not to the Google account | Accepted — kept in the phone's Private Vault on the laptop, all 36,062 of the S23's files known kept since 2026-10-15; restoring unproven |
| [0063](0063-backing-up-a-phones-folders-chosen-on-the-desktop.md) | Backing up a phone's folders, chosen on the desktop | Accepted — built and seen on the S23 and the laptop; chosen on either device; amended 2026-10-25: chosen as a tree, at any depth, the deepest choice deciding |
| [0064](0064-many-small-files-quickly.md) | Many small files, quickly: what limits it, and the order of work | Accepted as a plan — step 1, answering requests in parallel, built; not yet measured on the S23 |
| [0065](0065-the-scale-qurb-is-built-for.md) | The scale qurb is built for: big libraries, a family server, a service | Accepted as targets — measured at 100,000 and a million files; the file list in frames, and V24's index, built |
| [0066](0066-six-digits-to-pair.md) | Six digits to pair, on the same network | Accepted — built on the command line, the desktop and Android; paired by digits in the real-window smoke test, not yet between the S23 and the laptop; amends 0014 |
| [0067](0067-a-folder-for-each-person.md) | A folder for each person, opened with their phone | Accepted — all four steps built; `~/qurb` by person run on the laptop; the owner's folder opened with a yes on the S23, and the phone's folder browsed from it, 2026-10-15; sending to one's folder, and downloading from it on the S23, not yet seen; extends 0060 to the owner |
| [0068](0068-the-rendezvous-service-relays.md) | The rendezvous service relays too, and a device finds it there | Accepted — built, and deployed on the laptop behind its Funnel; not yet seen carrying a sync from the S23 on another network; amends 0017; the connection dials again since 2026-10-18 |
| [0069](0069-too-recent-to-trust.md) | A file read moments after it was written is read again | Accepted — built, index schema 26; found failing on Android 8, reproduced and fixed here, passing there since |
| [0070](0070-the-first-release.md) | The first release, 0.1.0: what is built, for what, and how far it is checked | Accepted — built 2026-10-18/19, not published; Windows not run; Linux run on the laptop, Android on emulators |
| [0071](0071-a-file-list-fetched-only-when-it-changed.md) | A device's file list fetched only when it changed; the relay alongside a slow direct path | Accepted — built, measured on a hotspot: 24 KiB for a pass with nothing changed, down from 9.8 MB; amended by 0072 for a pass after a change |
| [0072](0072-only-what-changed-in-a-list.md) | Only what changed in a device's file list is sent | Accepted — built, index schema 27; on a copy of the laptop's index one changed file costs 194 bytes rather than 7.9 MB; not yet run on the S23, nor on a hotspot or mobile data |
| [0073](0073-the-phone-works-with-its-computer.md) | The phone works with one computer: no shared files on it, and it may change its own folder there | Accepted as the design in the 2026-10 redesign — built on Android 2026-10-24: the shared folder read, written and searched, the phone's own folder changed from the phone, the phone out of the shared area, and Files; a deletion made there undone at the phone or at the computer; not yet the desktop window, nor a new folder in the phone's folder; amends 0067, 0036's *Keep new files private* and 0049 on a phone, an exception to read-only serving; amended the same day: Files lists every computer, the device is chosen on Home, the shared folder open to every guest, read-write |
| [0074](0074-who-shows-the-code.md) | A device about to join is told who shows the code | Accepted — built in the peer crate, the phone's engine, the command line and Android, setup and Devices; checked on an emulator, and against a build from before it; not yet in the desktop window; amends 0053 |
