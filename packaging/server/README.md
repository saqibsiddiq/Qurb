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

TLS for the rendezvous service, with a name that resolves to this machine:

```bash
sudo install -m644 packaging/server/Caddyfile /etc/caddy/Caddyfile
sudo $EDITOR /etc/caddy/Caddyfile          # put your own name in it
sudo systemctl restart caddy
```

Then point the devices at it:

```bash
qurb config ~/Downloads/qurb signal=wss://rendezvous.example.com
qurb config ~/Downloads/qurb relay=rendezvous.example.com:9001
```

On the phone: **⋮ → Rendezvous service**.

## Ports

| port | protocol | who reaches it |
|---|---|---|
| 443 | TCP | everyone — Caddy, which proxies to the rendezvous service |
| 9000 | TCP | loopback only |
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

## What this does not give you

**Availability when every device is off.** These services hold no content, so
two devices that are never awake together still never meet. The answer to that
is a storage-only replica — `qurb replica` — which holds content and therefore
costs storage. Running one *per user* is what makes a product expensive, which
is why it is a thing a person runs on their own hardware rather than something
offered here. See [anywhere.md](../../docs/anywhere.md).

**A backup.** Nothing here keeps a copy of anything.
