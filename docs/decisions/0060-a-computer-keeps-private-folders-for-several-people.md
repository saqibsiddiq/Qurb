# 0060 — A computer keeps private folders for several people, and qurb keeps no copy of what it sends

**Status:** Accepted, 2026-10-10, with the owner's answers at the end — steps
1 (no copies kept) and 2 (guests) built, steps 3–6 not yet; supersedes [0023](0023-one-person-per-account.md) for people who
are not the computer's owner, and amends
[0030](0030-sending-a-file-to-one-device.md) rules 1 and 4.
**Date:** 2026-10-10

## What was asked

The owner, on 2026-10-10, items 2, 3, 4 and 5 of a numbered list:

> 2. the primary idea of the application is to share files across different
>    devices, but when im sending data from one device to another, the app
>    also holds that file along with the system, so i think the app itself
>    must not store file, its just a medium for data transfer between
>    different devices
> 3. the original idea is that, qurb application can allow desktop to have
>    multiple users with separate folders, the respected users file cannot be
>    opened by anyone without permission, the user can access that in real
>    time if the laptop is online
> 4. the users will have the option to send files in there private folder or
>    directly in the download folder of the laptop
> 5. we can can optionally use phone security like fingerprint scanner, face
>    unlock, pattern, etc for someone to open there folder in the desktop or
>    if there exist any better way to authorize

That describes a different product from the one built. What exists is
Dropbox for one person: one key, every device that person's, and a folder
that copies to all of them. What is asked for is a computer that keeps
private folders for several people, each reaching theirs from their phone
while the computer is on, with qurb a way for files to travel rather than a
place they pile up.

## Why this does not fall to 0023's argument

[0023](0023-one-person-per-account.md) refused people inside qurb: two
people sharing a login can read each other's store, so a separation qurb
drew would be "security theatre".

That holds while the computer has every key. Here it would not. A guest's
folder is encrypted with a key that lives on the guest's phone. The computer
stores ciphertext under names it cannot read, and never has the key except
while the guest has opened the folder at it. Anyone who can read the store
can read only that. 0023 stays true for a folder while it is open on that
computer, and for the computer owner's own files.

## What carries over

- **Holding a device's own files without showing them** (0036). This is the
  nearest thing to a guest's folder: content kept in the chunk store, counted
  against the computer's allowance, served back only to its owner. What it
  lacks is a separate key, and names hidden from the computer. Today the
  laptop's index holds a phone's held filenames in plain text.
- **Sending to one device** (0030, 0059), **pairing approved by number**
  (0053), **opening on demand** (0045, the phone's file provider), **push
  through the rendezvous service** (0028).
- Everything underneath: chunking, encryption, QUIC, hole punching, the
  relay.

## Proposal

### People

**A person is a key.** One person's devices share it, as they do now. A
computer belongs to its owner and keeps folders for **guests**: other people,
each with their own key, who join with a code and an approval by number,
like pairing today, but **without taking the computer's key**. That is the
reverse of [0052](0052-the-key-travels-with-the-code.md).

### A guest's folder on the computer

- **Encrypted with the guest's key**, derived for that computer, so that
  giving it to one computer gives nothing for another.
- **Names hidden.** The computer stores opaque entries. The guest's device
  gets the list and reads the names itself.
- **No sharing of bytes between people.** Chunks are named with a key of the
  guest's. Deduplicating across people would tell the owner a guest holds the
  same file as them.
- **What the owner sees**: that the guest exists, how much space they use, a
  limit they can set, and *Remove*, which deletes the folder. It is their
  disk, and a guest should be told so. Not names, not contents.
- **Live from the phone**: while the computer is reachable, the guest's
  phone lists the folder and opens a file by fetching it. Nothing stays on
  the phone unless the guest keeps it there.

### Sending (items 2 and 4)

From a phone, a guest chooses where a file goes:

- **My folder on Saqib's laptop** — private, as above;
- **Saqib's laptop's Downloads** — to the owner, as a send is today
  ([0037](0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md)).

**qurb keeps no copy of what it sends.** Today the sender stores the bytes
until the recipient has them, then keeps them on until space runs short. On
the S23 that was 1.6 GB for one video. Proposed instead:

- A file picked from the phone or the computer is read **where it is**, when
  the recipient collects it, and checked against what was picked. If it was
  changed or deleted in the meantime, the send says so instead of sending
  something else. Nothing is copied to make a send.
- Android's share sheet lends a file only briefly. A file shared that way is
  copied once, and the copy is deleted **the moment the recipient confirms**.
- Nothing is kept after delivery, on any device.

The cost: a send to a device that is off completes only if the file is still
there, unchanged, when that device comes on.

### Opening a folder at the computer (item 5)

The desktop window lists the people it keeps folders for. *Open* asks that
person's phone, which shows *Open your folder on Saqib's laptop?* behind its
fingerprint, face or screen lock (Android's BiometricPrompt, with a keystore
key that cannot be used without it). Approved, the phone sends the folder key
over the paired connection. The window shows the folder until it is closed,
ten minutes pass without use, or the phone says *Lock*. Then the computer
forgets the key.

Better ways, considered:

- **A passkey** with the PRF extension: the computer shows a QR code, any
  phone approves with its passkey, iPhones included, and the folder key is
  derived from the answer. Cross-platform and phishing-resistant, but a
  browser stack the desktop does not have yet. Later.
- **A hardware key** (YubiKey, FIDO2 hmac-secret): the same idea, no phone.
  Later, for anyone who wants it.
- **A passphrase**, typed at the computer: the fallback when the phone is
  not to hand.

The phone's approval comes first: pairing and approval by number already
exist, and the key never leaves devices the person holds except for the
session.

### Limits, stated plainly

- **Open is open.** While a folder is open on the computer, anyone at that
  login can read it. Opening a file in another app means a decrypted copy;
  it would live in memory-backed storage (`/run/user`) and be removed on
  locking. Someone with administrator rights on a running computer could
  capture the key while it is open.
- **A guest who loses every device**, with no Google backup of the phone's
  key, loses the folder. Nobody else can open it, which is the point.
- **The owner can delete a guest's folder**, and the computer being off means
  the guest cannot reach it. A guest's only copy on someone else's computer
  is a trust decision; the app should say so when a guest joins.

### Protocol

New wire messages: joining as a guest, storing into and listing a guest's
folder, and the unlock. Devices negotiate the protocol in the handshake, so
it becomes `qurb/3`, and every device updates together. Existing pairings
carry over.

## Order of work, each piece shippable

1. **No copies kept** (item 2) — on its own; useful now.
2. **Guests**: joining a computer without its key; per-person keys side by
   side on one computer.
3. **A guest's folder**: storing, listing and fetching, names hidden; the
   phone's choice of *My folder* or *Downloads*.
4. **The phone**: *My folder on Saqib's laptop*, browsed live.
5. **The desktop**: the people it keeps folders for; *Open* with the phone's
   approval; locking.
6. **Watched on hardware.** A guest needs a second person's phone. Until one
   is borrowed, the emulator plays the guest and the S23 the owner.

Roughly several weeks in all, the folder and the phone's live browsing the
largest parts. A guess, not a measurement; it will be corrected in the
phase document as the work goes.

## Questions for the owner

1. **The folder that copies to every device** of one person: keep it for a
   person's own devices, or retire it, so that qurb is sending, private
   folders and live access only?
2. **Can the computer's owner ever open a guest's folder?** Proposed: never,
   except with that guest's approval each time.
3. **The owner's own files on the computer**: an ordinary folder, as now, or
   locked like a guest's?
4. **Item 2's cost**: is it right that a send to a device that is off fails
   if the file is changed or deleted before that device comes on?
5. **A guest's folder on their phone**: browsed live and fetched when opened,
   as proposed, or a full copy kept on the phone?

## The owner's answers, 2026-10-10

1. **The folder that copies to every device: kept, not central.** It stays
   for one person's own devices. Sending and private folders become the
   heart of the app.
2. **The owner opens a guest's folder only with that guest's approval**, each
   time, from the guest's phone.
3. *Not asked.* The owner's own files stay an ordinary folder, as now;
   locking them like a guest's can be added later.
4. **No copies.** A send reads the original when it is collected, and says so
   if it changed or went. A file from the share sheet is copied once and
   deleted the moment it arrives.
5. **A guest's folder on the phone is live**, listed from the computer and
   fetched when opened.

## Progress

### Step 1, no copies kept — built 2026-10-10

- **The engine.** A send records where its file is (`send_sources`, schema
  19) and stores nothing (`Placement::by_reference`). Its chunks are read
  from the file when collected and checked against the hashes taken when it
  was sent (`Store::chunk_from_source`), and they count as held for the
  question "can you give me this?" (`chunk_in_send`). The file is read once
  at send time, in pieces, to describe it (`chunker::chunk_reader`, which
  cuts exactly as the in-memory chunker does).
- **Called off, and said.** `Store::check_sends` runs in housekeeping, every
  five minutes on the desktop and after each sync on the phone. It calls off
  a waiting send whose file is gone, or changed, by size and time, reading
  the file again when only its time moved. It writes *not sent: it changed
  after it was sent* in the history, where both apps' "something failed"
  notification comes from. A change that keeps size and time is caught as
  it is served, when the chunk does not match, and called off at the next
  check.
- **Let go of once collected.** `Store::tidy_sends` stops reading the file,
  deletes a copy qurb made, gives back a document's read permission, and
  drops the references to chunks nothing here holds.
- **The phone.** A file from the picker is read in place through a callback
  the app supplies (`DocumentOpener`): it takes Android's persistable read
  permission and lends the engine a file descriptor each time. A file from
  the share sheet is copied into the app's files, not its cache, which
  Android may empty, and the copy is deleted once collected. A file already
  in qurb is read from the folder.
- **Sends made before** keep their sealed copies, which still go when space
  runs short on a desktop or when asked for by name on a phone.

Tests: `a_send_keeps_no_copy_and_is_read_from_its_file`,
`a_send_whose_file_is_deleted_is_called_off`,
`a_send_whose_file_changed_is_called_off`,
`a_change_that_hides_from_the_check_is_caught_when_served`,
`a_file_touched_but_unchanged_still_goes`,
`a_collected_send_lets_go_of_its_file` (storage);
`a_send_is_read_from_its_file_and_keeps_no_copy`,
`a_send_changed_before_collection_is_not_delivered` (over a connection);
`a_send_keeps_no_copy_and_a_shared_one_goes_once_it_arrives` (the phone's
FFI); `a_stream_is_cut_exactly_as_a_buffer_is` (the chunker). Four tests that
asserted the old rule were rewritten.

**Not done in step 1:**

- *Add files* on the phone still copies a file into qurb's folder. That is
  putting it in qurb, not sending it. Under step 3 it becomes putting it in
  the person's folder on the computer, with no copy on the phone.
- A called-off send is said in the history and the notification. Neither
  window nor phone yet offers *Send the new version* beside it.
- **Not watched on hardware**: the picker's document read in place on the
  S23 after the app was closed and opened again, and a send from the phone
  called off when its file was deleted.

### Step 2, guests — the design, 2026-10-10

**A guest is a peer of another person.** The peers table gains who a peer
is to this device: one of the same person's devices, as every peer has been;
a **guest**, another person visiting this computer; or a **host**, a computer
this device visits. A guest holds its own key, so pairing as a guest skips
0053's same-key check on purpose. Telling the two apart is what everything
below rests on.

**Pairing as a guest.** The computer shows a code for *Add a person*, marked
as a guest invite. The guest's device, set up with its own key, joins with
it (`Request::Visit`). The computer's person approves by the same six digits
as any pairing (0053). Nothing about the computer's key is sent. The
computer answers with a **meeting secret**, 32 random bytes over the
authenticated connection, which only those two devices hold.

**Finding each other.** The rendezvous service matches devices by a group
derived from the shared key, which a guest does not have. So both devices
also announce under a group and member derived from the meeting secret: one
more rendezvous connection per guest. The service needs no change. That
leaks nothing between people. Two guests of one computer are in different
meetings and never see each other's addresses, and the service still sees
only opaque values. Local beacons and the relay stay keyed to one person in
this step: a guest reaches a computer through the rendezvous service, which
carries its local addresses too.

**What each shows the other.** A new audience, `Guest`, sees only what is
addressed to it: sends into its vault, and in step 3 its folder. Never the
shared area, never this device's own vault, never anything held. Manifests
and chunks are refused for anything else, however asked. The audience is
chosen from the peer's relation on every request, not trusted from the
connection. And a device syncing with a peer of another person takes only
deliveries from it. A shared-area version from another person is never
adopted, even if one were offered.

**Sending.** A guest sends into the computer's Downloads the way any device
sends today: the computer collects from the guest. The computer can send to
a guest the same way.

**Protocol.** Two new messages, `Visit` and `Welcome`. A build without them
refuses a guest invite, which is safe, so the protocol stays `qurb/2` for
this step. Step 3's hidden-name folders are what change the meaning of
existing messages and move it to `qurb/3`.

### Step 2, guests — built 2026-10-10

As designed above, with two changes found while building it:

- **Relations are a table, `peer_relations`, not a column.** A test of the
  upgrade path runs the newest migration again, and SQLite cannot add a
  column twice. No row is one of the person's own devices.
- **Asking a guest what it holds is skipped.** A device asks its peers about
  copies recorded for them (0055). A guest's copies are deliveries in its own
  vault, which its server rightly shows nobody. Asking would only learn
  "no", and a "yes" would have marked the copy as one to ask for.

Built on all three: `qurb pair --guest` and `qurb visit`; *Add a person* in
the window's *Add a device*, guests listed apart, and the approval saying
*wants to visit this computer as a guest*; on the phone, *Visit someone's
computer*, any guest code scanned or typed treated as a visit, and
*Computers you visit* listed apart, offering no Private Vault backup.

Tests (`crates/peer/tests/guests.rs`): a guest invite round-trips and says
so; a guest visits without the key, both sides holding the same meeting
secret; a guest invite gives no key and pairs nobody as one's own; an
ordinary invite refuses a visit; over a real connection a guest is shown
only what was sent to it and refused a shared file's manifest; a guest takes
only deliveries even when offered more; a guest's copy never counts as one
to ask for. With the line choosing the guest audience disabled, the
visibility test fails, showing the guest the computer's shared file.

**Watched**: on the laptop, two scratch folders with different keys and a
rendezvous service of their own on a spare port. One was welcomed as the
other's guest from the command line, approved at the matching number
(151 739). Both daemons ran, found each other through their meeting, and
synced one file each way. The guest got what it was sent and not the
computer's shared file; the computer got the guest's send in Downloads. The
window's guest screens were looked at against its fixtures, and the smoke
test passed.

**Not done in step 2:**

- Local beacons, the relay and push stay keyed to one person. A guest finds
  a computer only through the rendezvous service, which carries its local
  addresses too. A guest phone is not woken by push when the computer has
  something for it; it collects at its next sync.
- A phone cannot welcome guests; only computers show guest codes.
- **Not watched on hardware.** A second person's phone is needed; until one
  is borrowed, the emulator can play the guest.
