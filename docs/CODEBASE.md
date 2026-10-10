# Understanding qurb from scratch

This is the one document to read if you know nothing about this project. It
assumes no prior context — not the architecture document, not the conversations
that produced it, not familiarity with sync engines. Everything else in `docs/`
goes deeper on one topic; this file is the map.

It is a **living document**. Anything that changes how the system fits together
should be reflected here in the same piece of work that changes it.

**Last verified against the code:** 2026-10-08 — the whole file checked against
the source, not just the sections that changed. Phases 0–2 are complete.
Phase 3 is built and its kill criterion is unmeasured, for want of a second
*network*. Phase 4 has a daemon, a desktop window that does everything the
command line does — setting up, pairing, sending, recently deleted, conflicts,
sharing folders with chosen devices, the passphrase — and an Arch package; no
automatic updater, by decision. Phase 5 has an Android app on a real phone that
syncs with a laptop in both directions, shares into qurb from anywhere on the
phone, and is woken by push; iOS is untouched. Both apps were rebuilt to the
owner's design direction on 2026-09-29, and given a dark mode on 2026-10-08
([0056](decisions/0056-dark-mode.md)); the Android app's places were walked on
a Galaxy S23 on 2026-10-03. What comes
next, in the owner's order: the owner's review of the design, then the relay on
a server of the owner's own, then a formal release for Linux and Android.
[features.md](features.md) lists everything that exists, by where a person
meets it.

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

A handful of ideas carry most of the system. If you understand these, the code
will make sense.

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

#### A deletion keeps a copy for thirty days

Deleting a file — here, or on another device and synced — moves the file out
of the folder into `.qurb/trash/` rather than unlinking it, and the `trash`
table remembers where it came from. For thirty days it can be restored, which
writes it back as a *new version* in the area it was deleted from, so a shared
file returns on every device, not only this one, and a vault file returns to
the vault. A sharing rule deleted elsewhere is the exception: it is removed,
not kept, since restoring it would undo somebody's change to who has a
folder. After that it goes, and a device over its storage limit empties the
trash before it evicts anything. The same place receives the version a person
did not keep when settling a conflict. See
[decisions/0042](decisions/0042-recently-deleted.md).

#### Nothing waits on the other device being awake

A file can be added on any device at any time, with every other device switched
off. There is no outbox and no retry queue: the file is written into the folder
and indexed like any other, and it reaches the others whenever one is next
reachable.

"What is still waiting to be delivered" is therefore a *question*, not a list —
asked of the index as "live files this device made, whose content no other
device is known to hold". It cannot drift from the truth, because it is read
fresh each time rather than maintained.

A device asked for content it does not hold says so before any bytes move. A
manifest is given only for content whose every chunk is in the chunk store or
in a live file in the folder. The asker then lists the file as elsewhere,
rather than failing at the first chunk on every sync, which a phone did for 18
files nobody had any more. A failure that repeats is recorded in the history
once. See [decisions/0054](decisions/0054-a-file-the-other-device-does-not-hold.md).

What another device is recorded as holding is a claim, made by the device that
made a file or by a report. A copy counts as one to ask for only on a paired
device, and each sync asks the other device about up to 16 of its claims on
files freed here, each once (`check_holders`, the `confirmed` table). It also
asks, once, about shared files made by a device no longer paired, which
nobody else would report on, since holdings are reported to a file's maker. A file
not here that no device this one can ask holds reads as *On no device*. See
[decisions/0055](decisions/0055-a-file-on-no-device-says-so.md).

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
The same report tells a sender about each send this device took, once, in
case the word sent as it took it was lost (`report_holdings` is given the
peer's tree).

**A send is not its bytes.** A delivery is taken once, and the record of it
is keyed by who sent it, under what name, and which version of that name
(the `deliveries` table). A send offered again whenever its sender reappears
is the same send, so a file the person deleted stays deleted. The same file
sent again is a new version, and arrives again. Before sending, a device
says which files it sent to that device before and asks
(`Store::sent_before`). Until 2026-10-10 the record was keyed by content, and
the same file sent twice was dropped without a word. See
[decisions/0059](decisions/0059-a-send-is-not-its-bytes.md).

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

### 2.4 Devices on one network need nobody to introduce them

Two devices on the same Wi-Fi have a path to each other, so they find each other
directly: every device multicasts a small beacon saying who it is and where it
can be reached, and listens for others. No server is involved, and a device
starts and syncs perfectly well when no rendezvous service exists at all.

The beacon is encrypted under a key derived from the master key, so a stranger
on the same café network sees random bytes — not the device identifier, not the
addresses, not the fact that qurb is running. That matters because the
identifier a device announces under is a bearer secret.

The rendezvous service is still there for the case it is actually needed: two
devices on *different* networks, which nothing local can help with. See
[decisions/0034](decisions/0034-finding-each-other-with-no-server.md), including
why beacons are answered rather than only broadcast.

### 2.5 There is no central truth, so devices must agree by themselves

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

A person settles it afterwards: keep the original, keep the other version, or
keep both under their own names. Settling is an ordinary change that syncs like
any other, and the version not kept goes to Recently deleted. A conflict is
found by its *name* — `report.conflict-3f2a9c01-2026-09-28-141500.txt`, the
device's short identifier and the time in UTC — parsed strictly by
`qurb_sync::conflict_origin`, so there is no table of conflicts to drift out of
step with the folder, and the window shows the device's name instead. See
[decisions/0043](decisions/0043-settling-a-conflict.md).

### 2.6 A path lives in one of two places

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
file in that device's vault, and the recipient files it in their own folder
without advertising it onward. See
[decisions/0029](decisions/0029-two-areas-shared-and-private.md) for the data
model and [decisions/0030](decisions/0030-sending-a-file-to-one-device.md) for
what a send promises — including the rule that a copy in somebody's vault is a
copy this device may *not* count on, which is the difference between eviction
and data loss.

**A send keeps no copy** ([decisions/0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)).
The sender records where the file is (`send_sources`): a path, or on a phone
a document it was lent (the `Documents` the app supplies). When the recipient
collects, the chunks are read from that file and checked against the hashes
taken when it was sent, the way a file in the folder supplies its own chunks.
A file changed or deleted before then is not sent. `Store::check_sends`
calls the send off and writes why in the history, and both apps' "something
failed" notification reads it there. Once the recipient has it,
`Store::tidy_sends` stops reading the file and deletes any copy qurb made —
only a file Android's share sheet lent briefly is copied, into the app's own
files. Sends made before 2026-10-10 kept a sealed copy; those still go when
space runs short on a desktop, or when asked for by name on a phone
(`sent_copies`, `release_sent_copies`).

**On a desktop, a received file leaves qurb.** The daemon saves it as an
ordinary file in `Downloads/qurb`, outside the folder, and stops tracking it:
not scanned, not counted against the limit, the person's to delete. What it
keeps is a `taken` record, never expired, so the sender offering it again
changes nothing. A phone has no downloads directory and files deliveries in its
folder, privately, as described above. See
[decisions/0037](decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md).

**A device can keep its own files private, and another holds them for it.**
With `own-files = private` a file added on a device goes into its own vault,
and the devices it names with `qurb holders` keep a copy — in the chunk store,
never in the folder, never shown — which lets it free its local copy with
`qurb free` and have it back with `qurb fetch`. A holder drops a file only on
the owner's deletion, never because it is missing from a list, so a wiped
phone cannot delete its own backup. On the wire each tree entry says which of
four areas it belongs to — shared, sent, held, hold — which is what moved the
protocol to `qurb/2`. Built and verified between two desktops, and used by the
Android app, where files that arrive on the phone are private by default and a
device is chosen on the Devices screen to keep them — verified between a Galaxy
S23 and a laptop: kept, freed, fetched back byte-identical, and let go on
deletion. See [decisions/0036](decisions/0036-a-phone-keeps-its-own-files.md).
A file the person adds from Files or from Private Vault goes into that area
instead, whatever the default says —
[decisions/0049](decisions/0049-adding-a-file-puts-it-where-you-are-looking.md).

**A folder in the shared area can be shared with chosen devices.** By default
a folder goes to every device. Choosing devices writes a small rule file,
`.qurb-sharing/<folder>`, which syncs like any other file — so every device
learns the rule the ordinary way, and each derives its own `shares` tables from
it. The server side enforces it: a device not chosen is not shown the folder's
files in the tree and is refused their manifests and chunks, however it asks.
Unticking a device stops new changes reaching it; what it already has, it
keeps, and the window says so rather than pretending otherwise. See
[decisions/0044](decisions/0044-sharing-with-chosen-devices.md).

**A folder can be kept only remotely.** This is one device's own choice,
recorded in `remote_folders` and never synced: the folder's files stay listed,
new versions arriving from other devices are recorded without their bytes
(`know_elsewhere`), and local copies are freed where another device is known
to have them. Keeping it locally again fetches everything back. This is the
*choosing* half of selective sync; the storage limit is the other half. See
[decisions/0045](decisions/0045-a-folder-kept-remotely.md).

**Another person's device can visit a computer as a guest.** Everything
above is one person's devices, holding one key. A guest is another person's
device, with its own key ([decisions/0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)).
The computer shows a guest code (`qurbg1-…`, *Add a person* in the window,
`qurb pair --guest`). The guest's device visits with it (`Request::Visit`,
*Visit someone's computer* on the phone, `qurb visit`). The computer's person
approves by number, and nothing of the computer's key is given. Each side
records the other in `peer_relations` (*guest*, or *host* on the guest's
side), and both are given a **meeting secret** (`meetings`). The rendezvous
service matches one person's devices by a group derived from their key, so
the two announce under a group and member derived from the meeting secret
instead: one more rendezvous session per meeting (`Connector::meet`). From
then on each shows the other only what was sent to it. That is the `Guest`
audience, chosen by the server from the relation on every request: no shared
area, no vault kept for anyone, and manifests and chunks refused for
anything else. The engine takes only deliveries from another person's device
whatever it is offered. A guest's copy never counts as one to ask for.
A guest's phone may also choose the computer it visits as the keeper of its
Private Vault, and then everything it shows that computer is **sealed**
first (`crates/storage/src/sealed.rs`). Sealed names are encrypted paths.
Each sealed file is a sealed header, carrying the real size, content hash
and chunk lengths, then each chunk sealed deterministically under a key
derived from the guest's key for that computer. The computer keeps them as
it keeps any device's vault, under the guest's *person* (an identifier the
guest derives for that computer), and can open neither a name nor a byte.
The guest's phone frees its own copies once kept, and fetches a file back,
unsealed and checked, when it is opened (`qurb_peer::fetch_kept`); a phone
set up again with the same key learns its folder from the computer
(`learn_kept`).

### 2.7 The index remembers what happened, not just what is

Everything above describes the index as a picture of the present: these paths,
these chunks, this version. It also keeps a history — one row per thing that
happened, with the path, the size and the device at the other end.

The reason is that the interesting questions are about the past. "Why is this
file not here?" is not answerable from the current state; it is answerable from
`evicted`, or `failed`, or `conflicted`, and the log that would have said so
belongs to a process that exited. `qurb activity` reads it. See
[decisions/0031](decisions/0031-what-happened-is-written-down.md), including
what the table deliberately does *not* hold.

### 2.8 An interface is a display of the engine, not a second one

A graphical front end runs the daemon inside itself rather than talking to one
over a socket, and asks it two different kinds of question. The daemon
publishes its live state — syncing, up to date, this many devices — on a
`watch` channel, because only the latest value is ever useful. Everything else
is a read-only query against the index: what devices, what files, what is
available where, what happened, what is still on its way.

Those queries are `qurb_cli::View`, and the terminal uses the same ones
(`qurb ls`, `qurb find`, `qurb activity`). See
[decisions/0032](decisions/0032-the-interface-hosts-the-daemon.md) — including
why a file's availability has more than two values, which is the difference
between "free up space" and "delete my only copy". There are four: *here*,
*elsewhere*, *only here*, and since
[decisions/0055](decisions/0055-a-file-on-no-device-says-so.md) *on no device*,
for a file known about that no device this one can ask still holds.

### 2.9 Encryption happens before anything leaves the device

Chunks are compressed, then encrypted, then written to disk and sent over the
network. The keys never leave your devices. Our servers see encrypted bytes and
routing metadata, nothing else.

This is called **zero-knowledge**, and it has a hard consequence that shapes the
product: if you lose your key, we cannot recover your data. Not "we won't" — we
genuinely cannot.

The key has a spelling, 24 words that *are* the key, but since 2026-10-05
nobody is asked to write them down
([decisions/0052](decisions/0052-the-key-travels-with-the-code.md)). A new
device gets the key from one the person already has, through the pairing code,
which it scans or types. A phone also keeps its key in Block Store,
end-to-end-encrypted in its Google backup. The key is what has to survive;
the files survive only where a device or a replica still holds them. See also
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

The right-hand box is the target design. What runs today is two programs a
person runs themselves — the rendezvous service (`qurb signal`) and the relay
(`qurb relay`), on their own computer or server — with public STUN servers
(Google's and Cloudflare's) answering "what is my address". There are no
accounts, no billing and no device registry beyond each device's own list of
the devices it trusts.

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
11. Peers request chunks they lack  ✅ only the missing ones move, eight at
                                       a time; a fetch cut off carries on
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
│   ├── features.md        Everything that exists, by where a person meets it
│   ├── product-plan.md    Turning the engine into a product: what exists
│   │                      against the brief, and the decisions still open
│   ├── glossary.md        Every term, defined plainly
│   ├── trying-it.md       Running it yourself, from one machine to a phone
│   ├── architecture.md    The target design, all subsystems
│   ├── roadmap.md         Phases, timelines, honest risk assessment
│   ├── anywhere.md        Syncing from outside the house: what must be
│   │                      reachable, and a free way to get there
│   ├── measuring-connectivity.md
│   │                      how to measure the direct-connection rate
│   ├── design/            The design pass: direction.md, the owner's
│   │                      direction word for word; brief.md, how it meets
│   │                      the product and where each feature goes
│   ├── decisions/         Why each choice was made (one file per decision)
│   └── phases/            What each phase produced, with measurements
│
├── crates/
│   ├── storage/           Local storage. The foundation everything sits on.
│   │   └── src/
│   │       ├── sealed.rs    a guest's files as the computer keeping them sees
│   │       │                them: sealed names, sealed chunks (0060)
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
│   │                        peak_memory: what receiving a large file costs
│   │
│   ├── sync/              What to do when two devices disagree.
│   │   └── src/           Pure logic: no disk, no network, no clock.
│   │       ├── clock.rs     version vectors and their partial order
│   │       ├── version.rs   one device's view of one path
│   │       ├── resolve.rs   deciding between two versions
│   │       ├── reconcile.rs deciding about a whole tree
│   │       ├── path.rs      which paths another device may name
│   │       ├── sharing.rs   which devices a folder is shared with: the
│   │       │                rule files, parsed and checked
│   │       └── device.rs    a device's identity, and its short form
│   │
│   ├── peer/              Reaching another device, over QUIC.
│   │   ├── src/wire.rs      the message format; bounded and hostile-input safe
│   │   ├── src/identity.rs  a device's certificate and its fingerprint
│   │   ├── src/pairing.rs   deciding which device to trust in the first place
│   │   ├── src/base32.rs    invite encoding, chosen for how QR codes work
│   │   ├── src/local.rs     beacons: finding each other with no server at all
│   │   ├── src/nat.rs       STUN, NAT classification, hole punching
│   │   ├── src/connect.rs   the policy: discover, announce, race candidates
│   │   ├── src/tls.rs       mutual authentication by pinned fingerprint
│   │   ├── src/kept.rs      a vault kept sealed by another person's computer:
│   │   │                    fetched back and unsealed (0060)
│   │   ├── src/server.rs    serves a store, read-only
│   │   ├── src/client.rs    asks for trees, manifests, chunks
│   │   └── src/source.rs    plugs the client into the engine
│   │
│   ├── mobile-ffi/        The engine, as a phone can call it.
│   │   ├── src/lib.rs       UniFFI surface: files and where they are, pairing,
│   │   │                    bounded sync, holding, freeing, sending, history
│   │   └── src/bin/         uniffi-bindgen, which mobile-bindings.sh runs
│   │
│   ├── keys/              The root secret and the way back to it.
│   │   ├── src/master.rs    HKDF derivation, one key per purpose
│   │   ├── src/phrase.rs    the 24 words, via BIP-39
│   │   ├── src/vault.rs     where the master key lives: open, unlock,
│   │   │                    protect, restore
│   │   └── src/protection.rs  the three ways to keep it — a file, the
│   │                        system keystore, a passphrase — and what each
│   │                        defends against
│   │
│   ├── signal/            Finding the other device.
│   │   ├── src/rendezvous.rs  identifiers the server cannot link to anyone
│   │   ├── src/message.rs     what is said; carries nothing about files
│   │   ├── src/server.rs      holds a channel open per device
│   │   ├── src/client.rs      announce, ask, punch when told
│   │   ├── src/wake.rs        the seam: how an absent device gets poked
│   │   ├── src/fcm.rs         that seam, filled in by Firebase (feature `push`)
│   │   └── src/tls.rs         a certificate of its own, for a bare IP address
│   │
│   ├── relay/             The fallback when no direct path exists.
│   │   ├── src/frame.rs      opaque forwarding, binary and bounded
│   │   ├── src/socket.rs     a relay connection pretending to be a UDP socket
│   │   └── src/server.rs     forwards between registered identifiers
│   │
│   ├── qurb/              The program a person runs.
│   │   ├── src/lib.rs       the daemon, as a library, so an interface can
│   │   │                    run the same one the terminal does
│   │   ├── src/main.rs      init, enrol, pair, join, visit, run, replica, status,
│   │   │                    verify, reclaim, fetch, free, private,
│   │   │                    unprivate, send, cancel, holders,
│   │   │                    remove-device, conflicts, share, keep, deleted,
│   │   │                    restore, forget, activity, ls, find, config,
│   │   │                    protect, version, signal, relay, netcheck
│   │   ├── src/daemon.rs    watch, apply, sync, collect, stay under the limit;
│   │   │                    stop if the folder is moved or deleted
│   │   ├── src/lock.rs      one daemon per folder, enforced not assumed
│   │   ├── src/profiles.rs  which folders exist, so commands need no path
│   │   ├── src/qr.rs        a pairing code a camera can read
│   │   ├── src/send.rs      what sending some files and folders actually sends
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
│   │   ├── src/notify.rs    the three things worth interrupting somebody about
│   │   ├── src/commands.rs  every question the window may ask
│   │   ├── src/autostart.rs starting at login, hidden
│   │   ├── src/instance.rs  one qurb per person; a second launch shows the first
│   │   └── ui/              the window: index.html, app.css (the design),
│   │                        theme.js (light or dark, before anything draws),
│   │                        a script per place (core, setup, home, files,
│   │                        devices, settings, app), icons.js (generated),
│   │                        fonts/ (Inter, bundled)
│   │
│   └── tray/              An icon in the corner: the daemon with a face.
│       ├── src/main.rs      the daemon, with an icon or a window to show
│       ├── src/host.rs      whether a tray icon would be visible at all
│       ├── src/icon.rs      the mark, drawn rather than shipped
│       ├── src/ui.rs        the menu, and what to do when there is no tray
│       └── src/window.rs    the window: status, and the storage slider
│
├── android/               The Android app. Kotlin over the FFI, no sync logic.
│   └── app/src/
│       ├── main/java/com/qurb/
│       │                  MainActivity.kt     the shell: four tabs, the places reached
│       │                                      from them, the Transfers bar, and the
│       │                                      actions more than one place offers
│       │                  Screen.kt           what a place is: views, a tab, a
│       │                                      refresh, Back
│       │                  Kit.kt              the components every place is built
│       │                                      from: rows, file states, groups,
│       │                                      attention, empty states, sheets
│       │                  Qurb.kt             the one handle on the engine, off
│       │                                      the main thread
│       │                  Errors.kt           an engine error, said for a screen
│       │                  HomeScreen.kt       is everything okay: one state, one
│       │                                      action, conflicts, recent
│       │                  FilesScreen.kt      Files and Private Vault: one browser,
│       │                                      two areas, each file's state
│       │                  DevicesScreen.kt    the devices as cards, who keeps this
│       │                                      phone's files, removing a device
│       │                  SettingsScreen.kt   grouped lists: storage, who has each
│       │                                      folder, privacy, recovery, advanced
│       │                  ActivityScreen.kt   everything this phone did, from Home
│       │                  DeletedScreen.kt    Recently deleted: thirty days to
│       │                                      change your mind
│       │                  ShowCode.kt         this phone showing a pairing code
│       │                  Approval.kt         a device asking to join, approved by
│       │                                      the six digits both screens show
│       │                  Words.kt            how the app says things, in one place
│       │                  SetupActivity.kt    set up, or join by scanning a code
│       │                  Backup.kt           the key, kept in Block Store
│       │                  ScanActivity.kt     reading a pairing QR code
│       │                  ShareActivity.kt    the share sheet's way in: save it, or
│       │                                      send it to one device
│       │                  AndroidKeyStore.kt  the platform half of decision 0021
│       │                  SyncWorker.kt       background sync, on WorkManager;
│       │                                      a long pass in the foreground
│       │                  Transfers.kt        the notification a long transfer
│       │                                      runs under, and its progress
│       │                  Notices.kt          the desktop's three notifications:
│       │                                      sent to you, collected, failed
│       │                  Previews.kt         an image or a text's start, for a
│       │                                      conflict's two versions
│       │                  SentCopies.kt       copies of sent files, let go of by name
│       │                  Appearance.kt       light, dark, or as the system is set
│       │                  ManageSpaceActivity.kt
│       │                                      what Android's Clear data opens: what
│       │                                      only this phone has, before it goes
│       │                  QurbDocumentsProvider.kt
│       │                                      the files, in the system picker,
│       │                                      from the index; freed ones download
│       │                                      when opened
│       ├── main/res/      the design as resources: colours and glass by role,
│       │                  Inter, Lucide icons (generated), the layouts
│       ├── push/java/     being woken by Firebase — compiled only when a
│       │                  google-services.json is present
│       └── nopush/java/   the same surface, doing nothing, when it is not
│
├── packaging/             Getting it onto a machine.
│   ├── arch/PKGBUILD      a pacman package of this checkout (makepkg -si)
│   ├── install.sh         qurb in this user's applications menu: the window,
│   │                      with qurb and qurb-tray beside it
│   ├── install-rendezvous.sh
│   │                      the rendezvous service on your own computer, with
│   │                      push, as a binary of its own name and the unit below
│   ├── qurb-rendezvous.service
│   │                      the rendezvous service on your own computer, at login
│   ├── qurb.desktop       the launcher entry
│   ├── qurb.svg           its icon: the mark, the window's and the tray's drawing
│   └── server/            systemd units and TLS for a host of your own
│
├── scripts/
│   ├── android-app.sh     build the app: libraries, bindings, then Gradle
│   ├── android-build.sh   cross-compile the engine for all four Android ABIs
│   ├── android-icons.py   the Lucide icons the app uses, as vector drawables
│   ├── android-test.sh    run the test suite on a device, over adb
│   ├── desktop-icons.py   the Lucide icons the window uses, as a sprite
│   ├── desktop-smoke.sh   drive the real desktop window end to end, on a
│   │                      display of its own (with desktop_smoke.py)
│   ├── smoke_portal.py    a stand-in desktop settings portal, for the
│   │                      smoke test's dark-preference check
│   └── mobile-bindings.sh generate the Kotlin and Swift bindings
│
├── experiments/
│   ├── desktop-fixtures/  Throwaway. The window's screens against made-up
│   │                      data, so layout can be worked on with no daemon.
│   ├── phase0-spike/      Throwaway. Proved the core ideas work.
│   │   └── src/
│   │       ├── lib.rs            chunker + content-addressable store
│   │       └── bin/
│   │           ├── chunkbench.rs measures the five Phase 0 kill criteria
│   │           ├── sweep.rs      compares chunk size configurations
│   │           └── quictest.rs   NAT classification + QUIC throughput
│   ├── phone-serving/     Throwaway. A phone's serving of a large file, timed
│   │                      on the phone outside the app: what found Cubic
│   │                      holding transfers to 5 MB/s (decision 0051).
│   └── service-capacity/  Throwaway. Load for the rendezvous service and the
│                          relay: what a small server carries, measured.
│
└── website/               The website, in the apps' design: the window's
                           tokens and components, its icons (generated from
                           its sprite), its words. Next.js on Vercel, deployed
                           by every push to main; otherwise separate — nothing
                           here depends on it or builds it.
```

**The `crates/` vs `experiments/` split is load-bearing.** Anything in
`experiments/` is disposable and is allowed to cut corners, as long as its
README says which corners. Anything in `crates/` is meant to last and is held to
a real standard. Nothing should quietly migrate from one to the other — Phase 1
code gets written fresh, informed by the spike rather than copied from it.

---

## 5. What actually exists right now

Being precise about this matters, because the architecture document describes a
complete system and a good deal of it is still unbuilt. The engine is real, and
so, on Linux and Android, is a product around it that does what the brief asks
of the features, designed to the owner's direction; what it does not have yet
is the owner's review of that design, a relay on a server, and a release.
Dark mode is built on both ([decisions/0056](decisions/0056-dark-mode.md)).
The unbuilt parts are listed at the end of this section.

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
| Recently deleted | a file taken out of the folder by a deletion goes to `trash/` in the store for 30 days; restoring writes it back as a new version in the area it came from, so a shared file returns everywhere; sharing rules are not kept — [0042](decisions/0042-recently-deleted.md) |
| Sharing with chosen devices | a folder's devices are a rule file under `.qurb-sharing/` that syncs like any other; each device derives `shares` tables and filters what peers are shown and what it takes by them — [0044](decisions/0044-sharing-with-chosen-devices.md) |
| A folder kept remotely | this device's choice (`remote_folders`, never synced): files listed, new versions recorded without their bytes, local copies freed where another device has them — [0045](decisions/0045-a-folder-kept-remotely.md) |
| Conflicts settled | found by name (`qurb_sync::conflict_origin`), settled as ordinary changes; the version not kept goes to Recently deleted — [0043](decisions/0043-settling-a-conflict.md) |
| Garbage collection | two-stage, never touches a referenced chunk; the daemon runs it every five minutes, a phone after each background sync |
| Integrity verification | detects missing, corrupt, and orphaned chunks |
| Reclaiming duplicates | `qurb reclaim`, for stores written before single-copy |
| A storage limit | drops local copies, keeps the index, never the only copy |
| Per-device private vaults | `files.scope`: `NULL` is shared, a device id is that device's vault |
| Holding another device's vault | `files.held`, a `holders` list, and four areas on the wire; never released, dropped only on a tombstone |
| A history of what happened | one table, pruned by age and count; `qurb activity` reads it |
| Sending to one device | `qurb send <files and folders> to <device>`; no copy kept: read from the file when collected, called off if it changed or went first — [0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md) |
| Receiving on a desktop | saved to `Downloads/qurb` as an ordinary file; overlap with the folder refused |
| Deliveries remembered | a `deliveries` record per send -- sender, name, version -- never expired, so a send is taken once and the same file sent again arrives again — [0059](decisions/0059-a-send-is-not-its-bytes.md) |
| Removing a device | `Store::remove_device`: trust ends here, waiting sends cancelled, its copies stop counting as copies, what is kept for it stays unless asked; trust asked per request, not only at the handshake — [0041](decisions/0041-removing-a-device.md) |

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
| A fetched file's date | the version's change time, set on disk before it is moved into place (2026-10-08; files fetched before keep their arrival time) |
| Two directories converging | including conflicts, deletions, resurrections |

Those four crates were the first to be joined together; the counts below are
for the workspace as it stands.

### Built and tested (`crates/peer`, Phase 1)

| thing | status |
|---|---|
| QUIC transport | one bidirectional stream per request |
| Mutual authentication | pinned fingerprints, handshake signature verified |
| Wire format | length-bounded; decoder has no panicking path |
| Incremental transfer | only chunks the receiver lacks cross the wire, eight in flight at once; a fetch that was cut off chunks what it has and carries on — [0050](decisions/0050-large-files-from-a-phone.md). From a phone over Wi-Fi this measured about 5 MB/s under Cubic, barely faster than one at a time ([phase 5](phases/phase-5-mobile.md#measured-on-the-s23-2026-10-05)); the next row is what changed that |
| Pairing | approved at the device showing the code, both screens showing the same six digits; refused between devices holding different keys; each side records whether the other is a phone, computer or replica — [0053](decisions/0053-approval-same-key-and-safe-copies.md) |
| Guests | another person's device visits with a guest code, keeping its own key; the two meet at the rendezvous service under a secret of their own and are shown each other only what was sent to them (the `Guest` audience) — [0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md) |
| A guest's folder, sealed | a guest's Private Vault kept by the computer it visits under sealed names and sealed chunks the computer cannot open, filed under the guest's person; freed on the phone once kept, fetched back and checked when opened — [0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md) |
| Congestion control | BBR rather than quinn's default, Cubic, which reads Wi-Fi's stray losses as congestion: 12.5–14.0 MB/s from the S23 against 5.2–5.3 — [0051](decisions/0051-bbr-not-cubic.md) |
| Read-only serving | a peer can ask, never tell — with one exception below |
| Vault authorisation | tree, manifest and chunk requests all check the asker's scope |
| Delivery reports | `Got`: the receiver says it holds it, so the sender can stop calling it undelivered |
| Every reachable address offered | LAN, overlay network and public, raced in parallel |
| Signalling that reconnects | a rendezvous restart costs seconds, not a daemon restart |
| Local discovery | encrypted beacons; two devices on one network need no server at all |
| A rendezvous on a bare IP | self-signed, pinned by fingerprint in the URL — no domain, no authority |

### Built and tested (`crates/keys`, Phase 1)

| thing | status |
|---|---|
| Master key | 256-bit, from the OS CSPRNG |
| Key derivation | HKDF-SHA256, one key per purpose, versioned labels |
| Recovery phrase | 24 words, BIP-39, checksummed |
| Recovery, end to end | the phrase turns back into the user's files |
| Key hygiene | redacted in `Debug`, wiped on drop, owner-only on disk |

801 tests in 90 test binaries on Linux, all passing (2026-10-10, debug build,
the development laptop, on a network that carries multicast — seven tests find
devices on the local network that way, and fail on one that does not). Clippy
is clean. On Android the engine's crates run by `scripts/android-test.sh`:
678 tests in 63 binaries passed on the Android 14 x86_64 emulator on
2026-10-08, including two phones pairing and syncing. The last run on a
Galaxy S23 was 426 of them, on 2026-09-17. See
[phases/phase-5-mobile.md](phases/phase-5-mobile.md#finishing-what-was-left).

**The wire protocol is `qurb/2`.** It was `qurb/0` until tree entries gained a
flag saying "this belongs in your vault", which is not a byte an older build can
safely ignore — it would adopt somebody else's private content as shared and
advertise it to the whole fleet. It became `qurb/2` on 2026-09-27 when that flag
became one of four areas — shared, sent, held, hold — because holding another
device's vault has a direction a flag cannot carry, and a `qurb/1` build would
read the new two as "sent to me" and file somebody's private files in its own
folder. Devices negotiate the version during the TLS handshake, so a mismatch
is a clean refusal to connect. Every device has to be rebuilt together.

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
| Hostile paths | `../`, absolute paths, qurb's own store — refused on the wire and again before any write or delete (added 2026-09-25) |
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
cargo run --release -p qurb-peer --example transfer -- /tmp/dev-a /tmp/dev-b
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

That was where Phase 2 ended: everything a *person* needs — an interface,
installers, an app on a phone — was missing. The window, the Android app and
the package came after, and are described below.

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
what that costs and why nothing else works. Home says how many files are still
held only by the phone, which is the honest form of "it will get there"; the
share sheet's confirmation says where what was just shared will go.

**It runs on a phone.** `./scripts/android-test.sh` pushes the test binaries
with `adb` and runs them: on a Samsung Galaxy S23 (Android 16, arm64-v8a) all 35
binaries passed on 2026-09-17 — 426 tests, including the real QUIC handshakes
and hole punching. The suite has grown since and has been run on the
emulator, not on the phone again. Receiving a 512 MiB file over the network
there grows the heap by 6 MiB.

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

**The release build is the light one**: arm64 only, the code shrunk by R8, and
the engine built with the `mobile` profile, link-time optimised. The designed
app's is an 11.6 MB APK, measured on a Galaxy S23 on 2026-10-08 at a median cold
start of 182 ms and about 91 MB in memory, most of it graphics. The first
release build was 10.7 MB installed against 46.7 MB for the debug build, at
about 175 ms — see [decisions/0039](decisions/0039-a-light-android-app.md).

**There is an Android app** — [`android/`](../android/) — which installs, sets up
an identity, keeps the key in the Android Keystore, pairs and syncs. Since
2026-09-29 it follows the same design as the desktop window, in four tabs —
Home, Files, Devices, Settings — under a floating bar. Home says whether
everything is synced and offers one action, *Send to device*, with conflicts to
settle as attention. Files is browsed by folder and searched, says where each
file's bytes are, and opens, frees, fetches, renames, moves, sends, saves out or
deletes each from a sheet; Private Vault, the phone's own files, is a step
inside it, in the same browser, and a file moves between the two with *Move
to Private Vault* or *Move to Files*. Devices pairs — scanning a code or
showing one, a device asking to join approved by the six digits both screens
show — chooses who keeps the phone's own files, and removes a device. Settings
has who has each folder (sharing, and keeping it only remotely), Recently
deleted, space, copies of sent files, notifications, the theme, syncing and the
version. Activity is reached from Home, and Transfers is a bar that appears
only while something moves. A conflict's sheet previews each version that is on
the phone. The desktop's three notifications are raised from the history after
each sync. Android's *Clear data* opens qurb's own screen, which says what
exists only on this phone before anything goes. Before the design the same
features were five tabs — Home, Vault, Devices, Transfers, Settings — and it is
through those that most of the hardware checks below were made. The designed
screens were walked on the S23 on 2026-10-03 — see
[phases/phase-5-mobile.md](phases/phase-5-mobile.md#the-designed-app-on-the-s23),
and
[android/README.md](../android/README.md). Building it found a bug nothing else could: UniFFI keeps only the *last*
`#[uniffi::export] impl` block for an object and silently discards the others,
so eight methods were missing from the generated Kotlin and Swift while every
Rust test passed.

The app syncs on its own through WorkManager — every fifteen minutes when
Android allows it, or every hour once a push has arrived in the last week,
since push then does the urgent part — and a `DocumentsProvider` puts the synced files in the
system file picker and the Files app — listed from the index, so a freed file
is shown and downloads when opened, and other apps can save into the folder. A device that has not answered when a
window closes counts as *unreachable*, not as time running out: the second is
a retry with exponential backoff, and confusing them once pushed a phone's next
sync further away each time its computer was off
([decisions/0020](decisions/0020-sync-takes-a-deadline.md#found-on-a-phone)).
The phone can fall back to a relay, set in Settings by `host:port` — a name is
looked up each pass — which is what lets it reach a laptop at home from mobile
data; a relay that cannot be reached costs only the fallback. A pass reaches
every paired device at once and syncs each as it answers, so a
switched-off device cannot use up the window a working one needs. A pass with
something waiting for a device it reached stays open up to ten seconds for
that device to collect it, because every device pulls and the
phone's own syncing can finish before the other side dials back; and dropping
the pass's connector stops everything it started
([same record](decisions/0020-sync-takes-a-deadline.md#a-pass-that-waits-to-be-collected-from)).
A pass with 32 MiB or more waiting to be collected runs in the worker as a
foreground service, under a notification. It answers for as long as chunks
keep going, up to thirty minutes
([decision 0050](decisions/0050-large-files-from-a-phone.md)). On the S23 it
survived the owner leaving the app. It ends ten seconds after the last chunk
went, or a minute after if what was being collected is still waiting, so a
collector that restarts mid-file is waited for, and it serves a device
collecting whether this pass dialled it or it dialled in. Android refuses the
foreground to an app that has already left the screen; a large transfer cut
short that way leaves a *Tap to finish sending* notification. The app watches the worker's passes, shows them as syncing and runs
its own after them, and *Sync now* queues behind a running pass rather than
replacing it.

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
| A window | Tauri 2, no framework and no build step; designed from the owner's direction ([design/direction.md](design/direction.md)): a translucent sidebar — Home, Files, Devices, Storage, Private Vault set apart, Settings — glass materials over a quiet environment, Inter, Lucide icons, motion that shows what moved |
| It hosts the daemon | the same one `qurb run` starts, on its own threads |
| Home | one state — synced, syncing, devices away, add your first device, needs attention — one action, *Send to device*, attention only when something needs a decision, and Recent |
| Files | a file browser from the index: search, breadcrumbs, folders apart from files, and where each file's bytes are — *On this device*, *Available elsewhere*, *Only copy here*, *On no device* ([0055](decisions/0055-a-file-on-no-device-says-so.md)); keep a file here, free its local space, send it, move it into Private Vault, delete it; a details panel naming the devices that hold it |
| Private Vault | this computer's own files, browsed the same way; *Move to shared* takes one back out ([0057](decisions/0057-moving-a-file-into-or-out-of-private-vault.md)) |
| Devices | who is paired, whether each is connected now and whether directly or through the relay, when each was last reached; removing one, with what that will and will not do said first |
| Activity | reached from Home: what this device did, paged, with the reason where there is one |
| Storage | how much can be freed without losing anything, the largest files that would free it, and the allowance |
| Settings | grouped lists: this device and its key, devices and pairings, where files sent here go, keep new files private, notifications, the 24 words, appearance — system, light or dark ([0056](decisions/0056-dark-mode.md)) — and the advanced settings — rendezvous, relay, port, start at login, version, Quit |
| Setting a device up | make a new one, with nothing to write down, or join with another device's code, which brings the key ([0052](decisions/0052-the-key-travels-with-the-code.md)); the storage question asked; the 24 words a fallback |
| Locked | a key protected by a passphrase is unlocked in the window; at login the window shows itself to ask |
| Pairing | show a code — QR, typed or spoken — or enter one, with a countdown; a device asking is approved by the six digits both screens show ([0053](decisions/0053-approval-same-key-and-safe-copies.md)) |
| Sending | a sheet — what, to which device, then the file travelling there and landing — from Home, a file, a device, or files dropped anywhere on the window |
| Notifications | three things only: a file sent to you, one collected, one that failed |
| Transfer progress | a panel that appears while something moves or waits: both directions, live, with the time left |
| Cancelling a send | before it is collected, from the window or `qurb cancel`; never after |
| Conflicts | attention on Home and Files, reviewed in a sheet: both versions, who made each and when, previewed when here (an image, or a text's start); keep one, the other, or both |
| Recently deleted | from Files: thirty days, restore — back in the area it was deleted from — or delete for good |
| Who has each folder | a folder's options, from its menu in Files: share it with chosen devices; free its space here or keep it on this computer |
| Security | in Settings: how the key is kept and changing it, under This device; pairings and removals, under Devices; this device's identity, under Advanced |
| One qurb per person | closing the window keeps it syncing; launching again shows the running one |

### Designed but not built

An iOS app, per-file keys, key rotation, relay selection and quotas, accounts
and billing, packages for anything but Arch, and an automatic updater — the
last deliberately, until it can be built safely
([decisions/0047](decisions/0047-versions-and-upgrades.md)).

The desktop interface does what the command line does. The window sets a
device up — the folder, how much disk it may use, and either a new key with
nothing to write down or the code another device shows, which brings its key
([0052](decisions/0052-the-key-travels-with-the-code.md)); the 24 words remain a
fallback — unlocks a
passphrase-protected key, pairs devices by code, browses what is synced and
where each file's bytes are, sends to one device, settles conflicts, restores
deleted files, chooses which devices each folder goes to and which folders stay
only remote, and shows activity, transfers and storage; the applications menu
opens it ([decisions/0040](decisions/0040-the-menu-opens-the-window.md)).
Closing it hides it and qurb keeps syncing; it starts at login without a
window, and *Quit qurb* in Settings stops it. Its design follows the owner's
direction ([design/direction.md](design/direction.md),
[decisions/0048](decisions/0048-the-design-direction.md)) and was built on
2026-09-29, and the Android app was rebuilt to it the same day. Both have a
dark mode on the same colour roles since 2026-10-08, chosen in Settings or
following the system ([decisions/0056](decisions/0056-dark-mode.md)).

Selective sync is built in both halves: a device drops local copies when it is
over its storage limit and fetches them back on request, and a person can say
a folder is kept only remotely
([decisions/0045](decisions/0045-a-folder-kept-remotely.md)). What Linux still
lacks is a placeholder — see the first gap below.

### The gaps that matter most

Eight things are known-missing rather than merely unbuilt:

1. **An evicted file simply vanishes from the folder on Linux.** Windows and
   macOS both have an API for a placeholder that keeps its name and size and
   fetches when opened; Linux has nothing short of a FUSE mount, whose failure
   would take the user's whole folder with it. So a file dropped for the
   storage cap is absent, and `qurb status` is the only place that says it
   still exists. See
   [decisions/0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md).

2. **Key recovery, for the last device.** Zero-knowledge means a lost key is
   lost data. Since [decisions/0052](decisions/0052-the-key-travels-with-the-code.md)
   nobody writes 24 words down: a new device takes the key from one it pairs
   with, and a phone also keeps its key in Block Store, end-to-end encrypted
   with its screen lock. That survived a reinstall on the S23, and did not
   survive *Clear data*; a restore onto a new phone, which Google's backup
   is for, has not been watched. What is left is the person whose every device
   is gone. A computer has nothing that leaves the machine, so a person with
   only computers, who did not keep the words, has no way back. Every
   consumer product in this space adds some escape hatch for that — social
   recovery, an escrowed key, a printed kit — and each trades away part of
   the promise. Which to add, if any, is undecided.

3. **Nobody has watched a phone sync for a day.** The background worker is
   scheduled and runs when asked; what Android actually grants it over a day,
   and what that costs in battery, is unmeasured. Everything verified on
   hardware so far was one phone and one laptop on one home network.

4. **Two kill criteria remain unmeasured**, both for want of time and hardware
   rather than for want of code: Phase 3's direct-connection rate needs a
   second machine on a different network, and Phase 5's battery-and-survival
   test needs the phone left alone for a day. An emulator answers neither.

5. **A replica over its storage cap drops nothing**, by decision: it keeps
   everything and says it is over, since being the copy that can be relied on
   is its whole job ([decisions/0058](decisions/0058-a-full-replica-says-so.md),
   after [0025](decisions/0025-a-storage-cap-that-cannot-lose-data.md)). On
   the small always-on box a replica is most useful on, its disk has to hold
   what it is given.

6. **Some features have not crossed between real devices.** Since
   2026-10-08 the phone's side of nearly everything has been watched with the
   laptop: sharing a folder with chosen devices, keeping one only remotely,
   removing a device, the share sheet's *send to a device*, the phone showing
   a pairing code, moving files into and out of Private Vault. Still not: most
   of the desktop window's own buttons with a real phone on the other end —
   its pairing was, approved by number on 2026-10-08, and the rest is driven
   by its smoke test against a second device on the same machine — two
   phones, and a new phone restored from a Google backup. The designed app,
   measured on the S23 on 2026-10-08, starts as fast as before and holds
   about 13 MB more memory, mostly graphics, measured once rather than back to
   back with the old build
   ([decisions/0039](decisions/0039-a-light-android-app.md)). See
   [features.md](features.md) for which is which.

7. **Transfer speed is measured on one home Wi-Fi only.** A large file left
   the phone at 5 MB/s because quinn's default congestion controller read
   Wi-Fi's stray losses as congestion. With BBR
   ([decisions/0051](decisions/0051-bbr-not-cubic.md)) it goes at 10.6–14.0
   MB/s through the app. One run at 1.33 MB/s on 2026-10-05 was not seen
   again in 15 more on 2026-10-08. Unmeasured on mobile data, through the
   relay, and alongside other traffic, where BBR is known to take more than
   its share.
   See [phases/phase-5-mobile.md](phases/phase-5-mobile.md#through-the-app-with-bbr--and-a-phone-cleared).

8. **A phone is still a fragile place for a file.** On 2026-10-05 the S23's
   data was cleared from Settings, and 18 files the laptop had freed went with
   it. Since [decisions/0053](decisions/0053-approval-same-key-and-safe-copies.md)
   a phone's copy does not let another device free its own, a phone's
   Private Vault is kept by its first computer, and *Clear data* opens qurb's
   own screen, which says what would be lost. What is left is what no app can
   prevent: a phone lost or broken before it has synced. The 18 files lost
   that day stay listed on both devices, as *On no device*
   ([decisions/0055](decisions/0055-a-file-on-no-device-says-so.md)).
   *Clear data* also emptied the phone's rendezvous setting, and the app fell
   back to the emulator's address without saying so: for three days the
   phone could be neither woken nor reached off the home Wi-Fi, until it was
   noticed on 2026-10-08 and set again by hand. The product does not yet
   prevent that
   ([phase 5](phases/phase-5-mobile.md#the-last-of-the-phone-and-a-setting-lost-three-days-before)).

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
then `pair` there, which shows a code, and `join` with that code on the
second, which brings the first device's key with it
([decisions/0052](decisions/0052-the-key-travels-with-the-code.md)); the first
asks to approve the second by a six-digit number both print. Then `run` on
both. `enrol` with the 24 words is the way in without the other device to
hand:

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
# Where files sent to this device are saved. Empty means Downloads/qurb; a
# directory inside the synced folder is refused, since they would sync to
# every device. `off` keeps them in the folder instead.
./target/release/qurb config ~/Sync downloads=~/Downloads/from-my-phone
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
# Files two devices changed at once, and settling one: keep this device's
# version, the other, or both. Nothing is lost either way.
./target/release/qurb conflicts ~/Sync
./target/release/qurb conflicts ~/Sync keep "notes.conflict-3f2a9c01-2026-09-28-141500.txt" both
```

```bash
# Recently deleted: thirty days to put a file back where it was.
./target/release/qurb deleted ~/Sync
./target/release/qurb restore ~/Sync '#1'
```

```bash
# Share a folder with chosen devices only, or with everyone again; keep a
# folder only listed here, fetching each file when asked for.
./target/release/qurb share ~/Sync work with this,laptop
./target/release/qurb share ~/Sync work with everyone
./target/release/qurb keep ~/Sync videos remote
```

```bash
# Stop trusting a device. Says what that will do; --yes does it.
./target/release/qurb remove-device ~/Sync old-phone
./target/release/qurb version
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
#
# `--tls` makes the rendezvous present its own certificate and print the whole
# setting — address and fingerprint — for devices to copy. That is what lets it
# run on a host with an address and no domain name. Without it, put a reverse
# proxy in front: unencrypted rendezvous is refused by devices anywhere but the
# local network.
./target/release/qurb signal 0.0.0.0:9000 --tls --host 203.0.113.5
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
# The desktop application: the same daemon, in a window, over the same
# queries `qurb ls`, `qurb find` and `qurb activity` use.
cargo build --release -p qurb-desktop
./target/release/qurb-desktop ~/Sync
```

```bash
# Drive that window end to end -- set up, every tab, pairing by code, a send,
# removing a device, a passphrase set, quit, refused and then unlocked on the
# next start -- on a display of its own, failing on any command that errs.
# Needs broadwayd and WebKitWebDriver.
./scripts/desktop-smoke.sh
# The window following the desktop's dark preference, from a stand-in
# settings portal on a bus of its own. Also needs dbus-daemon and PyGObject.
SMOKE_MODE=theme ./scripts/desktop-smoke.sh
```

```bash
# The tray icon: the same daemon, smaller. Falls back to a window where there
# is no system tray, which on GNOME is always.
cargo run --release -p qurb-tray -- ~/qurb
```

```bash
# Put it in the applications menu for this user: the menu opens the window,
# and qurb and qurb-tray are installed alongside. --uninstall undoes it.
cargo build --release -p qurb-cli -p qurb-tray -p qurb-desktop
./packaging/install.sh
```

```bash
# Or, on Arch and its derivatives, a pacman package of this checkout for every
# user. Upgrade by building again; `pacman -R qurb` removes it.
cd packaging/arch && makepkg -si
```

```bash
# A release APK, signed with the key named in ~/.config/qurb/signing.properties.
# An update must be signed with the same key, or Android refuses it.
./scripts/android-app.sh release
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
2. [features.md](features.md) — what a person can do with it today, on each
   device.
3. [glossary.md](glossary.md) — skim it, then use it as a reference.
4. [roadmap.md](roadmap.md) — what is being built when, and the honest risks.
5. [phases/phase-0-spike.md](phases/phase-0-spike.md) — the measurements, and
   the design error they caught.
6. [architecture.md](architecture.md) — the full target design.
7. [decisions/](decisions/) — read these when you want to know *why*, or when
   you are about to change something and want to know what it would break.
8. [crates/qurb/README.md](../crates/qurb/README.md) — the commands, and what
   the daemon does not do yet. The quickest way to see the shape of the whole
   thing is to run it.

Then the layers, bottom to top:

9. [crates/storage/README.md](../crates/storage/README.md) — the two invariants
   the storage layer is built around. Then its source, in this order:
   `store.rs`, `db.rs`, `gc.rs`.
10. [crates/watcher/README.md](../crates/watcher/README.md) — the four silent
    failure modes filesystem watching has to prevent.
11. [crates/sync/README.md](../crates/sync/README.md) — why concurrency means
    conflict, and why convergence is a different property from correctness.
12. [crates/engine/README.md](../crates/engine/README.md) — how a change becomes
    work, and the one heuristic the engine leans on.
13. [crates/keys/README.md](../crates/keys/README.md) — why a lost phrase is
    unrecoverable, and what the key file does and does not defend against.
14. [crates/peer/README.md](../crates/peer/README.md) — what actually crosses
    the wire, and what pinned identity does and does not protect.
15. [crates/signal/README.md](../crates/signal/README.md) and
    [crates/relay/README.md](../crates/relay/README.md) — the two services, and
    what each is deliberately unable to learn.
16. [crates/mobile-ffi/README.md](../crates/mobile-ffi/README.md) — the surface
    a phone calls, and the four platform constraints that shaped it.
17. [android/README.md](../android/README.md) — the app, and the things about
    Android that dictated its shape: the keystore, edge-to-edge drawing, and a
    background scheduler that decides when you run. The fourth, the 16 KB page
    size, is in [phases/phase-5-mobile.md](phases/phase-5-mobile.md#16-kb-page-alignment)
    and `scripts/android-build.sh`.

And when you want to run it for real, rather than on one machine:

18. [anywhere.md](anywhere.md) — what has to be reachable for a phone to sync
    from a train, what does not, and a way to get there for nothing.
19. [packaging/server/README.md](../packaging/server/README.md) — the two
    services on a host of your own: unit files, ports, and which of them may
    face the internet.

And when you want to close the measurements still outstanding:

20. [measuring-connectivity.md](measuring-connectivity.md) — how to measure the
    direct-connection rate, which is the number the relay bill depends on. The
    other open measurement, whether sync survives a phone's battery and its
    platform's patience, needs a phone left alone for a day — see
    [phases/phase-5-mobile.md](phases/phase-5-mobile.md).

---

## 8. Conventions

- **Rust** for anything on a device: engine, storage, crypto, networking. That
  includes the phones — [`crates/mobile-ffi`](../crates/mobile-ffi/) is the
  only place platform languages appear, and it is a seam rather than a second
  implementation.
- **Rust for the services too.** The rendezvous service and the relay are
  Rust, because they share wire types and key derivation with the devices — see
  [decisions/0015](decisions/0015-control-plane-in-rust.md), which revised the
  architecture's original choice of Go. Whether accounts and billing are Rust
  as well is left open until that work starts.
- Decisions go in `docs/decisions/`, numbered, never deleted. If a decision is
  reversed, the old file gets a status line pointing at its replacement. The
  record of what we believed and why is worth more than a tidy directory.
- Every phase gets a document in `docs/phases/` recording what it produced and
  what it measured, written as part of the phase rather than after it.
- Claims about performance carry the measurement or they are not made. "Fast"
  is not a specification.
- **Nothing blocking runs on an async task's thread.** Storage goes through
  `block_in_place`, and any other blocking call, a D-Bus call for instance,
  through `spawn_blocking`. Cargo merges a dependency's features across
  everything built in one command, so a library that is harmless in one crate
  can be built, for all of them, to start a Tokio runtime of its own, which
  panics on a Tokio thread. It did, with zbus: see
  [phases/phase-4-product.md](phases/phase-4-product.md#notifications-stopped-at-the-first-failure).

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
