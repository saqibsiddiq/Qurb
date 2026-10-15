# Running the services on a host of your own

Two small services, neither of which holds anybody's files.

| | what it does | what it sees |
|---|---|---|
| **rendezvous** | lets two devices learn each other's addresses | blinded identifiers, addresses |
| **relay** | carries packets when no direct path exists | ciphertext it has no key for |

Files go **directly between devices**, encrypted end to end. That is the point
of the design rather than a property of this deployment — see
[decision 0006](../../docs/decisions/0006-availability-gap.md).

A machine with 1 GB of memory is ample, and that is now measured rather than
assumed ([experiments/service-capacity](../../experiments/service-capacity/README.md),
on a laptop over loopback): the rendezvous service holds about 22 KiB per
connected device, 30 KiB presenting its own certificate, and answers an
introduction in about 0.1 ms with 9,000 devices connected; the relay holds
about 19 KiB per device that is on, and carried 241 MiB/s for one transfer at
about 5 CPU-seconds per GiB. For one person's devices none of that is close to
a limit. The relay's real cost is bandwidth — every byte through it crosses
the server twice — and bandwidth is the bill.

## No server at all

For one person's phone and computer, the rendezvous service can run on the
computer. The phone can only sync with the computer while the computer is on,
so a service that runs only then costs nothing in availability. It needs a
public address that terminates TLS in front of it; two free ones, with no card:

- **Tailscale Funnel**, if the computer runs Tailscale: one address for good,
  `https://<name>.<tailnet>.ts.net`. The phone does not need Tailscale.
- **A Cloudflare quick tunnel** (`cloudflared tunnel --url http://localhost:9000`):
  no account, but a new address every time it starts, which the phone's setting
  must follow.

```bash
packaging/install-rendezvous.sh      # qurb signal 127.0.0.1:9000, at login, with push if it can
tailscale funnel --bg 9000           # the first time, approve Funnel for the tailnet
qurb config signal=ws://localhost:9000   # the folder already set up
```

and on the phone, Settings → Rendezvous service: `wss://<name>.<tailnet>.ts.net`.

`install-rendezvous.sh` builds qurb with push and installs it as
`~/.local/bin/qurb-rendezvous`, a name no ordinary build or install of qurb
overwrites, and installs `packaging/qurb-rendezvous.service` to run it. Push is
on when `~/.config/qurb/firebase.json` exists (the service account under
[Waking sleeping phones](#waking-sleeping-phones)); the script writes
`~/.config/qurb/rendezvous.env` accordingly, and says which. With push, a
change on the computer wakes the phone to sync within seconds instead of at its
next periodic pass. With push on, the service keeps wake tokens in
`~/.config/qurb/rendezvous/wake-tokens.json` (readable by you only), so a
restart does not stop it waking the phone. `--uninstall` removes it all.

**What this lacks is a relay.** Neither option carries the relay's raw TCP, so
the phone and the computer have to reach each other directly. Whether they can
depends on both networks; `qurb netcheck` says whether the computer's allows
it. A home network that gives the same public address to whoever asks, as the
project owner's does, usually does — and on 2026-09-28 a Galaxy S23 on mobile
data synced with a laptop on such a network this way, directly, through Funnel,
with no relay. If a mobile network defeats it, the relay needs a server after
all.

## The quick way

From your computer, once there is a server you can reach with SSH and sudo:

```bash
cargo build --release -p qurb-cli -p service-capacity
packaging/server/deploy.sh ubuntu@203.0.113.5
```

It sends your computer's `qurb` if the server can run it — x86-64, with a C
library at least as new as the one it was built against, which Ubuntu 24.04 is
for a build from 2026-09 — and otherwise the source, to build there. On the
server, [`setup.sh`](setup.sh) installs the rendezvous service with its own
certificate and the relay, opens TCP 9000 and 9001 in the server's own
firewall, and prints the lines for your computer and phone. Then it checks
from your computer, across the internet, that the service introduces two
devices and that a transfer gets through the relay.

**Written 2026-09-28 and not yet run against a real server.** The parts it is
made of are tested; the script as a whole is not, until the first server.

### A free server to try it on

Oracle Cloud's *Always Free* tier is the one that fits, as of September 2026:
two small AMD machines (`VM.Standard.E2.1.Micro`, 1 GB) at no cost for as long
as the account exists, and 10 TB of outbound data a month — ample for the relay
between one person's devices. Choose Ubuntu 24.04, and a home region near you
at sign-up, since it cannot be changed afterwards. A card is asked for, to
verify identity.

Three things to know:

- **The cloud's own firewall.** Besides the server's firewall, which the
  script opens, Oracle filters at the network: in the console, the subnet's
  security list needs ingress rules for TCP 9000 and 9001 from 0.0.0.0/0. The
  script says so at the end, and the check from your computer fails until it
  is done.
- **Idle reclamation.** Oracle may reclaim an Always Free machine whose CPU,
  network and memory all stay under 20% for seven days — and these services are
  idle nearly all the time, by design (see the measurements above). Converting
  the account to pay-as-you-go, which still costs nothing within the free
  limits, is widely reported to prevent it; check Oracle's free-tier FAQ.
- **Capacity.** The free machines are sometimes unavailable in a region for a
  while. Oracle's free Arm machines, if chosen instead, were halved in June 2026
  to 2 cores and 12 GB — still plenty — and need a build on the server, which
  the script does.

### Or Google Cloud's

Google Cloud's free `e2-micro` (2 shared cores, 1 GB) runs only in three US
regions — Oregon `us-west1`, Iowa `us-central1`, South Carolina `us-east1` —
with 1 GB of outbound data a month. Enough to prove the arrangement works and
for the rendezvous service indefinitely; tight for the relay, where every byte
relayed to a device is a byte out. Its external address is not charged on the
free tier. When creating the machine, three settings decide whether it stays
free, and two are not the defaults:

- **Machine:** `e2-micro`, in one of the three regions above.
- **Boot disk:** Ubuntu 24.04 LTS, x86/64, on a **standard persistent disk**, up
  to 30 GB. The default, a *balanced* disk, is not free.
- **Networking:** the network service tier **Standard**; and a network tag,
  `qurb`, for the firewall rule.

Then, in *VPC network → Firewall*, one rule: ingress, targets with the tag
`qurb`, source `0.0.0.0/0`, TCP `9000,9001`. And under *Security → Manage
access* on the machine, your SSH public key — the text after its last space is
the user name you will log in as. A budget alert of a dollar or so, under
*Billing → Budgets & alerts*, turns a mistake into an email rather than a bill.

## Setting it up by hand

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
qurb config signal=wss://rendezvous.example.com
qurb config relay=rendezvous.example.com:9001
```

The relay can be given by name, as here, or by address. A name is looked up
each time a device starts syncing — on a phone, every pass — and the first of
its addresses that answers is used, so a name with an IPv6 address the relay
does not listen on still works. Names were refused until 2026-09-27.

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
qurb config 'signal=wss://203.0.113.5:9000#4047…2e41'
qurb config relay=203.0.113.5:9001
```

Quote it in a shell: `#` starts a comment otherwise, and a setting silently
truncated to `wss://203.0.113.5:9000` fails later with a certificate error
rather than at the moment it was mistyped.

The certificate is kept in `/var/lib/qurb-rendezvous` and reused across
restarts. That is deliberate: the fingerprint is what every device has been
told to expect, so a service that made a new one each time it started would
lock out every device it had. If you do replace it, every device needs the new
line.

On the phone, for either arrangement: **Settings → Rendezvous service**, and
**Settings → Relay** with the same `host:9001` the computers use. The relay is
what lets a phone on mobile data reach a computer at home: carriers commonly
put phones behind address translation a direct connection cannot get through.

## Ports

| port | protocol | who reaches it |
|---|---|---|
| 443 | TCP | everyone — Caddy, if you are using a domain and a real certificate |
| 9000 | TCP | loopback only behind Caddy; **everyone** when the service presents its own certificate |
| 9001 | TCP | everyone — the relay |

**The relay is TCP.** This table said UDP until 2026-09-27, and a firewall opened
as it said would have blocked the relay entirely. It carries QUIC between two
devices, but over TCP on purpose: it exists for networks where UDP does not get
through, and a fallback that needs the thing being fallen back from is no
fallback (see [its README](../../crates/relay/README.md)). With `ufw`, for the
setup without a domain:

```bash
sudo ufw allow 9000/tcp     # the rendezvous service, presenting its own certificate
sudo ufw allow 9001/tcp     # the relay
```

The relay is the one thing that faces the internet directly, and it has to:
there is nothing for a reverse proxy to terminate. It needs no TLS of its own
because what it carries is already an encrypted session it holds no key for.
On a network that lets nothing out but 443, even a TCP relay on 9001 is out of
reach; moving it to 443 means giving it a port Caddy is not using.

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

**Wake tokens are kept across a restart**, in `wake-tokens.json` in the
service's state directory (`$STATE_DIRECTORY`, or
`~/.config/qurb/rendezvous`), readable by the service's user only. They are
written when one changes — rarely: a device re-sends the same token on every
connection — and read back at start.

This reverses what this guide used to say: that a restart forgets them, heals
itself "at the cost of one delayed sync", and that keeping them would mean a
database. Running the service on a laptop, which restarts it whenever the
laptop reboots, showed the cost is larger: a phone re-sends its token only on
its own next scheduled pass, which Android may put off for hours, and no change
wakes it until then. A small file written when a token changes is not a
database.

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
# journal and in every backup of /etc. The words are in the window, under
# Settings → the recovery phrase; nothing asks you to write them down.
sudo -u qurb qurb enrol /var/lib/qurb-replica "<your 24 words>"

sudo install -m644 packaging/server/qurb-replica.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now qurb-replica
```

Then pair it with one of your devices, as you would any other:

```bash
sudo -u qurb qurb pair /var/lib/qurb-replica     # shows a code
qurb join <that code>                           # on your laptop
```

A replica has no folder and shows nobody any files. It stores chunks it cannot
read and serves them to devices that can. What it cannot yet do is free space —
see [the product plan](../../docs/product-plan.md) for that gap.


