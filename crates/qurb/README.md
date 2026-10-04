# qurb

The program a person runs. Everything else in this repository is a library.

```
qurb init [dir]                  set up a device and create a key (~/qurb if none)
qurb enrol <dir> "<24 words>"    set up a device with an existing key
qurb pair [dir]                  show a code and wait for a device to join
                                 (the code lasts five minutes, then it stops)
qurb join [dir] <code>           join a device that is showing a code
qurb run [dir]                   watch, sync, and keep running
qurb replica [dir] [--only p]    hold content for devices that are asleep
qurb status [dir]                what this device holds and trusts
qurb verify [dir] [--deep]       check the store against itself
qurb reclaim [dir]               free space the folder itself already holds
qurb fetch [dir] <path>          ask for a dropped file's contents back
qurb send [dir] <file|folder>... to <dev>
                                 send files and folders to one device, privately
qurb cancel [dir] <name> to <dev>
                                 take back a send not yet collected
qurb free [dir] <path>           free a file's local copy another device keeps
qurb holders [dir] [add|remove <dev>]
                                 the devices that keep this one's own files
qurb conflicts [dir] [keep <copy> this|other|both]
                                 files two devices changed at once; settle one
qurb share [dir] [<folder> with <dev>,... | <folder> with everyone]
                                 which devices a folder is shared with
qurb keep [dir] <folder> here|remote
                                 keep a folder here, or only list it here
qurb deleted [dir]               recently deleted, restorable for 30 days
qurb restore [dir] <#n or path>  put one back, on every device
qurb remove-device [dir] <dev> [--delete-kept] [--yes]
                                 stop trusting a device; says what that does
qurb activity [dir] [path]       what happened, newest first
qurb ls [dir] [path]             what this folder holds, and where
qurb find [dir] <text>           files whose name contains something
qurb config [dir] [key=value]    show or change settings
qurb protect [dir] <how>         keep the key in a file, the keystore, or
                                 behind a passphrase
qurb version                     the build, its protocol, its index schema

qurb signal [addr] [--push <j>]  the rendezvous service
qurb relay [addr]                the relay
qurb netcheck                    what kind of router this machine is behind
```

`[dir]` is the qurb folder. Left out, it is the one set up most recently —
and a word that is not a folder with qurb in it is never taken for one, so
`qurb protect keystore` and `qurb protect ~/qurb keystore` both mean what
they say. Folders are listed by their full path, in
`~/.config/qurb/folders`.

## Two devices, start to finish

On the first:

```bash
qurb init ~/Sync          # writes down 24 words — this is the only copy
qurb pair ~/Sync          # shows a code
```

On the second, using the phrase from the first:

```bash
qurb enrol ~/Sync "wheel push industry ..."
qurb join ~/Sync qurb1-...
```

Then `qurb run ~/Sync` on both.

Sharing a key is what makes two devices *yours*. Pairing is separate and still
necessary: it is how they learn each other's network identity, and it happens
out of band because someone able to change what is on your screen has already
won.

## Setting a device up

`qurb init` and `qurb enrol` both go through `qurb_cli::setup`, which is the
one definition of what a set-up device is — a key, an identity, a config and an
index. The desktop window calls the same functions, so a device created there
and one created here are the same thing rather than two similar things.

## What an interface asks

`qurb_cli::View` is the read-only query surface a front end uses: devices,
files with their availability, storage, history, what is still on its way, and
search. `qurb ls` and `qurb find` are the terminal's use of it.

The three-way availability is the part worth knowing about. A file that is here
and also on the phone, and a file that is here and nowhere else, look identical
to anything that only checks whether the bytes are on disk — and offering to
free the second would be offering to delete it.

## Why is my file not here?

```bash
qurb activity ~/Sync holiday/beach.jpg
```

Every device writes down what it did — stored, deleted, received, sent,
collected, evicted, restored, conflicted, paired, failed — so the question is
still answerable after a restart, which is when people ask it. Without a path
it lists everything, newest first.

## Sending a file to one device

```bash
qurb send ~/Downloads/tickets.pdf ~/Pictures/Trip to phone
qurb send ~/Sync ~/Downloads/tickets.pdf to phone     # naming the folder
```

Any number of files and folders, then `to` and the device. A folder is sent
whole under its own name — `Trip/day1/beach.jpg` — and arrives as a folder.
Links inside a folder are not followed, and a qurb store inside one is never
sent, since it holds this device's keys. Two things with the same name are both
sent, the second as `notes (2).txt`. Anything that cannot be sent is named,
with why, and the rest still goes.

The folder to send from is the one given first if it has a store in it, and
otherwise the one most recently used.

```bash
qurb cancel tickets.pdf to phone
```

Takes back a send the device has not collected yet, by the name `qurb send`
printed. It never arrives, however long the device was switched off. Once
collected it cannot be taken back — the file is theirs then — and a device in
the middle of collecting when you cancel may still finish. The space it held
comes back after the usual seven-day retention, not at once.

The file goes to that device and to no other, and nothing about it is
advertised to the rest of the fleet. Where it lands depends on what the
recipient is: a desktop running this daemon saves it as an ordinary file in
`Downloads/qurb` (see `downloads` below), and a phone keeps it in its own
folder, privately. Name the recipient the way `qurb status` lists it, or by its short id if
two devices share a name.

The bytes stay on this device until the recipient confirms they arrived, so
sending to a phone that is switched off works — it collects the file the next
time it syncs. After that the copy here is the first thing dropped when the
storage limit bites, before any of this device's own files.

## How news travels

A device holds one request open against each peer — "tell me when your state
differs from this" — and the answer arrives when it does. An edit reaches
another device in about 400ms, most of which is the watcher deliberately waiting
to see whether the file is still being written.

It does not break the rule that a peer can ask and never tell: the device that
wants to know is the one asking, and the answer simply arrives later than usual.

A counter rather than a flag, because a flag can be missed — a peer told
"something changed" cannot tell a notification it has already acted on from a
new one. It says what it last saw instead, and gets an immediate answer if
anything has happened since. The counter need not survive a restart: a peer
holding a number from before sees one that does not match, which is exactly the
right conclusion.

A sweep every two minutes covers what being told cannot — a notification lost
with a dropped connection, a peer that was unreachable when it changed, a
machine coming back from sleep.

## The services

`qurb signal` introduces devices and tells both to punch at the same moment. It
learns no filenames and cannot tell whose devices these are. `qurb relay`
carries traffic for devices that cannot reach each other directly, and can
neither read it nor forge it.

Both belong behind TLS before they face the internet. Neither refuses to start
without it, which is a gap rather than a decision.

## Where the key is kept

```bash
qurb protect ~/Sync              # what it is now, and the options
qurb protect ~/Sync keystore     # into the operating system's keystore
qurb protect ~/Sync passphrase   # wrapped with something only you know
```

The key does not change, so nothing it protects becomes unreadable — this
changes the lock, not the contents. A passphrase means `qurb run` asks at
startup, so the device can no longer start unattended, which is the trade.

Your recovery phrase is unaffected either way: it recovers the key, while the
passphrase guards the copy on this disk.

## Settings

`<dir>/.qurb/config`, a flat `key = value` file meant to be edited by hand:

```
signal = wss://signal.example.com
relay  = 198.51.100.7:443
name   = Study desktop
port   = 0
limit  = 10G
downloads =
own-files = shared
notifications = on
```

`qurb config <dir> key=value` changes one and checks it first; the window's
Settings write the same file, and a running daemon picks up `limit`,
`downloads` and `own-files` without restarting.

An unknown key is an error rather than being ignored, because a misspelled
setting that silently does nothing is a bad afternoon.

### `limit`

How much disk this folder may use — files plus chunk store. `0`, the default,
means no limit. Accepts `500M`, `10G`, `1T`, or a plain byte count.

Over the limit, qurb drops local copies of the files it has gone longest
without touching. The path stays: it still syncs, still appears in
`qurb status`, and `qurb fetch <dir> <path>` brings the contents back.

Two refusals are built in and will not be talked out of:

- A file is dropped only when **another device is known to hold those exact
  bytes**. A device never drops content it made itself.
- A device that cannot free enough **stays over its limit** and says so. A
  limit is a promise about disk, not a reason to delete the only copy of
  something.

On Linux a dropped file is simply absent from the folder — there is no
placeholder API to keep its name visible, so `qurb status` is where you find
out it still exists.

### `own-files`

`shared`, the default on a desktop: a file added here goes to every device.
`private`: it goes into this device's own vault instead, and only the devices
named with `qurb holders add <device>` see it — each keeps a copy it never
shows. That is what lets `qurb free <path>` drop the local copy and `qurb fetch
<path>` bring it back. A holder lets go of a file only when this device deletes
it. What a phone does; see
[decision 0036](../../docs/decisions/0036-a-phone-keeps-its-own-files.md).

### `downloads`

Where a file somebody sends this device is saved. Empty, the default, means
`qurb` inside your Downloads folder (wherever `XDG_DOWNLOAD_DIR` says it is).
A path means there. `off` keeps received files inside the synced folder,
privately, which is what qurb did before.

A file saved there is an ordinary file. qurb does not scan it, does not count
it against `limit`, and does not treat deleting it as anything: it is yours to
open, move or delete. What qurb keeps is a record that it took the delivery, so
the sender offering it again changes nothing. A name already taken in that
directory is never overwritten; the arriving file is saved beside it as
`name.from-<device>-<when>.ext`.

The directory may not overlap the synced folder, in either direction —
received files inside the folder would be synced to every device. `qurb
config` refuses such a setting and `qurb run` refuses to start with one. If
the synced folder is itself `Downloads/qurb`, where new folders used to go, the
default becomes `Downloads/qurb-received` instead.

See [decision 0037](../../docs/decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md).

### `notifications`

`on`, the default, or `off`: whether the desktop window raises its three
notifications — a file sent here, one you sent collected, one that failed. The
command line and the tray raise none either way.

## What is waiting to be delivered

`qurb status` ends with a line like:

```
  only here  2 file(s), 2.9 MiB — no other device has these yet
```

Files this device made that no other device is known to hold. While that line
is there, losing this device loses that work.

It is a question asked of the index each time — live files made here whose
content nothing else has taken — rather than a queue of pending transfers.
There is nothing to queue: a file added while every other device is switched
off is simply in the folder, and it moves when one is next reachable. A device
learns it is no longer the only holder because the device that received the
content says so, which is the only message in the protocol that asks for
nothing.

## What it does not do yet


- **Choose what to keep, automatically.** `qurb keep <folder> remote` says a
  folder is only listed here; otherwise the storage limit picks by what is
  coldest. Nothing chooses by folder on its own.
- **Run `qurb run` as a service.** A replica has a unit
  (`packaging/server/qurb-replica.service`); the daemon a person uses on a
  desktop runs in the window, which starts at login. No launch agent or Windows
  service.
- **Anything graphical on its own.** The window is
  [`qurb-desktop`](../desktop/README.md), which runs this same daemon inside
  itself.
