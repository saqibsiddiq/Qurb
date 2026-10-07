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
| [0014](0014-pairing.md) | Pairing transfers a full fingerprint out of band | Accepted — the token also guards the key since 0052; a device is let in only on approval since 0053 |
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
| [0025](0025-a-storage-cap-that-cannot-lose-data.md) | A storage cap that cannot lose data | Accepted |
| [0026](0026-sharing-while-the-other-device-is-off.md) | Sharing while the other device is off | Accepted |
| [0027](0027-plaintext-stops-at-the-local-network.md) | Plaintext rendezvous stops at the local network | Accepted |
| [0028](0028-waking-a-sleeping-device.md) | Waking a sleeping device, and what it costs | Accepted |
| [0029](0029-two-areas-shared-and-private.md) | Two areas: one shared, one private per device | Accepted, extended by 0036 |
| [0030](0030-sending-a-file-to-one-device.md) | Sending a file to one device | Accepted, amended by 0037, extended by 0036; a delivery sent again is acknowledged, and a phone lets go of delivered sends only when asked by name, since 2026-10-07 |
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
| [0053](0053-approval-same-key-and-safe-copies.md) | Pairing is approved, checks the key, and a phone's copy is not a safe last one | Accepted — built and tested on all three; watched on the S23 with the command line, not in the desktop window |
| [0054](0054-a-file-the-other-device-does-not-hold.md) | A file the other device does not hold is listed, not failed every sync | Accepted — built, tested, and watched on the S23; extended by 0055 |
| [0055](0055-a-file-on-no-device-says-so.md) | A file on no device says so | Accepted — built on the desktop, the phone and the command line |
| [0056](0056-dark-mode.md) | Dark mode, on the same tokens | Accepted — built on the desktop and Android |
