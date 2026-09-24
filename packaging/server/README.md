# Running the services on a host of your own

Two small services, neither of which holds anybody's files.

| | what it does | what it sees |
|---|---|---|
| **rendezvous** | lets two devices learn each other's addresses | blinded identifiers, addresses |
| **relay** | carries packets when no direct path exists | ciphertext it has no key for |

Files go **directly between devices**, encrypted end to end. That is the point
of the design rather than a property of this deployment — see
[decision 0006](../../docs/decisions/0006-availability-gap.md).

A machine with 1 GB of memory is ample. The rendezvous service holds a socket
per connected device; the relay's cost is bandwidth, and bandwidth is the bill.

## Setting it up

```bash
# A user that owns nothing
sudo useradd --system --no-create-home --shell /usr/sbin/nologin qurb

# The binary, built on a machine with a Rust toolchain
cargo build --release                      # or --features push, see below
sudo install -m755 target/release/qurb /usr/local/bin/qurb

sudo install -m644 packaging/server/qurb-signal.service /etc/systemd/system/
sudo install -m644 packaging/server/qurb-relay.service  /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now qurb-signal qurb-relay
```

### TLS, with a domain name

If a name resolves to this machine, a real certificate is the least surprising
thing to have:

```bash
sudo install -m644 packaging/server/Caddyfile /etc/caddy/Caddyfile
sudo $EDITOR /etc/caddy/Caddyfile          # put your own name in it
sudo systemctl restart caddy
```

```bash
qurb config ~/Downloads/qurb signal=wss://rendezvous.example.com
qurb config ~/Downloads/qurb relay=rendezvous.example.com:9001
```

### TLS, with no domain name

A host with an address and nothing else — which is what a small VPS is until
you buy a name — can present its own certificate, and devices check its
fingerprint instead of asking an authority. It is the same way a peer's
identity is checked, for the same reason: see
[decision 0035](../../docs/decisions/0035-a-rendezvous-on-a-bare-address.md).

Use this **instead of** the Caddy setup above, not alongside it.

```bash
sudo systemctl disable --now qurb-signal
sudo $EDITOR packaging/server/qurb-signal-tls.service   # put this host's address in it
sudo install -m644 packaging/server/qurb-signal-tls.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now qurb-signal-tls
```

It prints the whole setting at startup, fingerprint included:

```
sudo systemctl status qurb-signal-tls
```

```
Devices reach it as:

  wss://203.0.113.5:9000#40478155b092acf37fd1af1e526936dddd19b2152580b15fdcc0230cd6762e41
```

Copy that line, in full, onto each device:

```bash
qurb config ~/Downloads/qurb 'signal=wss://203.0.113.5:9000#4047…2e41'
qurb config ~/Downloads/qurb relay=203.0.113.5:9001
```

Quote it in a shell: `#` starts a comment otherwise, and a setting silently
truncated to `wss://203.0.113.5:9000` fails later with a certificate error
rather than at the moment it was mistyped.

The certificate is kept in `/var/lib/qurb-rendezvous` and reused across
restarts. That is deliberate: the fingerprint is what every device has been
told to expect, so a service that made a new one each time it started would
lock out every device it had. If you do replace it, every device needs the new
line.

On the phone, for either arrangement: **⋮ → Rendezvous service**.

## Ports

| port | protocol | who reaches it |
|---|---|---|
| 443 | TCP | everyone — Caddy, if you are using a domain and a real certificate |
| 9000 | TCP | loopback only behind Caddy; **everyone** when the service presents its own certificate |
| 9001 | UDP | everyone — the relay |

The relay is the one thing that faces the internet directly, and it has to:
it carries QUIC between two devices, and there is nothing for a reverse proxy
to terminate. It needs no TLS of its own because what it carries is already an
encrypted session it holds no key for.

## Waking sleeping phones

Optional, and it changes how fast a phone notices rather than whether it does.
See [decision 0028](../../docs/decisions/0028-waking-a-sleeping-device.md) for
what it costs — briefly, Google learns that a device was poked and when.

```bash
cargo build --release --features push
sudo install -m755 target/release/qurb /usr/local/bin/qurb

sudo mkdir -p /etc/qurb
sudo install -o qurb -g qurb -m400 firebase.json /etc/qurb/firebase.json

sudo systemctl disable --now qurb-signal
sudo install -m644 packaging/server/qurb-signal-push.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now qurb-signal-push
```

The phone half needs a `google-services.json` from the same Firebase project in
`android/app/` before building the app. Its presence is what switches push on:
without it the SDK is not linked at all.

Two things worth knowing, both learned the hard way:

**A plain `cargo build --release` overwrites the push binary.** The feature is
not on by default, so rebuilding the workspace without `--features push`
replaces `target/release/qurb` with one that refuses `--push` at startup. It
says so clearly rather than starting without push, which is the right
behaviour — but it is a confusing minute if you have forgotten.

**Restarting the service forgets every wake token.** They are held in memory,
so the first change after a restart wakes nobody. Devices re-register on their
next connection, so it heals itself at the cost of one delayed sync. Persisting
them would mean a database, which is the thing this service is valuable for not
having.

## What this does not give you

**Availability when every device is off** — unless you also run a replica, see
below. The rendezvous service and the relay hold no content, so two devices
that are never awake together never meet.

**A backup.** Nothing here keeps a copy of anything, and a replica is not one
either: it holds what your devices hold, including their deletions.

## A replica, if you want one

This is the piece that makes an always-on host worth paying for: a device that
holds content so the others need not be awake together. Your phone can send a
photo at midnight and your laptop can collect it on Tuesday.

**It is the one service here that holds your key.** The rendezvous and the
relay see routing metadata and ciphertext they have no key for — they could be
run by a stranger. A replica is enrolled with your recovery phrase, which means
anybody with root on this host can read your files. Run it only on a host you
control, and decide that trade deliberately rather than by following
instructions.

```bash
# Enrolled by hand, because a recovery phrase in a systemd file would be in the
# journal and in every backup of /etc.
sudo -u qurb qurb enrol /var/lib/qurb-replica "<your 24 words>"

sudo install -m644 packaging/server/qurb-replica.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now qurb-replica
```

Then pair it with one of your devices, as you would any other:

```bash
sudo -u qurb qurb pair /var/lib/qurb-replica     # shows a code
qurb join ~/Downloads/qurb <that code>            # on your laptop
```

A replica has no folder and shows nobody any files. It stores chunks it cannot
read and serves them to devices that can. What it cannot yet do is free space —
see [the product plan](../../docs/product-plan.md) for that gap.


