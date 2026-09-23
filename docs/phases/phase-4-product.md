# Phase 4 — Desktop product

**Status:** in progress
**Target:** months 8–10

Turning a working engine into something a person can run. The phase the roadmap
warns is not the fun part and is a full quarter.

## Progress

| area | status |
|---|---|
| a daemon that runs | ✅ [`qurb`](../../crates/qurb/) |
| the commands around it | ✅ init, enrol, pair, join, run, status, verify, reclaim, fetch, config |
| running the services | ✅ `qurb signal`, `qurb relay` |
| push, rather than polling | ✅ ~430ms, measured |
| protecting the key at rest | ✅ keystore and passphrase |
| storing files in parallel | ✅ 487 → 830-888 files/s |
| onboarding and the recovery phrase | ◐ works, in a terminal |
| installers | ⬜ not started |
| signed updates with rollback | ⬜ not started |
| observability | ◐ structured logs, nothing more |
| the interface | ◐ a window with the settings that matter |
| one daemon per folder | ✅ an advisory lock, not a convention |
| pairing | ✅ scan a QR code, once, and it stays paired |
| a folder that needs no path | ✅ `~/Downloads/qurb`, with a registry |
| storing each file once | ✅ the folder *is* the payload store |
| a storage cap | ✅ limit, eviction, fetch-back — and a slider |
| a replica anybody can run | ✅ `qurb replica` |
| syncing from another network | ✅ needs a reachable rendezvous; see anywhere.md |
| changes crossing at once | ✅ ~1s between idle daemons, no polling |
| the services deployable | ✅ systemd units, TLS, ports written down |
| garbage collection running | ✅ every 5 minutes, 7-day retention |

495 tests pass across eleven crates; clippy is clean.

## The interface

A system tray icon — [`crates/tray`](../../crates/tray/) — that runs the daemon
rather than talking to one. A socket between them would buy attaching to an
already-running daemon and cost a second surface to design, version and secure;
two daemons on one store collide on the SQLite lock regardless, so the choice is
really which single process runs. `qurb run` stays for machines with no screen.

This needed the daemon to say what it is doing. It had only ever had its log,
which is right for a terminal and useless to an interface: a person wants "up to
date, three devices, last synced two minutes ago", not a stream of events to
reconstruct it from. The daemon now publishes a `Status` on a `watch` channel —
watch rather than broadcast, because a display only wants the current value and
an icon catching up through stale summaries would be showing the past.

**It will not say "up to date" when no device has been reached.** A contented
icon beside a store that has not spoken to another device in a week is the kind
of lie that makes people stop trusting a sync tool, so `State::Alone` exists and
reads "no devices reachable". It also separates *no paired devices* from *paired
but unreachable*: one is a setup step, the other a network problem.

### GNOME has no tray, and says nothing about it

The nastiest part. GNOME removed the system tray and ships no StatusNotifier
host, so an icon there is invisible — and creating one still **succeeds**.
Nothing returns an error. A program trusting that return value is running,
unseen and unquittable.

So the session bus is asked whether anything has registered
`org.kde.StatusNotifierWatcher` before anything is built. With no watcher the
program says so, explains how to get a tray back, and keeps syncing with status
on stderr.

The check has to come before GTK is touched for a second reason, found by
running it: `tray-icon` is GTK-backed on Linux and *panics* if a menu is
constructed before `gtk::init`. An error path alone would never have run.

### Two bugs the fallback exposed at once

Having a display made two invisible problems visible immediately.

It reported **"0 files" with two in the store**, because counting happened after
the networking that had just failed. What a device holds is knowable without
reaching anything, and an interface showing zero because a service is down is
worse than showing nothing.

And when the daemon died, the status still said **"syncing"** — the failure
returned without reporting it, so the display kept showing the last thing that
had been true. It now records why it is stopping before it stops.

Neither was a new bug. Both had been there all along, invisible because nothing
was looking.

## Protecting the key at rest

The largest security gap the project had. The master key sat in a file readable
only by its owner, which defends against other users of the machine and against
nothing that can read the disk.

Three options now, and the difference between them is worth stating because a
user reading "end-to-end encrypted" will assume the strongest:

| | defends against | starts unattended |
|---|---|---|
| `file` | other users of the machine | yes |
| `keystore` | anyone reading the disk while it is locked | yes |
| `passphrase` | anyone who takes the disk *and* the session | no |

File remains the default, which looks like timidity and is not: a headless
machine may have neither a keystore nor anybody to type a passphrase, and a
device that cannot unlock itself is worse than one whose key sits in a file.
`qurb protect` changes it, and says what each option does before it does
anything.

Changing the protection changes the lock and not the contents — the key is read
out and written back, so nothing it protects becomes unreadable. The old copy is
removed only once the new one is in place, because a device that loses its key
halfway through being made safer has been made catastrophically less safe.

The keystore path is tested here against the Secret Service. Keychain and the
Windows Credential Manager go through the same library and are exercised by
nothing, which the documentation says rather than implying otherwise.

## The daemon

Until now nothing outside tests and examples could be run, and the demo put two
devices in one process — which meant the one thing that mattered for the Phase 3
kill criterion, *two machines on two networks*, was impossible.

`qurb run` watches a directory, keeps the store in step with it, serves peers on
every path it has, and pulls from each paired device it can reach.

## Three bugs that only running it could find

Every one of these passed the whole test suite.

### An invite offered an address nobody could dial

`qurb pair` bound `0.0.0.0` and put that in the invite. It is a true statement of
where the socket is listening — every interface — and completely useless to a
peer, which cannot connect to it.

Every test bound `127.0.0.1` explicitly and so never saw it. The failure appeared
at the *joining* device, which reported that it could not connect, making it look
like the joiner's problem.

The fix asks the routing table rather than listing interfaces: a UDP socket
*connected* to an arbitrary address sends nothing but makes the operating system
choose a source address, and the one it chooses is the one that would really be
used. Listing interfaces instead means guessing between a wired connection, a
wireless one, a virtual machine bridge and three container networks.

### Two devices started together took two minutes to find each other

The first sync attempt happens before the other device has announced, which fails
— and the retry backoff went straight to two minutes. Starting both daemons at
once, as anyone would, produced a system that appeared to do nothing at all.

The two failures are indistinguishable at the moment they happen and want
opposite treatment: a device still starting up should be retried in seconds, one
that is switched off should be left alone. So the backoff now doubles from five
seconds to a two-minute cap. Simultaneous startup went from 120 seconds to **9**.

### An edit took thirty seconds to cross

A device syncs when *it* changes something, or when its timer fires. An edit made
on the laptop therefore reached the desktop only when the desktop next asked.

**Now fixed.** A device holds one request open against each peer — "tell me when
your state differs from this" — and the answer arrives when it does.

Measured, two devices on this machine, the same edit three times: **433ms, 432ms,
431ms**, against up to ten seconds before. Most of what remains is the watcher
deliberately waiting to see whether the file is still being written, which is a
floor worth having rather than latency to remove. Thirty seconds of complete
idleness produced no log activity at all, so the responsiveness is not bought
with chatter.

It does not break the rule that **a peer can ask and never tell**: the device
that wants to know is the one asking, and the answer simply arrives later than
usual. A peer still cannot make anything happen.

A counter rather than a flag, because a flag can be missed — a peer told
"something changed" cannot distinguish a notification it has already acted on
from a new one. It says what it last saw, and gets an immediate answer if
anything has happened since. The counter deliberately does not survive a restart:
a peer holding a number from before sees one that does not match, concludes the
device it was watching has been away, and looks. Which is right.

## What running it confirmed

Two devices, two directories, real sockets: paired over the LAN, synced in both
directions, propagated a deletion, and resolved a genuine conflict — both
versions kept, the same conflict filename computed independently on each device,
and both agreeing on the resulting file list afterwards. `qurb verify --deep`
reports everything agrees.

## Pairing, once — and a folder nobody has to name

Three things stood between the daemon and something a person could be handed.

### "Paired" did not survive the daemon restarting — or rather, starting

The TLS verifier held its list of trusted devices as a `Vec<Fingerprint>`
snapshot, taken when the connection layer was built. `qurb pair` runs as a
*separate process*, so a daemon that was already running never learned about a
device paired afterwards. Pairing appeared to work — it wrote the fingerprint —
and then nothing connected until the daemon was restarted.

The snapshot is now a `TrustList`: an `Arc<RwLock<Vec<Fingerprint>>>` shared
between the verifier and the daemon, replaced wholesale every five seconds from
disk. Wholesale rather than appended, so that *forgetting* a device takes effect
too. Learning a new fingerprint also triggers an immediate sync rather than
waiting for the next interval.

Measured on two fresh devices on the development laptop, 2026-09-22: **five
seconds** from `qurb pair` completing to the devices connected, against never
without a restart.

### The pairing code was eight lines of base32 read off a screen

Now `qurb pair` renders the same invite as a QR code in the terminal, and the
Android app scans it with CameraX and ML Kit. The invite is unchanged — same
bytes, same expiry, same one-time use — so nothing about the security argument
moves. What changes is that a phone no longer needs a keyboard, and a person no
longer needs to transcribe a fingerprint to know they paired with the right
machine.

### Every command wanted a path

`qurb run ~/qurb` is fine once. It is not fine as the thing a person types
daily, and it is impossible as the thing a desktop launcher does. Commands now
default to a folder: `$XDG_DOWNLOAD_DIR/qurb`, falling back to
`~/Downloads/qurb`, with a small registry under `$XDG_CONFIG_HOME/qurb/folders`
recording which folders exist so the most recently used one wins. The legacy
`~/qurb` is still found if it is there.

Several people on one computer is answered by several operating-system
accounts rather than by a qurb-level notion of a user — see
[decisions/0023](../decisions/0023-one-person-per-account.md).

## Every file cost twice its size

Content lived in two places on every syncing device: the file in `~/qurb`, and
compressed, encrypted chunks of the same bytes in `~/qurb/.qurb/chunks`. Nobody
had noticed because the test corpora were small and the store was never
compared against the folder that produced it.

Measured on the development laptop, 2026-09-22: 7.6 MB of files in `~/qurb`,
7.5 MiB of chunk payloads holding exactly that same content.

The fix is that a materialised file *is* the payload store for the chunks it
holds — the chunk store keeps only what the folder cannot supply, and reads
that find no payload seek into the file and hash-check what they find.
[decisions/0024](../decisions/0024-the-file-is-the-payload-store.md) records
what that costs, which is not nothing: editing a file now makes its superseded
versions unreadable rather than recoverable.

`qurb reclaim` frees the duplicates an older store still holds. On the real
laptop store: 7.5 MiB across 15 chunks, `qurb verify --deep` clean afterwards.

This was found while sizing up the storage cap, and it had to be fixed first: a
cap that counts every file twice is a cap on half of what the user thinks they
are limiting.

## Telling a device how much disk it may use

`qurb config <dir> limit=10G`. Over the limit, qurb drops local copies of the
coldest files and keeps everything the index knows about them, so the path
still syncs, still lists, and comes back on `qurb fetch`.

The interesting part of this feature is not the freeing. It is the two refusals.

**A file is dropped only when another device is known to hold those exact
bytes**, recorded when this device adopts a version another device made. So a
device never evicts content it originated — the phone keeps the photo it took,
the desktop may drop the copy it was sent. A device that cannot free enough
stays over its limit and says so in the log, which looks like a bug and is not:
a limit is a promise about disk, and no number in a settings box outranks the
only copy of someone's work.

**Dropping a file must not look like deleting it.** A syncing device decides a
file was deleted by not finding it during a scan — so without care, a device
short of disk drops a file, the next scan calls that a deletion, and the file
is deleted on every other device. The index now records whether this device is
*holding* each file separately from whether the file is there, set before the
unlink and never after, and both the scan and the watcher skip a file that is
missing on purpose.

Measured on the development laptop, 2026-09-22, two devices on loopback over a
real QUIC connection with a 2 MiB limit and 4.8 MiB of content:

| | device that received the files | device that made them |
|---|---|---|
| dropped | 1 file, 3.0 MB | nothing |
| ended at | 1.9 MiB, under the limit | 5.0 MB, 2.8 MB over, deliberately |

Neither device recorded a deletion, and the other device still held both files.
`qurb fetch` brought the dropped file back on the following sweep, byte-identical.

Garbage collection is part of this and had never run: `Store::gc` was written
and tested in Phase 1 and had no caller outside tests, so superseded chunks
accumulated without limit — 82 MB of them on this laptop for one deleted file.
The daemon now collects every five minutes with a seven-day retention window,
before it checks the limit. Collecting first costs the user nothing; only then
is it fair to drop copies of files they still have.

Two things this leaves undone. An evicted file **vanishes from the folder** on
Linux, which has no placeholder API — `qurb status` is the only place that says
it still exists. And `qurb fetch` waits for the next sweep rather than nudging
the daemon, so a file can take up to two minutes to come back.

## A window with a slider in it

The tray icon was the interface, and on GNOME there is no tray, so on the most
common Linux desktop the interface was a window that could only be *read*. The
one setting people actually want to change — how much of the disk qurb may
have — was reachable only by typing `qurb config <dir> limit=10G`.

So the window now has it: a usage bar, a checkbox for whether there is a limit
at all, and a slider that runs from 1 GiB to the size of the filesystem the
folder is on. Offering more than the disk holds would be offering a number that
cannot mean anything.

Two details decided the design.

**The slider writes the settings file, and nothing else.** Not a socket to the
daemon, not shared memory — the file the daemon already reads. That makes
`qurb config limit=10G` in a terminal and the slider in the window literally
the same act, and neither can leave the other showing something stale. The
daemon re-reads it on every maintenance pass, so a change applies without a
restart; a setting that needs a restart is a setting people think is broken.

**The window reads the file too, rather than the daemon's published status.**
The daemon only republishes on a sync pass, so a slider driven from the status
would sit at its old value for up to two minutes after someone moved it —
visibly snapping back under their finger.

## Two daemons on one folder

Found by running one: the desktop app launched from the applications menu and
`qurb run` typed into a terminal are the same daemon with different faces, and
neither knew about the other. Two of them on one store contend on the SQLite
write lock, answer as the same device on the network, both reconcile the same
directory and both enforce the same cap — and none of that reports an error. It
is slow and confusing rather than broken, which is worse.

`crates/qurb/src/lock.rs` takes an advisory `flock` on the store for the life
of the daemon. The kernel releases it when the process dies, which a lock file
holding a PID could not promise after a crash. The second daemon now refuses
with a sentence saying what is already running.

The same run turned up two smaller things. The window showed nothing when the
daemon failed underneath it — it reported files and folders from a process that
had stopped — so a problem now appears in the window in red. And the folder
registry had test folders in it from an afternoon's work, which meant the app
launched from the applications menu opened a throwaway directory in `/tmp`
rather than the real one. That one is a lesson about test hygiene rather than a
defect: the registry is per-user configuration, and tests must not write to it.

## Nobody waits for a poll

Two idle daemons, both connected to the same rendezvous service, still took up
to two minutes to move a file neither was busy with — because a device that had
just changed something had no way to say so, and the peer had no reason to ask.

`Waiting { to }` says there is something for a member: who, never what. The
service forwards it to peers that are connected and keeps it for peers that are
not, delivering it the moment they appear. That second half is the one that
matters, because the device most in need of telling is the one that was asleep
when the change happened.

Measured, two idle daemons on this laptop: a file written on one was on the
other **one second later**. The sender recorded the change at 27.087, the
recipient logged the notice at 27.088, and the transfer finished at 27.099.

Notes collapse — fifty changes for one absent peer leave one note, since the
answer to "should I sync" is the same either way — and a device never nudges
back the peer the news came from, which would loop for ever.

## Two devices that were never reachable enough

Two connection defects turned up together, and both had been there all along.

**A device announced one address**: whichever its default route used. On a
laptop at home that is `192.168.1.5`, which is nothing at all to a phone on a
mobile carrier — so every connection from outside the house depended on
punching a hole through the home NAT, and an overlay network's address, which
would have worked first time, was never mentioned. `Endpoints.local` was
already a list and the candidates were already raced; only the filling-in was
missing.

That broke two tests immediately, and both were real.

Announcing the machine's other interfaces when the socket is bound to *one*
address advertises places nothing is accepting — a peer races addresses that
can only fail and concludes the device is unreachable. It enumerates only for a
wildcard bind now.

And racing four addresses instead of one grew a 128 MiB transfer from about
30 MiB of heap to 105. Dropping a handshake that has *already finished* leaves a
connection established at the far end holding the buffers of a transfer nobody
will use, and a device reachable on both a local network and an overlay hits
that every time. The losing paths are closed as they land.

**A rendezvous service restart disconnected every device permanently.** The
signalling connection went with it and never came back; the daemon kept syncing
on its timer, so nothing looked broken — it had simply stopped being reachable
and stopped being able to say it had news. Every deploy of a hosted service
would have done that to everyone. It reconnects now, doubling from a second to
a minute and resetting after a connection that lasted. Verified live: a daemon
left for two minutes with no service reconnected twenty-six seconds after one
appeared.

## Still to do

- **Running the *daemon* as a service** — a user unit, a launch agent, a
  Windows service. The two *server* services now have units; the thing a person
  runs on their own laptop does not.
- **Installers**, and the update mechanism with rollback. `packaging/install.sh`
  puts qurb in one user's applications menu and is not a package.
- **A replica that can free space.** It keeps every payload, because with no
  folder there is nowhere else for the bytes to live — and eviction works by
  deleting a file from a folder, so a cap on a replica reports the overrun
  rather than acting on it. Dropping chunk payloads is a different operation
  and is not written.
- **The interface beyond the settings.** There is a window with status and a
  storage slider; there is no way to pair a device, browse what is synced, or
  recover a deleted file from it. The onboarding screen that asks someone to
  write down 24 words is still a terminal, and is the highest-stakes part of
  the product.
