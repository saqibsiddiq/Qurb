# Roadmap

Six phases. Each has an explicit **kill criterion**: the result that means stop
and reconsider rather than push on.

**Where it stands, 2026-10-08:** Phases 0–2 complete; Phase 3 built with its
kill criterion unmeasured; Phases 4 and 5 working on Linux and Android. What
comes next was set by the project owner on 2026-09-28, in this order:

1. **Design and UX** of the desktop window and the Android app. Every feature
   the product brief asks for is built — [features.md](features.md) lists them.
   Both apps were rebuilt to the owner's design direction on 2026-09-29
   ([0048](decisions/0048-the-design-direction.md)), and have had a dark mode
   since 2026-10-08 ([0056](decisions/0056-dark-mode.md)). What has and has
   not been watched between the Galaxy S23 and the laptop is marked row by
   row in features.md. What remains is the owner's review.
2. **The relay on a server of the owner's own**, so the phone and the laptop
   reach each other from any network, not only where a direct path exists.
3. **A formal release for Linux and Android.**

macOS, Windows and iOS come after Linux and Android are finished.

## Honest feasibility

Stated plainly, because the plan is only useful if the estimates are:

| question | answer |
|---|---|
| Can this be built at all? | **Yes, high confidence.** Every subsystem exists in shipped software — Tailscale, Syncthing, restic, WireGuard, Iroh. Nothing here requires invention. |
| Can it be a daily-usable tool in ~4 months solo? | **Likely.** This is the target. |
| Can it be a commercial SaaS in 12 months solo? | **No.** The scope is roughly 24–36 person-months. |

Feasibility is not the risk. **Scope per person-year is the risk**, compounded by
an unusually high quality floor: a note-taking app that is 95% correct is fine, a
sync engine that is 95% correct silently destroys data and never gets a second
chance.

---

## Phase 0 — Spike ✅ complete

*2–3 weeks. Throwaway code answering whether the core ideas hold.*

All five kill criteria passed, and the phase caught a real design error in the
chunk parameters — cheap to fix at week two, expensive at month eight. Full
results: [phases/phase-0-spike.md](phases/phase-0-spike.md).

---

## Phase 1 — The engine ✅ complete

*Months 1–3. Two desktops, one folder, no UI beyond a tray icon.*

Every component built — storage, watching, the local engine, conflict
resolution, QUIC transfer, key management — and **the kill criterion run and
passed**: 100,000 files synced between two devices with every correctness check
green. See [phases/phase-1-engine.md](phases/phase-1-engine.md).

The criterion also established what is *not* wrong. At 100k files the system is
correct and slow: a cold index runs at 422 files/s, with 58% of that going to
filesystem syscalls rather than to cryptography or the database. The fast paths
are already there — a warm re-index of 100k files takes 1.13s, and syncing 1,000
edited files out of 100,000 takes 3.5s.

Optimising is ordinary work with a clear order (parallelism first). It should
happen *after* Phase 2, because parallelising a storage engine before there are
tests that catch an optimisation breaking correctness is how data gets lost.

The full local pipeline: chunking, hashing, SQLite index, content-addressable
store, compression, encryption, filesystem watching, and QUIC transfer between
two machines.

Encryption goes in from the start. Retrofitting cryptography is how projects end
up with a threat model nobody can describe.

The part the original plan underweights is **chunk garbage collection**.
Deduplication means one chunk may be referenced by many files and many versions;
deletion therefore requires reference counting. Get this wrong and data corrupts
silently, long before anyone notices.

> **Kill criterion:** cannot sync 100k files cleanly without a reproducible
> correctness bug. — **Run 2026-09-16. Passed.**

**Also due this phase:** a decision on
[0006 — availability](decisions/0006-availability-gap.md). Not necessarily which
option ships, but whether the engine must be able to replicate chunks to a node
that is not the user's own device. That answer shapes the data model.

---

## Phase 2 — Adversarial correctness ✅ complete

*Months 4–5. The phase most projects skip and then regret.*

Property-based convergence testing with shrinking, crash injection with real
`SIGKILL`, clock skew, month-long absences, case-insensitive collisions,
corruption repair from a peer, large renames, hostile peers, and concurrent
collection. See [phases/phase-2-correctness.md](phases/phase-2-correctness.md).

**Five real defects found**, none of which any Phase 1 test would have caught.
The most serious: a writer could reference a chunk garbage collection had just
removed, because the check and the reference sat in different transactions and
the write lock only covered the second — caught by a foreign key rather than by
the design. Also repair silently doing nothing, renames re-transferring entire
trees depending on how the names sorted, deletions leaving empty directories,
and contention behaviour inherited from a library default.

Every one the same shape — correct in isolation, wrong in combination. The
defects lived in the seams, not in the components.

Property-based testing over a simulated two-device world: random operations,
random interleavings, assert convergence. Then deliberate abuse — kill the
process mid-write, corrupt a chunk, skew a clock by three days, take a device
offline for a simulated month, rename a directory containing 10k files during a
sync, collide `README` and `readme` on a case-insensitive filesystem.

Conflict semantics are settled here in code, per
[0005](decisions/0005-conflict-resolution.md).

> **Kill criterion:** convergence failures that cannot be explained. If the
> engine cannot reliably agree with itself across two devices, mobile will not
> rescue it. — **Not met.** Every failure was explained, and five were defects
> that are now fixed.

---

## Phase 3 — Networking at scale 🔨 built — kill criterion unmeasured

*Months 6–7.*

STUN, signalling, and a relay. Device pairing by QR code with out-of-band key
verification. The plan said a Go control plane; the services are Rust instead
([0015](decisions/0015-control-plane-in-rust.md)), and accounts and billing
are left to Phase 6.

**Pairing is built** — an invite carrying the inviter's full fingerprint across
an out-of-band channel, a trust store binding device identity to network
identity, and a listener that takes its guest list from it. See
[phases/phase-3-networking.md](phases/phase-3-networking.md).

Budget real time for the relay. It is one line in the architecture document and
a substantial service in practice — connection management, fairness, abuse
prevention — and it is the component that costs money per byte forever.

> **Kill criterion:** direct-connection rate below ~70% across tested networks.
> Below that, bandwidth cost scales with users and the economics change.

---

## Phase 4 — Desktop product 🔨 in progress

*Months 8–10.*

The daemon runs, with some two dozen commands around it, plus `qurb signal`
and `qurb relay` for the services. Running it kept finding bugs the whole test
suite had missed — which is the phase's real lesson rather than an aside. See
[phases/phase-4-product.md](phases/phase-4-product.md).

The interface went through two shapes. First a tray icon with a small GTK
window — status and a storage slider — which GNOME, having no tray, could not
show. Then the desktop window, in Tauri, which hosts the daemon
([0032](decisions/0032-the-interface-hosts-the-daemon.md)) and does what the
command line does: setting up with the recovery phrase shown and confirmed,
pairing by QR code, files and where their bytes are, sending, transfers,
recently deleted, settling conflicts, sharing folders with chosen devices, a
passphrase on the key. Storage caps, single-copy storage and a replica anybody
can run all landed here too, none of them planned.

Installing is a pacman package on Arch and an install script elsewhere. Signed
updates with rollback are deliberately not built
([0047](decisions/0047-versions-and-upgrades.md)): the index is copied before
it migrates, which is the rollback that matters most. Still missing: packages
for other distributions, and observability beyond a log and the activity
history. The window was designed on 2026-09-29 and awaits the owner's review.

Onboarding for a zero-knowledge product is uniquely hard: a non-technical person
must be persuaded to write down a recovery phrase *before* they have any
investment in the product, because afterwards nobody can help them. That screen
will take many drafts.

**Ship here.** Desktop-only, to a small group of real users on real hardware.

---

## Phase 5 — Mobile 🔨 Android working, iOS not started

*Months 11–16. Six months, and that assumes it goes well.*

Android first — `WorkManager` is comparatively cooperative. iOS second, and
expect it to hurt: `FileProvider` extensions run under a tight memory ceiling,
`BGTaskScheduler` grants short and unpredictable windows, and a locked phone on
cellular may not participate in a peer-to-peer mesh at all.

Plan for iOS to be a **good viewer with opportunistic sync**, not a peer equal to
desktop. Every peer-to-peer application on iOS makes this compromise; deciding it
deliberately beats discovering it in month fifteen.

**Started, against the recommendation below.** The memory-ceiling prediction was
correct and cost more than expected: the receive path held whole files, which a
FileProvider extension would not survive, and fixing it changed the engine
rather than anything mobile-specific.

The engine now runs on a real phone and syncs there — 426 tests passed on a
Galaxy S23 at the last run there, and receiving a 512 MiB file over the network
costs 6 MiB of heap. There is an Android app, with the key in the Android
Keystore, syncing in the background through WorkManager and showing its files in
the system picker. A phone and a laptop sync in both directions, verified on
hardware.

Since then it has become a **share target** — anything on the phone goes into
qurb from the system share sheet, with no network and no other device switched
on — and it can be **woken by push**, which took a change on a sleeping Galaxy
S23 from a quarter of an hour to seven hundred milliseconds, and to about five
seconds on mobile data through a rendezvous service on the laptop. The app now
also browses by folder, settles conflicts, restores deleted files, shows a
pairing code as well as scanning one, and chooses which devices have each
folder. On 2026-09-29 it was rebuilt to the same design as the desktop — four
tabs, Private Vault inside Files — and walked on the phone on 2026-10-03.

What does not exist is iOS, in any form. The prediction about `BGTaskScheduler`
is answered on the Rust side by a sync that takes a deadline, and is untested on
Apple's. The recommendation to cut mobile from year one was not taken, and the
honest accounting is that it cost roughly what the roadmap said it would — the
memory ceiling was real, and most of the work was in the engine rather than in
anything mobile-specific. See
[phases/phase-5-mobile.md](phases/phase-5-mobile.md).

---

## Phase 6 — Commercial

*Months 17–24.* Billing, passkeys, support, terms of service, regional relays,
incident response.

---

## The two risks that are not technical

**Availability.** Was the single largest open question; answered in
[0006](decisions/0006-availability-gap.md) by storage-only replicas, and built
in Phase 3. What remains is not technical: a replica has to be a machine
somebody keeps switched on, and most people do not have one.

**Key recovery.** Zero-knowledge means a lost key is unrecoverable data, and
that conversation will eventually happen with a real, distressed person. Every
consumer product in this space adds some escape hatch — social recovery, an
escrowed key, a printed kit. Decide which compromise to make now, on paper,
rather than under pressure.

Partly answered on 2026-10-05 ([0052](decisions/0052-the-key-travels-with-the-code.md)):
a new device takes the key from one it pairs with, and a phone keeps it in
Block Store, inside the phone's end-to-end encrypted Google backup. Still open: the
person whose every device is gone, with no phone backup and no words kept.

---

## Recommendation

Build Phases 0–2, about five months. At the end there is a sync engine that
provably works between two desktops — genuinely the hard technical core. After
that, the remaining risk is execution rather than existential.

**Cut mobile from year one.** It is the hardest surface, the least
differentiated, and worthless without an engine that is already trustworthy.
