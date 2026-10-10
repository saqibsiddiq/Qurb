# 0060 — A computer keeps private folders for several people, and qurb keeps no copy of what it sends

**Status:** Accepted, 2026-10-10, with the owner's answers at the end — not
yet built; supersedes [0023](0023-one-person-per-account.md) for people who
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
