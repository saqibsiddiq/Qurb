# 0059 — A send is not its bytes: the same file sent again arrives again

**Status:** Accepted — built in the engine, the command line, the desktop and
Android; amends rule 2 of [0030](0030-sending-a-file-to-one-device.md) and
what [0037](0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md) remembers
**Date:** 2026-10-10

## What happened

The owner, on 2026-10-10: "due to deduplication, im unable to share the same
file again even after removing from the laptop, when sending the duplicate file
there must be a message that this file already exists in the system and ask if
you still want to proceed".

Deduplication was not the cause: storing the same bytes once never stops them
moving. Three things together were:

1. **The recipient remembered a delivery by its bytes.** Rule 2 of 0030 keyed
   "taken once" by content, so that a send its sender kept offering would not
   arrive again after the person deleted it. That also refused every later
   send of the same bytes, from anyone, for good. On 2026-10-05 a phone set
   up again sent the laptop a 1.7 GB video it had sent that morning; it never
   arrived, and the fix of 2026-10-07 only made the sender stop waiting.
2. **The sender did not make a new send.** Sending a file under a name it had
   already sent there, with the old send still kept, was "unchanged" to the
   store, so no new version existed for the recipient to take.
3. **The sender counted collection by bytes.** A send is collected when the
   recipient reports holding its content. Reported the first time, the second
   send counted as collected the moment it was made.

Nobody was told at any point. The send looked delivered on both sides.

## Decision

**A send is identified by who sent it, under what name, and which version of
that name.** Offered again by a sender that reappears, it is the same send,
and still taken once: a file the person deleted stays deleted. Sent again, it
is a new version and a new delivery, even when the bytes are identical.

- **The recipient** records each send it takes in `deliveries` (schema 18),
  keyed by sender, path and version vector, and checks that
  (`Db::delivery_taken`). What it took before is in `taken`, keyed by
  content, and is read only for a version made before it was taken. That is
  the old send offered again, so nothing already received arrives twice
  after the upgrade, and the same bytes sent since still do.
- **The sender** makes every send a new version (`Placement::fresh`), and
  forgets the recipient's earlier record of holding those bytes in its vault
  (`Db::sending_again`). So a send made again waits until the recipient takes
  it. A record that the recipient holds them in the shared area is kept: that
  is a copy it can hand back, whatever was sent.
- **The recipient tells the sender about each send once.** It does that as it
  takes the send, and again at most once in the holdings report, in case the
  first word was lost (`deliveries.acknowledged`). Before, that report
  remembered only the bytes, so a lost word about a second send of them would
  never have been repeated.

**Before sending, the person is told what went there before, and asked.**
`Store::sent_before` names each file this device has sent to that device,
under what name and when. Only a file the same size as something sent there
is read, so picking a folder of photos does not mean hashing all of them.
Then:

| where | asked how |
|---|---|
| desktop window | the send sheet shows *Sent to Laptop before* with each file, and *Send it again* or *Leave it out* (*Don't send* when every file is one) |
| phone | the same sheet, from Home, a file's *Send to device…*, or a device; the share sheet asks in a dialog |
| command line | `qurb send` lists them and asks *Send it again? [y/N]*; with no terminal, or a no, they are left out and it says so; `--again` sends them |

The message says the device *may still have it*. The sender cannot know: a
file sent to a desktop is an ordinary file in Downloads, and the person may
have deleted it.

## What this does not do

- **It does not ask the recipient.** "Already on that device" would be the
  more useful message. But it needs the device online, and a desktop does not
  track files in Downloads after they arrive (0037).
- **It does not stop a different device sending the same bytes.** A phone set
  up again has no record of the old phone's sends, so it is not asked, and
  the laptop gets a second copy. That is what the owner asked for. The copy
  is filed beside the first, under a free name, if the first is still there.
- **A redundant word from the recipient can still mislead the sender, rarely.**
  If the same bytes are sent again under a *different* name, and the
  recipient's one retried word about the *first* send arrives after that, the
  second send counts as collected early. That needs the retry to fall between
  taking the first and the second send was made, in the same sync. Telling
  sends apart on the wire would close it, at the cost of a protocol change
  every device must take at once. Not done.
- **Old records compare two devices' clocks.** A send taken before this
  change is matched by when it was made, by the sender's clock, against when
  it was taken, by the recipient's. Clocks days apart could take an old send
  once more, or refuse a new one, for that content only.

## Tests

`a_file_sent_again_arrives_again`, `the_same_file_from_another_device_is_another_delivery`
and `a_sender_is_told_about_each_send_once` (`crates/peer/tests/vaults.rs`);
`a_delivery_taken_before_the_upgrade_is_not_taken_twice`,
`the_same_file_sent_again_is_a_new_send` and
`a_file_never_sent_there_is_not_mentioned` (`crates/storage/tests/vault_send.rs`).
`a_delivery_is_taken_once_and_stays_deleted` stands unchanged. The test it
replaces, `a_delivery_sent_again_is_acknowledged_and_not_taken_twice`,
asserted the behaviour this decision reverses.

## Checked, and not

- The command line, on two folders paired on the laptop: sent once; sent
  again with no terminal, which listed it and left it out; sent with
  `--again`, which sent it.
- The desktop's question, against the window's fixtures.
- **Not yet watched**: a file sent again between the phone and the laptop,
  arriving again, and the phone's question on the S23.
