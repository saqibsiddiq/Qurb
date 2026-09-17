# qurb

The program a person runs. Everything else in this repository is a library.

```
qurb init <dir>                  set up a device and create a key
qurb enrol <dir> "<24 words>"    set up a device with an existing key
qurb pair <dir>                  show a code and wait for a device to join
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

## The services

`qurb signal` introduces devices and tells both to punch at the same moment. It
learns no filenames and cannot tell whose devices these are. `qurb relay`
carries traffic for devices that cannot reach each other directly, and can
neither read it nor forge it.

Both belong behind TLS before they face the internet. Neither refuses to start
without it, which is a gap rather than a decision.

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

- **Push.** A device syncs when *it* changes something, or every ten seconds. An
  edit made elsewhere arrives when this device next asks, so up to ten seconds
  late. The right fix is a peer asking to be told — which keeps the rule that a
  peer can ask and never tell — and it is not built.
- **Notice new pairings while running.** The guest list is read at startup, so a
  device paired afterwards needs a restart.
- **Run as a service.** No unit file, no launch agent, no Windows service.
- **Anything graphical.** This is the daemon the interface will sit on.
