# Glossary

Terms used throughout this project, defined plainly. Where a term has a
standard meaning in distributed systems, the definition here is the one that
matters for qurb specifically.

---

**Area** — Where a path lives: the *shared area*, which every device the
path's folder is shared with converges on, or one device's *vault*. In the
index, `files.scope` — `NULL` for shared, a device id for a vault. On the wire
each tree entry names one of four, because holding somebody's vault has a
direction: *shared*; *sent* (into the receiver's vault, by a send); *held* (the
receiver's own file, which the sender keeps for it); *hold* (the sender's own
file, for the receiver to keep). The move to four is what made the protocol
`qurb/2`. See [decisions/0029](decisions/0029-two-areas-shared-and-private.md).

**Availability** — Two meanings. *Of a file*, as the interfaces show it:
**here** (the bytes are on this device), **elsewhere** (freed here, another
device has them) or **only here** (no other device is known to have them —
losing this device loses the file). Three values rather than two because
"free up space" and "delete my only copy" must never look alike. *Of the
system*: whether files can be reached when every device is off — answered by
replicas ([decisions/0006](decisions/0006-availability-gap.md)).

**Beacon** — A small encrypted announcement a device multicasts on its
local network — who it is, where it can be reached — so two devices on the same
Wi-Fi find each other with no server. Encrypted under a key from the master
key, so a stranger on the network sees random bytes.

**BLAKE3** — The cryptographic hash function used to name chunks. Chosen over
SHA-256 because it is several times faster and parallelises well. In practice
hashing is not our bottleneck (disk I/O is), so the speed matters less than
expected — but it costs nothing to have.

**CAS — Content-Addressable Storage** — A store where the name of a piece of
data is derived from the data itself, specifically its hash. Storing the same
bytes twice is a no-op. Corruption is detectable by re-hashing. See
[CODEBASE.md](CODEBASE.md) section 2.1.

**CDC — Content-Defined Chunking** — Splitting a file at boundaries determined
by the content rather than by byte offset, so that inserting or deleting data
does not shift every subsequent boundary. The reason a one-byte edit to a large
file costs kilobytes rather than gigabytes. We use the FastCDC algorithm.

**Chunk** — A piece of a file, between 128 KiB and 2 MiB, averaging 512 KiB. The
unit of deduplication, storage, transfer, and integrity checking. Chunks are
compressed and encrypted individually.

**Chunk GC — Garbage Collection** — Reclaiming disk space by deleting chunks no
file references any more. Hard because deduplication means one chunk can be
referenced by many files and many versions, so deletion requires reference
counting. Built in Phase 1 before anything else, because a collector that is
slightly wrong destroys data silently; the daemon runs it every five minutes and
a phone after each background sync, keeping superseded chunks for seven days.

**Cone NAT** — See *NAT*. A router whose external port mapping is the same
regardless of destination. Hole punching works. The good case.

**Conflict** — Two devices changing the same file without either having
seen the other's change, which the *vector clocks* reveal. Both versions are
kept: one keeps the name, the other is saved beside it as
`name.conflict-<device>-<UTC time>.ext`. A person settles it later by keeping
one, the other, or both; the one not kept goes to *Recently deleted*. See
[decisions/0005](decisions/0005-conflict-resolution.md) and
[0043](decisions/0043-settling-a-conflict.md).

**DERP — Designated Encrypted Relay for Packets** — Tailscale's name for relay
servers that forward encrypted packets between peers who cannot reach each other
directly. Runs on port 443 so it survives restrictive corporate firewalls.
Relaying costs bandwidth, so the fraction of connections needing it is a
number with a direct dollar cost.

**E2EE — End-to-End Encrypted** — Data is encrypted by the sender and decrypted
by the recipient, with no intermediate party able to read it. Our relays forward
bytes they cannot interpret.

**Eviction / freeing** — Deleting a device's local copy of a file while
keeping everything the index knows about it, so it can be fetched back. Done by
the *storage cap*, by a person choosing *free* on one file, or by a folder
*kept remotely*. Only ever done when another device is known to hold the same
bytes.

**FastCDC** — The specific content-defined chunking algorithm we use. Faster
than the older Rabin fingerprint approach for the same quality of boundaries.

**FFI — Foreign Function Interface** — The mechanism by which Swift and Kotlin
call into the shared Rust core on mobile. Done with UniFFI, which generates the
Kotlin and Swift from the Rust — `crates/mobile-ffi`.

**Fingerprint** — The hash of a device's certificate. Pairing carries it
out of band, in the code or QR, and every later connection is checked against
it — *pinned*, so no certificate authority can vouch for an impostor.

**Holder** — A device that keeps a copy of another device's private files,
in its store and never in its folder, so the owner — usually a phone — can
free its own copies. A holder lets a file go only on the owner's deletion. See
[decisions/0036](decisions/0036-a-phone-keeps-its-own-files.md).

**Hole punching** — The technique that lets two devices behind NAT connect
directly. Both send packets to each other simultaneously; each outbound packet
opens a hole in its own router's filter through which the other's packet can
return. Fails against symmetric NAT.

**Index** — The SQLite database on each device: every path, its chunk list,
its version vector, where its bytes are, and a history of what happened. Its
*schema* number counts the migrations it has had (15 at 2026-09-28); it is
copied before each migration and refused by an older build.

**Kept remotely** — One device's choice about one folder: its files stay
listed, new versions arriving from elsewhere are recorded without their bytes,
and local copies are freed where another device has them. A file is fetched
when asked for. Not synced — it is this device's business. See
[decisions/0045](decisions/0045-a-folder-kept-remotely.md).

**Manifest** — The record mapping a file path to its ordered list of chunk
hashes. Reconstructing a file means fetching its manifest, then fetching each
chunk it names.

**Materialised** — Whether a device is holding a file's actual bytes, as
opposed to merely knowing the file exists. A syncing device materialises its
files into a folder; a *replica* materialises nothing. The distinction is
recorded per file, because a file that is absent on purpose must never be
mistaken for one the user deleted.

**Member ID** — A blinded identifier a device announces itself under at the
*rendezvous service*, derived from the master key and the device's fingerprint.
The service can route between members of a group without being able to tell
which device, or whose, any of them is.

**Merkle DAG** — A structure where each node names its children by hash, so any
change to a leaf changes every hash on the path to the root. Lets two devices
compare an entire directory tree by exchanging one hash, then descending only
into the parts that differ.

**NAT — Network Address Translation** — What your home router does: many
devices share one public IP address. The reason two devices on different home
networks cannot simply dial each other, and therefore the reason STUN, hole
punching, and relays exist.

**Noise Protocol Framework** — A toolkit for building cryptographic handshakes,
which WireGuard uses. The architecture planned its `IK` pattern; what was built
is TLS 1.3 inside QUIC with each side pinning the other's certificate
*fingerprint*, which gives the same mutual authentication. Whether to move to
Noise later is open — see
[decisions/0011](decisions/0011-peer-identity-pinning.md).

**Nudge** — A message saying a peer has something for you: who, never what. The
rendezvous service forwards it to peers that are connected and keeps it for
peers that are not, so a device that was asleep when a change happened learns of
it on waking rather than at its next poll.

**Pairing** — Introducing two devices so each trusts the other: one shows a
code — a QR code, or words to type or read aloud — and the other enters it.
The code carries the shower's *fingerprint*, and expires after five minutes.
Removing a device undoes it; see
[decisions/0041](decisions/0041-removing-a-device.md).

**Protocol version** — What two devices speak, carried in the TLS
handshake (ALPN) as `qurb/2`. Devices with different versions refuse to
connect rather than misunderstand each other, so every device is updated
together when it changes.

**Push** — Waking a device that cannot be told anything because it is asleep.
On Android this is Firebase Cloud Messaging; on iOS it would be APNs, and on
both platforms it is the only way in. The message carries nothing but "go and
sync". See
[decisions/0028](decisions/0028-waking-a-sleeping-device.md).

**QUIC** — A transport protocol over UDP that includes encryption and supports
many independent streams on one connection. A stalled stream does not block the
others, unlike TCP. We use the `quinn` Rust implementation.

**Recently deleted** — Where a file goes when it is deleted, here or on
another device: out of the folder into `.qurb/trash/`, for thirty days.
Restoring writes it back as a new version, so it returns on every device. See
[decisions/0042](decisions/0042-recently-deleted.md).

**Recovery phrase** — 24 words (BIP-39) that *are* the master key, written
on paper. With them, any device can be set up again; without them and every
device, the files are gone — nobody, including us, can recover them.

**Relay** — A server that forwards encrypted traffic between two devices
that cannot reach each other directly; `qurb relay`. It carries a QUIC session
it cannot read, over TCP on port 443. qurb's version of what Tailscale calls
*DERP*.

**Rendezvous service** — The server that lets two devices learn each other's
addresses and punch at the same moment. It sees blinded identifiers and
addresses, never files, filenames or keys. Also called *signalling*.

**Replica** — A device that holds content without a person using it: always on,
no folder, materialising nothing and originating nothing. It exists so that two
devices which are never awake at the same moment can still exchange files. Run
with `qurb replica`.

**Send** — Putting a file into one device's *vault*: that device receives it,
no other does. No copy is kept: the file is read from where it is when the
recipient collects it, and a send can be cancelled until then. Changing or
deleting the file first calls the send off
([0060](decisions/0060-a-computer-keeps-private-folders-for-several-people.md)). On a desktop, a
received file is saved to `Downloads/qurb` as an ordinary file. See
[decisions/0030](decisions/0030-sending-a-file-to-one-device.md).

**Sharing rule** — A small file at `.qurb-sharing/<folder>` naming the
devices a folder in the shared area is shared with. It syncs like any other
file; each device derives its `shares` tables from the rules, and refuses a
folder's files to devices not named. No rule means every device. See
[decisions/0044](decisions/0044-sharing-with-chosen-devices.md).

**Single-copy storage** — Keeping each file's bytes once rather than twice. On a
device with a folder, the file *is* the payload store, and the chunk store keeps
only what the folder cannot supply — otherwise every synced file would cost
twice its size. See
[decisions/0024](decisions/0024-the-file-is-the-payload-store.md).

**Storage cap** — A limit on how much disk qurb may use for a folder. Over it,
local copies of the coldest files are dropped while the index keeps knowing
about them — but only ones another device is known to hold, never the last copy.

**STUN — Session Traversal Utilities for NAT** — A tiny protocol for asking a
public server "what address do my packets appear to come from?" The answer is
what peers exchange in order to attempt hole punching. Querying two different
STUN servers from the same local socket and comparing the answers is how we
classify a NAT as cone or symmetric.

**Symmetric NAT** — See *NAT*. A router that assigns a different external port
per destination, which breaks hole punching because the address a peer learns
from STUN is not the address it will be contacted on. Connections involving
symmetric NAT usually need a relay. The bad case.

**Tombstone** — A record marking a file as deleted, propagated to peers so they
delete their copies too. Necessary because the absence of a file is not
self-describing: without a tombstone, a peer cannot distinguish "deleted" from
"not yet received."

**Vault** — A device's private area. Others may put files into it — a *send*,
or a *holder* returning a file — but nobody but the owner may list it or read
it back, and the server side checks that on every request. The apps call it
**Private Vault**. See *Area*.

**Vector clock** — A per-device counter map, like `{laptop: 42, phone: 12}`,
used to determine whether one change happened before another or whether they
were concurrent. Concurrency means conflict. See
[decisions/0005-conflict-resolution.md](decisions/0005-conflict-resolution.md).

**Wake token** — The Firebase token a phone gives the *rendezvous service*
so it can be woken by *push*. Kept by the service in `wake-tokens.json`, readable
only by its owner, so a restart does not leave phones unreachable until they
next sync.

**Zero-knowledge** — Our servers hold no information that would let them read
user data. The strong version of the privacy promise, and the source of the
hardest product constraint: a lost key means unrecoverable data.

**Zstd** — The compression algorithm applied to chunks before encryption. Fast,
with a good ratio on text. Skipped for already-compressed formats where it would
burn CPU for nothing.
