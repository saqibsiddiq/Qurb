# Understanding qurb from scratch

This is the one document to read if you know nothing about this project. It
assumes no prior context — not the architecture document, not the conversations
that produced it, not familiarity with sync engines. Everything else in `docs/`
goes deeper on one topic; this file is the map.

It is a **living document**. Anything that changes how the system fits together
should be reflected here in the same piece of work that changes it.

**Last verified against the code:** 2026-09-23 — the whole file checked against
the source, not just the sections that changed. Phases 0–2 are complete.
Phase 3 is built and its kill criterion is unmeasured, for want of a second
*network*. Phase 4 has a daemon, a window with the one setting people want to
change, and no installer. Phase 5 has an Android app on a real phone that syncs
with a laptop in both directions, shares into qurb from anywhere on the phone,
and can be woken by push; iOS is untouched.

---

## 1. What this project is

qurb is a **private cloud storage system**. It aims to feel like Dropbox — you
put a file in a folder on your laptop, it appears on your phone — while being
built on the opposite premise: **your files are never stored on our servers.**

They move directly from your laptop to your phone, encrypted, over the internet.
We run servers, but they only help your devices find each other. They cannot
read your files, and in the normal case your file data never passes through
them at all.

The two goals are in tension, and that tension is the whole engineering problem:

- **Dropbox-like ease** wants a central server that is always awake, always has
  your files, and can be reached from anywhere.
- **Genuine privacy** wants no central server to hold anything.

Everything in the architecture is an attempt to get the first without giving up
the second.

---

## 2. The mental model

Five ideas carry most of the system. If you understand these, the code will make
sense.

### 2.1 Files are stored as chunks, addressed by their content

We never store "the file `/Photos/Summer.jpg`" as a unit. We split it into
pieces (**chunks**), take a cryptographic hash of each piece, and store the
piece in a directory named after that hash. This is a
**content-addressable store**, or CAS.

```
/Photos/Summer.jpg  →  [chunk A][chunk B][chunk C]
                          ↓        ↓        ↓
                       hash A   hash B   hash C
                          ↓        ↓        ↓
                   chunks/9f/86d0…  chunks/13/2f95…  chunks/a1/4b7c…
```

The filename and the data are stored separately. A database row says "this path
consists of these hashes, in this order." The chunks themselves have no idea
what file they belong to.

Two consequences follow immediately, and they are the reason for the design:

- **Deduplication is free.** If two files contain the same chunk, its hash is
  the same, so it is stored once. Copy a 4 GB video into three folders and you
  use 4 GB, not 12 GB.
- **Verification is free.** The name of a chunk *is* its hash. To check a chunk
  is intact, hash it and compare to its own filename. This is how corruption
  is detected.

#### The file in the folder is one of the places chunks live

The picture above is not the whole story, and the part it leaves out matters
more than it looks. On a device that syncs a folder, the file `Summer.jpg` is
sitting right there in `~/qurb`, holding exactly the bytes those chunks are
made of. Writing an encrypted copy of them into `chunks/` as well would make
every synced file cost **twice** its size.

So it is not written. On a device with a folder attached, the chunk store keeps
only what the folder cannot supply, and a read that finds no payload goes to the
file instead — seeking to the chunk's offset, which the index computes as the
running sum of the chunks before it. The bytes are hashed before they are
returned, so a file edited behind qurb's back makes the *old* chunks fail to
read rather than answer with the wrong content.

A store with no folder — a storage-only replica — keeps every payload, because
nothing else has them. See
[decisions/0024](decisions/0024-the-file-is-the-payload-store.md) for what this
costs, and `qurb reclaim` for freeing the duplicates an older store still holds.

#### A device may be told how much disk it can use

Set with `qurb config <dir> limit=10G`, or with the slider in the desktop app —
the same act either way, because the slider writes the same settings file the
command does, and the daemon re-reads it as it runs.

Over the limit, qurb frees space by deleting local copies of files while keeping
everything the index knows about them: path, content hash, chunk list, version.
The file leaves the folder; the file does not leave qurb, and `qurb fetch`
brings it back.

Two things about this are worth carrying in your head, because both are places
where a storage cap would otherwise destroy data.

**It refuses rather than approximates.** A file is dropped only when another
device is known to hold those exact bytes. A device that cannot free enough
stays over its limit and says so. That looks like a bug and is not: a limit is
a promise about disk, and no number in a settings box outranks the only copy of
someone's work.

**Dropping a file must not look like deleting it.** A syncing device decides a
file was deleted by not finding it. So the index records whether this device is
*holding* each file, separately from whether the file is there — set before the
unlink, never after — and both the scan and the watcher skip a file that is
missing on purpose. Without that, a device running low on disk would delete the
user's files on every other device. See
[decisions/0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md).

#### Nothing waits on the other device being awake

A file can be added on any device at any time, with every other device switched
off. There is no outbox and no retry queue: the file is written into the folder
and indexed like any other, and it reaches the others whenever one is next
reachable.

"What is still waiting to be delivered" is therefore a *question*, not a list —
asked of the index as "live files this device made, whose content no other
device is known to hold". It cannot drift from the truth, because it is read
fresh each time rather than maintained.

For that question to have an answer, a device that finishes receiving content
tells the device it got it from: `Got { content }`, the only message in the
protocol that asks for nothing. Credited to the certificate the connection
authenticated with, never to anything the message claims — a storage cap drops
local copies on the strength of that record.

A device also sends it for content it is merely *holding*, a few per sync and
each only once. Without that, anything delivered before this existed would be
counted as delivered nowhere for ever, since a file both devices already have
is never transferred again. See
[decisions/0026](decisions/0026-sharing-while-the-other-device-is-off.md).

#### Nobody waits for a poll to find out

A device that changes something tells the rendezvous service it has work for
each of its peers — who, never what. The service forwards that to peers that
are connected and **keeps it for peers that are not**, delivering it the moment
they appear.

That is the difference between a change crossing in a second and crossing at
the recipient's next scheduled attempt, which on a phone is a quarter of an
hour. Measured with two idle daemons: a file written on one was on the other
**one second later**.

It is also the input a push notification needs. The service knows both who has
work and who is absent, which is exactly the condition for waking a phone —
and the condition for *not* waking one, when the peer it would sync with is
not there either.

### 2.2 Chunk boundaries are chosen by content, not by position

The obvious way to split a file is every N bytes. Dropbox does this with 4 MB
blocks. It has a fatal weakness: **insert one byte at the front and every
boundary after it shifts**, so every chunk hash changes, so the whole file has
to be re-sent.

Instead we use **content-defined chunking** (CDC). The algorithm slides a window
over the data and cuts wherever the content itself hits a particular pattern.
Because the cut points are determined by nearby *bytes*, not by *offset*,
inserting data shifts only the chunks immediately around the insertion.

We measured this in Phase 0 on a 196 MiB file, prepending a single byte:

| method | chunks still reusable |
|---|---|
| content-defined (FastCDC) | 304 / 305 — **99.7%** |
| fixed 4 MiB blocks | 0 / 50 — **0%** |

That table is the entire justification for the extra complexity of CDC. See
[decisions/0004-chunk-parameters.md](decisions/0004-chunk-parameters.md) for how
we chose the chunk sizes, which turned out to matter more than expected.

### 2.3 Devices talk to each other directly, through firewalls

Your laptop and your phone are both behind routers doing NAT (network address
translation). Neither has a public address you can dial. Normally that means
they can only talk *through* a server in the middle.

**UDP hole punching** gets around this. Both devices ask a public server
(**STUN**) "what does my address look like from outside?", exchange those
answers through our coordination server, and then send packets to each other
simultaneously. Each outbound packet tricks the sender's own router into
accepting the reply. If it works, the devices now have a direct path and our
servers are out of the loop.

It does not always work. Some networks (**symmetric NAT**) assign a different
external port per destination, which defeats the trick. For those we fall back
to a **relay** — a server that forwards encrypted bytes without being able to
read them. Relaying costs us bandwidth, so the percentage of connections that go
direct is a number with a dollar value attached.

There is a second problem that direct connections do not solve: a device can only
send you a file if it is *switched on*. A **replica** is a device that always is,
holding content without a person using it, so the rest need not all be awake at
once. See [decisions/0006](decisions/0006-availability-gap.md).

### 2.4 There is no central truth, so devices must agree by themselves

With a central server, "what is the current version of this file?" has an easy
answer: whatever the server says. We have no such server. Each device has its
own opinion and they must converge.

We track causality with **vector clocks**: each device keeps a counter per
device, like `{laptop: 42, phone: 12}`, meaning "I have seen 42 changes from
laptop and 12 from phone." Comparing two vectors tells you whether one change
happened after another, or whether they happened *concurrently* — neither aware
of the other.

Concurrent changes are conflicts. The rule is
[decisions/0005-conflict-resolution.md](decisions/0005-conflict-resolution.md),
and its most important clause is: **never silently discard a user's edit.**
Both versions are kept; only the question of which one keeps the original
filename is decided automatically.

### 2.5 A path lives in one of two places

Everything above describes one namespace that every paired device converges on.
That is the **shared area**, and it is where a path lives unless something says
otherwise — put a file in the folder on your laptop and it appears on your
phone.

Alongside it, each device has a **private vault**. Other devices may put content
into it; nobody but the owner may list it or read it back. It is the difference
between "our files" and "here, this is for you".

In the index this is one nullable column, `files.scope`: `NULL` is the shared
area, a device id is that device's vault. In the protocol it is an
authorisation check on every request that could reveal content — the tree, a
manifest, a chunk — because a user interface that declines to draw a listing
stops nothing, and a peer can ask for content by hash without ever looking at a
listing.

Sending is the operation built on top: `qurb send <file> to <device>` puts a
file in that device's vault, the sender holds the bytes until the recipient
confirms they arrived, and the recipient files it in their own folder without
advertising it onward. See
[decisions/0029](decisions/0029-two-areas-shared-and-private.md) for the data
model and [decisions/0030](decisions/0030-sending-a-file-to-one-device.md) for
what a send promises — including the rule that a copy in somebody's vault is a
copy this device may *not* count on, which is the difference between eviction
and data loss.

### 2.6 The index remembers what happened, not just what is

Everything above describes the index as a picture of the present: these paths,
these chunks, this version. It also keeps a history — one row per thing that
happened, with the path, the size and the device at the other end.

The reason is that the interesting questions are about the past. "Why is this
file not here?" is not answerable from the current state; it is answerable from
`evicted`, or `failed`, or `conflicted`, and the log that would have said so
belongs to a process that exited. `qurb activity` reads it. See
[decisions/0031](decisions/0031-what-happened-is-written-down.md), including
what the table deliberately does *not* hold.

### 2.7 An interface is a display of the engine, not a second one

A graphical front end runs the daemon inside itself rather than talking to one
over a socket, and asks it two different kinds of question. The daemon
publishes its live state — syncing, up to date, this many devices — on a
`watch` channel, because only the latest value is ever useful. Everything else
is a read-only query against the index: what devices, what files, what is
available where, what happened, what is still on its way.

Those queries are `qurb_cli::View`, and the terminal uses the same ones
(`qurb ls`, `qurb find`, `qurb activity`). See
[decisions/0032](decisions/0032-the-interface-hosts-the-daemon.md) — including
why a file's availability has three values rather than two, which is the
difference between "free up space" and "delete my only copy".

### 2.8 Encryption happens before anything leaves the device

Chunks are compressed, then encrypted, then written to disk and sent over the
network. The keys never leave your devices. Our servers see encrypted bytes and
routing metadata, nothing else.

This is called **zero-knowledge**, and it has a hard consequence that shapes the
product: if you lose your key, we cannot recover your data. Not "we won't" — we
genuinely cannot.

That is why there is a recovery phrase — 24 words that *are* the key, in a form
a person can write on paper — and why the onboarding flow that makes people
write it down is the highest-stakes screen in the application. See
[decisions/0012](decisions/0012-key-hierarchy-and-recovery.md). Protecting that
key *at rest* was the largest security gap for a long time and is now a choice
between a file, the operating system's keystore and a passphrase — see
[crates/keys/README.md](../crates/keys/README.md) for what each defends
against.

---

## 3. How the pieces fit together

```
        YOUR DEVICES                          OUR SERVERS
  ┌─────────────────────┐              ┌──────────────────────┐
  │   Desktop           │              │  Control plane       │
  │   ┌───────────────┐ │              │  accounts, billing   │
  │   │ sync engine   │ │◄────────────►│  device registry     │
  │   │ chunk · hash  │ │   metadata   │                      │
  │   │ store · index │ │   only       │  STUN                │
  │   └───────────────┘ │              │  "what is my         │
  │   ┌───────────────┐ │              │   public address?"   │
  │   │ SQLite index  │ │              │                      │
  │   │ CAS on disk   │ │              │  Relay (DERP)        │
  │   └───────────────┘ │              │  encrypted forward   │
  └──────────┬──────────┘              │  when direct fails   │
             │                         └──────────┬───────────┘
             │  direct, encrypted (QUIC)          │
             │  ← this is the normal path         │ fallback only
             │                                    │
  ┌──────────▼──────────┐                         │
  │   Phone             │◄────────────────────────┘
  │   same engine, via  │
  │   Rust FFI          │
  └─────────────────────┘
```

The important asymmetry: **thick clients, thin servers.** Almost all the
difficulty lives on the device. The servers are a phone book and an emergency
mail forwarder.

### What happens when you save a file

This is the single most useful trace to have in your head.

```
 1. OS tells us a file changed      ✅ filesystem watcher (inotify/FSEvents/
                                       ReadDirectoryChangesW)
 2. Split into chunks               ✅ FastCDC, 128 KiB … 2 MiB
 3. Hash each chunk                 ✅ BLAKE3
 4. Ask the index: seen this hash?  ✅ SQLite lookup
      already known → record a reference, no data written
      new          → continue
 4b. Is the file itself in the      ✅ if so, it *is* the payload — record the
     synced folder?                    chunk, write nothing, skip to 8
 5. Compress                        ✅ Zstd, kept only when it actually helps
 6. Encrypt                         ✅ XChaCha20-Poly1305
 7. Write to the CAS                ✅ chunks/<first 2 hex>/<full hash>
 8. Record the file's chunk list    ✅ SQLite: path → ordered hashes
 9. Bump the vector clock           ✅ this device's counter += 1
10. Tell peers what changed         ✅ the tree, over QUIC
11. Peers request chunks they lack  ✅ only the missing ones move
12. The peer says it has it now     ✅ so this device can stop calling
                                       the file undelivered
```

Steps 1–8 are built, tested, and joined together: step 1 in
[`crates/watcher`](../crates/watcher/), steps 2–8 in
[`crates/storage`](../crates/storage/), and
[`crates/engine`](../crates/engine/) deciding what each change means and driving
the rest. A directory now syncs into a local store and stays in step with it.

All of them now run end to end between two devices over a real network
connection, including the things around the pipeline: devices pair out of band,
find each other through the rendezvous service, punch through NAT, and fall back
to a relay when no direct path exists.

What is unmeasured is *how often* the direct path works. That needs two machines
on two different networks — see
[measuring-connectivity.md](measuring-connectivity.md).

Step 11 is where the chunking finally pays off. Inserting 16 bytes at the front
of a 200 MB file moved **248 KiB** over the wire — one chunk, 0.1% of the file —
where fixed-size blocks would have re-sent all 190 MiB.

**How the other device finds out.** Steps 10 and 11 need both devices reachable
at the same moment, and a phone is awake only in short bursts. The rendezvous
service therefore *pushes*: when a device announces itself, everyone else in its
group is told at once, with addresses attached. A peer that had to poll would
not be asking during the twenty seconds a phone is up — see
[decisions/0022](decisions/0022-the-service-announces-arrivals.md).

**Ordering matters at step 7 and 8, and not in the obvious way.** The payload is
written and fsynced *before* the index records that it exists. A crash between
them leaves a chunk nothing references — wasted space, reclaimed later. The
opposite order would leave the index pointing at a payload that was never
written, which no amount of local repair can fix. Orphaned data is a cost; a
dangling reference is a corruption.

Steps 2–4 are why a one-byte edit to a large file costs kilobytes instead of
gigabytes. Step 4 is why storing the same photo twice costs nothing.

---

## 4. Where things live

```
qurb/
├── README.md              Start here — what this is, current status
├── CLAUDE.md              Conventions and working agreements
├── Cargo.toml             Rust workspace root; shared dependency versions
│
├── docs/
│   ├── CODEBASE.md        ← you are here
│   ├── product-plan.md    Turning the engine into a product, and the one
│   │                      decision that blocks it
│   ├── glossary.md        Every term, defined plainly
│   ├── trying-it.md       Running it yourself, from one machine to a phone
│   ├── architecture.md    The target design, all subsystems
│   ├── roadmap.md         Phases, timelines, honest risk assessment
│   ├── decisions/         Why each choice was made (one file per decision)
│   └── phases/            What each phase produced, with measurements
│
├── crates/
│   ├── storage/           Local storage. The foundation everything sits on.
│   │   └── src/
│   │       ├── chunker.rs   split at content-defined boundaries, hash
│   │       ├── format.rs    compress, then encrypt (the on-disk format)
│   │       ├── cas.rs       payloads to and from chunks/<2 hex>/<full hex>
│   │       ├── db.rs        SQLite index; reference counts live here
│   │       ├── gc.rs        reclaim chunks nothing references
│   │       └── store.rs     public API and the ordering rules
│   │
│   ├── watcher/           Turns filesystem events into settled changes.
│   │   └── src/
│   │       ├── ignore.rs    what never to watch (our own store, VCS, scratch)
│   │       ├── debounce.rs  collapse bursts; pure, takes the clock as input
│   │       ├── scan.rs      full walk, for startup and after overflow
│   │       └── watcher.rs   native events wired to the above
│   │
│   ├── engine/            Decides what a change means, and does it.
│   │   ├── src/lib.rs       reconcile, apply, and the run loop
│   │   ├── src/role.rs      syncing device, or storage-only replica
│   │   ├── src/repair.rs    refetch chunks the disk damaged
│   │   ├── src/peer.rs      compare with another device and act on it
│   │   └── examples/        sync_once: one directory into a store
│   │                        sync_pair: two directories against each other
│   │
│   ├── sync/              What to do when two devices disagree.
│   │   └── src/           Pure logic: no disk, no network, no clock.
│   │       ├── clock.rs     version vectors and their partial order
│   │       ├── version.rs   one device's view of one path
│   │       ├── resolve.rs   deciding between two versions
│   │       └── reconcile.rs deciding about a whole tree
│   │
│   ├── peer/              Reaching another device, over QUIC.
│   │   ├── src/wire.rs      the message format; bounded and hostile-input safe
│   │   ├── src/identity.rs  a device's certificate and its fingerprint
│   │   ├── src/pairing.rs   deciding which device to trust in the first place
│   │   ├── src/base32.rs    invite encoding, chosen for how QR codes work
│   │   ├── src/nat.rs       STUN, NAT classification, hole punching
│   │   ├── src/connect.rs   the policy: discover, announce, race candidates
│   │   ├── src/tls.rs       mutual authentication by pinned fingerprint
│   │   ├── src/server.rs    serves a store, read-only
│   │   ├── src/client.rs    asks for trees, manifests, chunks
│   │   └── src/source.rs    plugs the client into the engine
│   │
│   ├── mobile-ffi/        The engine, as a phone can call it.
│   │   └── src/lib.rs       UniFFI surface: files, pairing, bounded sync
│   │
│   ├── keys/              The root secret and the way back to it.
│   │   ├── src/master.rs    HKDF derivation, one key per purpose
│   │   ├── src/phrase.rs    the 24 words, via BIP-39
│   │   └── src/vault.rs     where the master key lives, and its limits
│   │
│   ├── signal/            Finding the other device.
│   │   ├── src/rendezvous.rs  identifiers the server cannot link to anyone
│   │   ├── src/message.rs     what is said; carries nothing about files
│   │   ├── src/server.rs      holds a channel open per device
│   │   ├── src/client.rs      announce, ask, punch when told
│   │   ├── src/wake.rs        the seam: how an absent device gets poked
│   │   └── src/fcm.rs         that seam, filled in by Firebase (feature `push`)
│   │
│   ├── relay/             The fallback when no direct path exists.
│   │   ├── src/frame.rs      opaque forwarding, binary and bounded
│   │   ├── src/socket.rs     a relay connection pretending to be a UDP socket
│   │   └── src/server.rs     forwards between registered identifiers
│   │
│   ├── qurb/              The program a person runs.
│   │   ├── src/lib.rs       the daemon, as a library, so an interface can
│   │   │                    run the same one the terminal does
│   │   ├── src/main.rs      init, enrol, pair, join, run, replica, status,
│   │   │                    verify, reclaim, fetch, send, activity, ls, find,
│   │   │                    config, protect
│   │   ├── src/daemon.rs    watch, apply, sync, collect, stay under the limit
│   │   ├── src/lock.rs      one daemon per folder, enforced not assumed
│   │   ├── src/profiles.rs  which folders exist, so commands need no path
│   │   ├── src/qr.rs        a pairing code a camera can read
│   │   ├── src/status.rs    what the daemon is doing now, on a watch channel
│   │   ├── src/view.rs      what an interface asks: devices, files, storage,
│   │   │                    history, outgoing, search — all read-only
│   │   ├── src/setup.rs     creating a device, joining one, describing a folder
│   │   │                    — one definition, used by the terminal and the window
│   │   └── src/config.rs    a flat file meant to be edited by hand
│   │
│   ├── desktop/           The desktop application: the daemon in a window.
│   │   ├── src/main.rs      opens the window, and the daemon if there is one
│   │   ├── src/session.rs   unmade or running, and the phrase in between
│   │   ├── src/commands.rs  every question the window may ask
│   │   └── ui/              the screens: HTML, one stylesheet, one script
│   │
│   └── tray/              An icon in the corner: the daemon with a face.
│       ├── src/host.rs      whether a tray icon would be visible at all
│       ├── src/icon.rs      the icon, drawn rather than shipped
│       ├── src/ui.rs        the menu, and what to do when there is no tray
│       └── src/window.rs    the window: status, and the storage slider
│
├── android/               The Android app. Kotlin over the FFI, no sync logic.
│   └── app/src/
│       ├── main/java/com/qurb/
│       │                  MainActivity.kt     what is here, and a Sync button
│       │                  SetupActivity.kt    the 24 words, once
│       │                  ScanActivity.kt     reading a pairing QR code
│       │                  ShareActivity.kt    the share sheet's way in
│       │                  AndroidKeyStore.kt  the platform half of decision 0021
│       │                  SyncWorker.kt       background sync, on WorkManager
│       │                  QurbDocumentsProvider.kt
│       │                                      the files, in the system picker
│       ├── push/java/     being woken by Firebase — compiled only when a
│       │                  google-services.json is present
│       └── nopush/java/   the same surface, doing nothing, when it is not
│
├── packaging/             Getting it onto a machine.
│   ├── install.sh         qurb in this user's applications menu
│   ├── qurb.desktop       the launcher entry
│   └── server/            systemd units and TLS for a host of your own
│
├── scripts/
│   ├── android-app.sh     build the app: libraries, bindings, then Gradle
│   ├── android-build.sh   cross-compile the engine for all four Android ABIs
│   ├── android-test.sh    run the test suite on a device, over adb
│   └── mobile-bindings.sh generate the Kotlin and Swift bindings
│
├── experiments/
│   ├── desktop-fixtures/  Throwaway. The window's screens against made-up
│   │                      data, so layout can be worked on with no daemon.
│   └── phase0-spike/      Throwaway. Proved the core ideas work.
│       └── src/
│           ├── lib.rs            chunker + content-addressable store
│           └── bin/
│               ├── chunkbench.rs measures the five Phase 0 kill criteria
│               ├── sweep.rs      compares chunk size configurations
│               └── quictest.rs   NAT classification + QUIC throughput
│
└── website/               The landing page. Next.js, and entirely separate —
                           it shares a repository with the engine and nothing
                           else. Nothing here depends on it or is built by it.
```

**The `crates/` vs `experiments/` split is load-bearing.** Anything in
`experiments/` is disposable and is allowed to cut corners, as long as its
README says which corners. Anything in `crates/` is meant to last and is held to
a real standard. Nothing should quietly migrate from one to the other — Phase 1
code gets written fresh, informed by the spike rather than copied from it.

---

## 5. What actually exists right now

Being precise about this matters, because the architecture document describes a
complete system and a good deal of it is still unbuilt. The engine is real; the
product around it largely is not.

### Built and tested (`crates/storage`, Phase 1)

| thing | status |
|---|---|
| Content-defined chunking | FastCDC, 128 KiB / 512 KiB / 2 MiB |
| Compression and encryption at rest | Zstd then XChaCha20-Poly1305 |
| Content-addressable store | crash-safe writes via temp file + rename |
| SQLite index | paths, chunk lists, reference counts via triggers |
| Deduplication | within a file, across files, across versions |
| Single-copy storage | a materialised file *is* its own payload store |
| Deletion, tombstones, restore | content survives a retention window |
| Garbage collection | two-stage, never touches a referenced chunk; the daemon runs it every five minutes |
| Integrity verification | detects missing, corrupt, and orphaned chunks |
| Reclaiming duplicates | `qurb reclaim`, for stores written before single-copy |
| A storage limit | drops local copies, keeps the index, never the only copy |
| Per-device private vaults | `files.scope`: `NULL` is shared, a device id is that device's vault |
| A history of what happened | one table, pruned by age and count; `qurb activity` reads it |
| Sending to one device | `qurb send <file> to <device>`; held until collected, released first afterwards |

### Built and tested (`crates/watcher`, Phase 1)

| thing | status |
|---|---|
| Native filesystem events | inotify / FSEvents / ReadDirectoryChangesW |
| Debouncing | collapses editor write-bursts into one change |
| Stability checking | never releases a file that is still being written |
| Ignore rules | our own store, VCS metadata, editor scratch files |
| Overflow handling | dropped events demand a full rescan |
| New-directory walk | closes the recursive-watch race that loses files |

### Built and tested (`crates/engine`, Phase 1)

| thing | status |
|---|---|
| Startup reconciliation | walks the tree, agrees it with the index |
| Size and mtime fast path | 2437 files: 12.96s first run, 0.03s second |
| Applying watcher changes | upserts, removals, vanished files |
| Directory removal | tombstones everything the index holds beneath a path |
| Error isolation | one unreadable file does not stop the rest |
| Overflow recovery | a dropped-event rescan re-reconciles everything |

### Built and tested (`crates/sync`, Phase 1)

| thing | status |
|---|---|
| Version vectors | partial order, merge, deterministic encoding |
| Conflict detection | by causality, never by wall-clock time |
| Conflict resolution | both versions kept; a concurrent edit beats a delete |
| Deliveries | vault entries are collected once, not reconciled |
| Tree reconciliation | plans describing end states, not decisions |
| Convergence | two and three devices, 320 random seeds |

### Joined together

| thing | status |
|---|---|
| Version vectors persisted | schema v2: vector, author, device identity |
| Local changes stamped | a counter per device, advanced only on real change |
| Comparing with a peer | `tree`, `plan_against`, `apply_plan` |
| Content fetched by hash | renames and copies cost a lookup, not a transfer |
| Two directories converging | including conflicts, deletions, resurrections |

Those four crates were the first to be joined together; the counts below are
for the workspace as it stands.

### Built and tested (`crates/peer`, Phase 1)

| thing | status |
|---|---|
| QUIC transport | one bidirectional stream per request |
| Mutual authentication | pinned fingerprints, handshake signature verified |
| Wire format | length-bounded; decoder has no panicking path |
| Incremental transfer | only chunks the receiver lacks cross the wire |
| Read-only serving | a peer can ask, never tell — with one exception below |
| Vault authorisation | tree, manifest and chunk requests all check the asker's scope |
| Delivery reports | `Got`: the receiver says it holds it, so the sender can stop calling it undelivered |
| Every reachable address offered | LAN, overlay network and public, raced in parallel |
| Signalling that reconnects | a rendezvous restart costs seconds, not a daemon restart |

### Built and tested (`crates/keys`, Phase 1)

| thing | status |
|---|---|
| Master key | 256-bit, from the OS CSPRNG |
| Key derivation | HKDF-SHA256, one key per purpose, versioned labels |
| Recovery phrase | 24 words, BIP-39, checksummed |
| Recovery, end to end | the phrase turns back into the user's files |
| Key hygiene | redacted in `Debug`, wiped on drop, owner-only on disk |

549 tests pass across twelve crates on Linux; clippy is clean. The last run on
a Galaxy S23 was 426 of them, before this week's work — see
[phases/phase-5-mobile.md](phases/phase-5-mobile.md).

**The wire protocol is `qurb/1`.** It was `qurb/0` until tree entries gained a
flag saying "this belongs in your vault", which is not a byte an older build can
safely ignore — it would adopt somebody else's private content as shared and
advertise it to the whole fleet. Devices negotiate it during the TLS handshake,
so a mismatch is a clean refusal to connect. Every device has to be rebuilt
together.

**Two devices now sync over a real network connection**, converging through
concurrent edits, deletions and resurrections, with both sides computing the
same conflict filename independently, using keys derived from a recovery phrase.

**Tested at 100,000 files** — 4.40 GiB between two devices, every correctness
check green: all paths present on both sides, reference counts agreeing, every
chunk decrypting and hashing to its own name, sampled files byte-identical. A
warm re-index of those 100k files takes 1.13s and an incremental sync of 1,000
edited files takes 3.5s. A cold index takes 237s, which is slow — 58% of it is
filesystem syscalls, and the fix is parallelism, which nothing in the design
prevents. See [phases/phase-1-engine.md](phases/phase-1-engine.md).

**Devices now pair.** An invite carries the inviter's full fingerprint across an
out-of-band channel — a QR code, or a code read aloud — and both sides record the
other in a trust store binding device identity to network identity. A listener
takes its guest list from there, so the system has a notion of "my devices"
rather than a list the caller assembled.

**Devices can also get through a router.** STUN discovers what address this
machine looks like from outside, and hole punching opens a path — on the same
socket throughout, because a router's mapping belongs to one local port and
using a fresh socket would punch a hole nobody is listening behind.

**A device can also be a storage-only replica** — always on, holding content so
the others need not all be awake at once, materialising nothing and originating
nothing. That is the answer to the availability gap that had been open since
Phase 1; see [decisions/0006](decisions/0006-availability-gap.md).

**A rendezvous service now coordinates them.** Devices announce where they are
and are told to punch at the same moment — which is what hole punching needs and
what a request-and-response API cannot arrange. It learns no filenames and
cannot link a group of devices to a person; see
[decisions/0016](decisions/0016-what-signalling-learns.md).

**And a connection policy joins them up.** `Connector` binds a socket, discovers
its public address, announces, asks to be introduced, and races every address the
peer offered — keeping the first that answers, local addresses first. Both sides
dial when told to, because a QUIC handshake's opening packets *are* the hole
punch and a device that only listens has punched nothing.

**And a relay carries what cannot go directly.** It forwards opaque datagrams
with an ordinary QUIC session running inside, so it sees ciphertext addressed to
an identifier it cannot link to a person. It is TCP, on port 443, because it
exists for networks where UDP does not work.

**And the two are joined.** Reaching a peer tries every direct address at once
and falls back to the relay when none answers, with identity pinned exactly as
hard either way.

**And there is now a program to run.** `qurb init`, `pair`, `join` and `run`
turn all of the above into a daemon that watches a directory and syncs with the
devices it has been paired with; `qurb signal` and `qurb relay` run the services.

**What has never been measured is how often that fallback is needed.** The
direct-connection rate on real networks is the number the relay bill depends on.
Until now it could not be measured because there was nothing to run on a second
machine; now there is, and
[measuring-connectivity.md](measuring-connectivity.md) says how.

**And the engine cross-compiles for a phone.** All four Android architectures
build, and [`crates/mobile-ffi`](../crates/mobile-ffi/) generates the Kotlin and
Swift a phone calls. What that became is the Android section below.

### Hardened (Phase 2, complete)

| thing | status |
|---|---|
| Property-based convergence | 21 properties, shrinking, persisted regressions |
| Crash injection | real `SIGKILL` at seven points; no dangling references |
| Clock skew | ±10 years changes nothing about who wins |
| Long-absent devices | a month offline, in both directions |
| Case-insensitive collisions | probed, reported, and refused |
| Corruption repair | damaged chunks refetched from a peer and verified |
| Large renames | free in both sort directions; empty directories pruned |
| Hostile peers | wrong bytes, nonsense, silence — none reaches disk |
| Concurrent collection | the collector runs against a live writer |

The phase found five real defects, all of the same shape — correct in isolation,
wrong in combination — living in the seams between components rather than inside
them. See [phases/phase-2-correctness.md](phases/phase-2-correctness.md).

**A directory now syncs into a local store**, and can be tried:

```bash
cargo run --release --example sync_once -- ~/Documents /tmp/qurb-store
```

Two directories, treated as two devices:

```bash
cargo run --release --example sync_pair -- /tmp/dev-a /tmp/dev-b
```

The same, but over a real QUIC connection, reporting what crossed the wire.
Device A creates a key and B enrols with its recovery phrase:

```bash
cargo run --release --example transfer -- /tmp/dev-a /tmp/dev-b
```

What enrolling a device looks like on its own:

```bash
cargo run -p qurb-keys --example enrol -- /tmp/device-a
```

The Phase 1 kill criterion — generate a tree, sync it, verify it, edit it, sync
again. Needs roughly 20 GiB free at 100k files:

```bash
cargo run --release -p qurb-peer --example scale -- /tmp/scale 100000
```

**The whole system at once.** Starts a rendezvous service and a relay, brings up
two devices in the directories given, pairs them, connects them, and then keeps
running — drop a file into either directory and watch it appear in the other:

```bash
cargo run --release -p qurb-peer --example demo -- /tmp/device-a /tmp/device-b
```

Everything is in one process, which is the one thing about it that is not
realistic. The sockets, handshakes, encryption, chunking and conflict resolution
are all the real implementations.

What is missing is everything a *person* needs: an interface, installers,
updates, and an app on either phone.

### Measured in the Phase 0 spike

| thing | result |
|---|---|
| Chunking throughput | 665 MiB/s warm, 246 MiB/s cold — disk-bound, not CPU-bound |
| Boundary stability | 99.7% chunk reuse after an edit, vs 0% for fixed blocks |
| Round-trip integrity | 2722 real files, byte-exact |
| QUIC transport | 287 MiB/s loopback |
| NAT classification | home network is cone NAT; more networks still to test |

### Running on Android (Phase 5, in progress)

The engine cross-compiles for all four Android architectures, and
[`crates/mobile-ffi`](../crates/mobile-ffi/) gives it a surface a phone can
call, generating Kotlin and Swift from the Rust. A phone pairs out of band,
finds the other device through the rendezvous service, connects over QUIC and
syncs — all through that surface, with `sync_within(seconds)` because both
platforms kill background work that outstays its window. The master key can be
handed to the platform's own keystore, which the app supplies because neither
Android's nor iOS's is reachable from Rust.

The Android app is a **share target**: anything on the phone can be sent into
qurb from the system share sheet, with no network and no other device switched
on. It can also be **woken** when another device has something, if a push
service is configured — without one it learns at its next scheduled look, about
fifteen minutes away. See
[decisions/0028](decisions/0028-waking-a-sleeping-device.md), which sets out
what that costs and why nothing else works. The app's own screen and the share sheet's confirmation both say how many
files are still held only by the phone, which is the honest form of "it will
get there".

**It runs on a phone.** `./scripts/android-test.sh` pushes the test binaries
with `adb` and runs them: on a Samsung Galaxy S23 (Android 16, arm64-v8a) all 35
pass — 426 tests, including the real QUIC handshakes and hole punching.
Receiving a 512 MiB file over the network there grows the heap by 6 MiB.

Three problems mobile exposed were fixed in the core, because all three were
core problems that a desktop merely tolerates:

- **Whole files no longer pass through memory.** Adopting a 1 GiB file grew the
  heap by 1024 MiB and now grows it by 1. An iOS FileProvider extension is
  killed at a ceiling in the tens of megabytes, so the old path worked for small
  files and killed the process for large ones.
- **Filenames are normalised to NFC.** macOS and iOS decompose names on the way
  in, which made a synced `café` look deleted-and-recreated on every scan and
  duplicated it without limit.
- **`Connector::start` announced `0.0.0.0`** — true about a socket bound to
  every interface, and useless to a peer. Hidden until now because every test
  binds `127.0.0.1` explicitly and STUN normally supplies an address that works
  instead; it broke two devices on a network with no route to the internet.

**There is an Android app** — [`android/`](../android/) — which installs, sets up
an identity, keeps the key in the Android Keystore, lists files, pairs and
syncs. Building it found a bug nothing else could: UniFFI keeps only the *last*
`#[uniffi::export] impl` block for an object and silently discards the others,
so eight methods were missing from the generated Kotlin and Swift while every
Rust test passed.

The app syncs on its own through WorkManager, every fifteen minutes when
Android allows it, and a `DocumentsProvider` puts the synced files in the
system file picker and the Files app.

**A real phone and a real laptop sync both ways**, verified on hardware: a
4.7 MB photo crossed from a Galaxy S23 to a laptop, byte-identical by SHA-256.
Getting there found five more defects that no test could have caught, because
each needed two machines and a router — most importantly that the rendezvous
service knew the moment a device appeared and told nobody, which made sync
one-directional in practice while looking symmetrical in design
([decision 0022](decisions/0022-the-service-announces-arrivals.md)).

**Unwatched behaviour and iOS are what remain.** The background worker is
scheduled and was verified through WorkManager, but a phone left alone for a
day — syncing on Android's timetable, at some cost in battery — has never been
observed, and everything verified so far was on one phone, one laptop and one
network. iOS needs Xcode, which needs a Mac. See
[phases/phase-5-mobile.md](phases/phase-5-mobile.md).

### Built and running (`crates/desktop`, Phase 4)

| thing | status |
|---|---|
| A window | Tauri 2, five screens, no framework and no build step |
| It hosts the daemon | the same one `qurb run` starts, on its own threads |
| Home | live state, recent files, what is still on its way |
| Files | listing, paging, search, and three-way availability |
| Devices | who is paired, when each was last reached |
| Activity | what this device did, paged, with the reason where there is one |
| Storage | usage, the allowance, and a control that can change it |
| Settings | name, rendezvous, relay, port, and the 24 words again |
| Setting a device up | make a new one or join an existing, with the phrase shown and confirmed |
| Pairing, sending, progress | **not built** — see the crate's README |

### Designed but not built

An iOS app, per-file keys, key rotation, relay selection and quotas, accounts
and billing, installers and updates.

The desktop interface is partly built rather than absent: a window showing what
the daemon is doing, with a slider for how much disk it may use. What it cannot
do is pair a device, browse what is synced, or recover a deleted file — and the
onboarding that asks somebody to write down 24 words is still a terminal.

Selective sync is half-built rather than unbuilt: a device drops local copies
when it is over its storage limit and fetches them back on request, which is
the mechanism. What is missing is the *choosing* — a person saying which
folders they want kept locally, rather than the cap deciding by what is
coldest.

### The gaps that matter most

Five things are known-missing rather than merely unbuilt:

1. **An evicted file simply vanishes from the folder on Linux.** Windows and
   macOS both have an API for a placeholder that keeps its name and size and
   fetches when opened; Linux has nothing short of a FUSE mount, whose failure
   would take the user's whole folder with it. So a file dropped for the
   storage cap is absent, and `qurb status` is the only place that says it
   still exists. See
   [decisions/0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md).

2. **Key recovery.** Zero-knowledge means a lost key is lost data. Every
   consumer product in this space eventually adds some escape hatch — social
   recovery, an escrowed key, a printed kit — and each trades away part of the
   promise. Choosing which compromise to make is better done on paper now than
   under pressure from an upset user later. Still undecided.

3. **Nobody has watched a phone sync for a day.** The background worker is
   scheduled and runs when asked; what Android actually grants it over a day,
   and what that costs in battery, is unmeasured. Everything verified on
   hardware so far was one phone and one laptop on one home network.

4. **Two kill criteria remain unmeasured**, both for want of hardware rather
   than for want of code: Phase 3's direct-connection rate needs a second
   machine on a different network, and Phase 5's battery-and-survival test needs
   a real phone. An emulator answers neither.

5. **A replica cannot free space under a storage cap.** Eviction works by
   deleting a file from a folder, and a replica has no folder — so a cap on one
   reports the overrun rather than acting on it. Dropping chunk payloads is a
   different operation and is not written. It matters for the small always-on
   box a replica is most useful on. See
   [decisions/0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md).

Three earlier entries here have since been closed, and how they were closed is
worth knowing:

**Chunk garbage collection** was written first in Phase 1, because a collector
that is even slightly wrong destroys data in files the user never touched and
raises no error doing it. See
[phases/phase-1-engine.md](phases/phase-1-engine.md). Writing it was not the
same as running it: until the storage cap went in, `Store::gc` had no caller
outside tests, and superseded chunks accumulated without limit. The daemon now
collects on a five-minute timer with a seven-day retention window, before it
checks the limit.

**Availability** — files being unreachable when every device is switched off —
is answered by storage-only replicas, decided in
[decisions/0006](decisions/0006-availability-gap.md), built in Phase 3 and
runnable since `qurb replica`. Verified with the two ordinary devices never
running at the same time: see [anywhere.md](anywhere.md).

**Protecting the key at rest** was the largest security gap and is now a choice
between a file, the operating system's keystore, and a passphrase — see
[crates/keys/README.md](../crates/keys/README.md) for what each defends against.

---

## 6. Running things

A step-by-step guide for trying it on real hardware, including an Android phone,
is in [trying-it.md](trying-it.md). What follows is the reference.

```bash
cargo build --release
```

**The program itself.** Two devices, start to finish — `init` on the first,
`enrol` on the second with the phrase it printed, then `pair` and `join` to
introduce them, then `run` on both:

```bash
./target/release/qurb init ~/Sync
```

`qurb` with no arguments lists the rest. See
[crates/qurb/README.md](../crates/qurb/README.md).

```bash
# Free the duplicate payloads a store written before single-copy still holds.
# Safe to interrupt, and a no-op on a store that has no folder attached.
./target/release/qurb reclaim ~/Sync
```

```bash
# Bound how much disk this folder may use. 0, the default, means no limit.
./target/release/qurb config ~/Sync limit=10G
```

```bash
# Ask for a file whose local copy was dropped. Acted on when a peer is next
# reachable, so it works while offline.
./target/release/qurb fetch ~/Sync holiday/beach.jpg
```

```bash
# What this folder holds and whether the bytes are actually here. "only here"
# means no other device has it — losing this device would lose the file.
./target/release/qurb ls ~/Sync
./target/release/qurb ls ~/Sync photos
```

```bash
# Files whose name contains something. Names only, not contents.
./target/release/qurb find ~/Sync beach
```

```bash
# What this device did, newest first. With a path, what happened to that file —
# which is the question a log cannot answer once the process has exited.
./target/release/qurb activity ~/Sync
./target/release/qurb activity ~/Sync holiday/beach.jpg
```

```bash
# Send a file to one device and to nobody else. The bytes stay here until that
# device confirms it has them, so it works while the recipient is switched off.
./target/release/qurb send ~/Sync ~/Downloads/tickets.pdf to phone
```

```bash
# A device that holds content so the others need not all be awake at once.
# No folder, no files shown to anybody, nothing materialised.
./target/release/qurb enrol /srv/qurb "<the same 24 words>"
./target/release/qurb replica /srv/qurb
```

```bash
# The two services. `--push` needs a build with `--features push` and a
# Firebase service account; without it devices sync when they next look.
./target/release/qurb signal 127.0.0.1:9000 --push /etc/qurb/firebase.json
./target/release/qurb relay 0.0.0.0:9001
```

```bash
# the test suites -- start here to see what each layer guarantees
cargo test --workspace
```

```bash
# Measure chunking, dedup, round-trip integrity, boundary stability
./target/release/chunkbench ~/Downloads
```

```bash
# Compare chunk size configurations against a real corpus
./target/release/sweep ~/Downloads
```

```bash
# Classify this network's NAT — run on every network you care about
./target/release/quictest stun
```

```bash
# Cross-compile the engine for all four Android architectures
./scripts/android-build.sh --release
```

```bash
# Generate the Kotlin and Swift a phone would call
./scripts/mobile-bindings.sh
```

```bash
# Run the test suite on a connected Android device or emulator
./scripts/android-test.sh x86_64
```

```bash
# Build the app, and put it on a connected phone
./scripts/android-app.sh install
```

```bash
# The desktop application: the same daemon, in a window. Five screens over the
# same queries `qurb ls`, `qurb find` and `qurb activity` use.
cargo build --release -p qurb-desktop
./target/release/qurb-desktop ~/Sync
```

```bash
# The tray icon: the same daemon, smaller. Falls back to a window where there
# is no system tray, which on GNOME is always.
cargo run --release -p qurb-tray -- ~/qurb
```

```bash
# Put it in the applications menu for this user. --uninstall undoes it.
./packaging/install.sh
```

Syncing from outside the house needs the rendezvous service somewhere both
devices can reach; [anywhere.md](anywhere.md) is the recipe, including a free
one. Only that service needs a public name — the files go directly between the
devices and never touch it. To run the services on a host of your own, unit
files and the step-by-step are in
[packaging/server/](../packaging/server/README.md).

The Android build needs the NDK, because SQLite is C. The script looks for one
and says where to get it if there is none. Nothing else in the tree needs a
cross-compiler.

`quictest stun` contacts public STUN servers, which reveals your public IP to
them exactly as any VPN or video-call client does. Nothing else in the spike
touches the network.

Set `SPIKE_SCRATCH` to a disk-backed path before running `chunkbench` with no
arguments. It defaults to `/tmp`, which on many Linux systems is a RAM-backed
tmpfs that the 2 GiB synthetic corpus will fill.

---

## 7. Suggested reading order

1. This file.
2. [glossary.md](glossary.md) — skim it, then use it as a reference.
3. [roadmap.md](roadmap.md) — what is being built when, and the honest risks.
4. [phases/phase-0-spike.md](phases/phase-0-spike.md) — the measurements, and
   the design error they caught.
5. [architecture.md](architecture.md) — the full target design.
6. [decisions/](decisions/) — read these when you want to know *why*, or when
   you are about to change something and want to know what it would break.
7. [crates/qurb/README.md](../crates/qurb/README.md) — the commands, and what
   the daemon does not do yet. The quickest way to see the shape of the whole
   thing is to run it.

Then the layers, bottom to top:

8. [crates/storage/README.md](../crates/storage/README.md) — the two invariants
   the storage layer is built around. Then its source, in this order:
   `store.rs`, `db.rs`, `gc.rs`.
9. [crates/watcher/README.md](../crates/watcher/README.md) — the four silent
   failure modes filesystem watching has to prevent.
10. [crates/sync/README.md](../crates/sync/README.md) — why concurrency means
    conflict, and why convergence is a different property from correctness.
11. [crates/engine/README.md](../crates/engine/README.md) — how a change becomes
    work, and the one heuristic the engine leans on.
12. [crates/keys/README.md](../crates/keys/README.md) — why a lost phrase is
    unrecoverable, and what the key file does and does not defend against.
13. [crates/peer/README.md](../crates/peer/README.md) — what actually crosses
    the wire, and what pinned identity does and does not protect.
14. [crates/signal/README.md](../crates/signal/README.md) and
    [crates/relay/README.md](../crates/relay/README.md) — the two services, and
    what each is deliberately unable to learn.
15. [crates/mobile-ffi/README.md](../crates/mobile-ffi/README.md) — the surface
    a phone calls, and the four platform constraints that shaped it.
16. [android/README.md](../android/README.md) — the app, and the three things
    about Android that dictated its shape: the keystore, the 16 KB page size,
    and a background scheduler that decides when you run.

And when you want to run it for real, rather than on one machine:

17. [anywhere.md](anywhere.md) — what has to be reachable for a phone to sync
    from a train, what does not, and a way to get there for nothing.
18. [packaging/server/README.md](../packaging/server/README.md) — the two
    services on a host of your own: unit files, ports, and which of them may
    face the internet.

And when you want to close the measurements still outstanding:

19. [measuring-connectivity.md](measuring-connectivity.md) — how to measure the
    direct-connection rate, which is the number the relay bill depends on. The
    other open measurement, whether sync survives a phone's battery and its
    platform's patience, needs a device — see
    [phases/phase-5-mobile.md](phases/phase-5-mobile.md).

---

## 8. Conventions

- **Rust** for anything on a device: engine, storage, crypto, networking. That
  includes the phones — [`crates/mobile-ffi`](../crates/mobile-ffi/) is the
  only place platform languages appear, and it is a seam rather than a second
  implementation.
- **Go** for cloud services, when they exist.
- Decisions go in `docs/decisions/`, numbered, never deleted. If a decision is
  reversed, the old file gets a status line pointing at its replacement. The
  record of what we believed and why is worth more than a tidy directory.
- Every phase gets a document in `docs/phases/` recording what it produced and
  what it measured, written as part of the phase rather than after it.
- Claims about performance carry the measurement or they are not made. "Fast"
  is not a specification.

---

## 9. Keeping this file honest

The failure mode for a document like this is drifting out of sync with the code
until it becomes actively misleading — worse than having no document at all.

Update it whenever you change how the system fits together: a new subsystem, a
changed data flow, a moved directory, a corrected assumption. Update the
"last verified" date at the top when you have actually checked the whole file
against the code rather than just edited one section.

Do not update it for routine implementation work that does not change the
shape of the system. This is a map, not a changelog.
