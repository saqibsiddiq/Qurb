# qurb

The program a person runs. Everything else in this repository is a library.

```
qurb init <dir>                  set up a device and create a key
qurb enrol <dir> "<24 words>"    set up a device with an existing key
qurb pair <dir>                  show a code and wait for a device to join
                                 (the code lasts five minutes, then it stops)
qurb join <dir> <code>           join a device that is showing a code
qurb run <dir>                   watch, sync, and keep running
qurb status <dir>                what this device holds and trusts
qurb verify <dir> [--deep]       check the store against itself
qurb config <dir> [key=value]    show or change settings

qurb signal [addr]               the rendezvous service
qurb relay [addr]                the relay
```

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
```

An unknown key is an error rather than being ignored, because a misspelled
setting that silently does nothing is a bad afternoon.

## What it does not do yet

- **Notice new pairings while running.** The guest list is read at startup, so a
  device paired afterwards needs a restart.
- **Run as a service.** No unit file, no launch agent, no Windows service.
- **Anything graphical.** This is the daemon the interface will sit on.
