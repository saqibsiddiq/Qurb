# Syncing from anywhere

qurb's promise is that a phone with an internet connection can reach a laptop
at home, from any network. This is how to actually get there, and what stands
in the way.

**Status: the code is ready; the deployment is not.** Everything below works,
but it needs a rendezvous service somewhere both devices can reach, and by
default there is no such thing — it runs wherever someone runs it, which to
begin with is a laptop on a home network.

## What has to be reachable, and what does not

Only the **rendezvous service**. It is a small WebSocket service whose entire
job is letting two devices learn each other's addresses. It never sees a file,
a filename, or a key.

The files go **directly between the devices** over QUIC, encrypted end to end.
That is not a detail of the current implementation, it is the point of the
design: see [decisions/0006](decisions/0006-availability-gap.md).

So "syncing from anywhere" needs one small service with a public name, and
nothing else. Whether a direct connection can then be made depends on the two
networks — run `qurb netcheck` on each to find out. A laptop behind an
endpoint-independent NAT, which is the common case, can be reached from almost
anywhere.

A **relay** is the fallback for when it cannot. It carries encrypted packets it
cannot read, and it is only used when a direct path fails. It has the same
deployment question as the rendezvous service, and it is worth solving second
rather than first, because most networks do not need it.

## The free way: Tailscale

The least work, no money, and no holes in anything.

[Tailscale](https://tailscale.com) is a mesh VPN on WireGuard. Its personal
plan is free. Devices on a tailnet reach each other from any network, behind
any NAT, with no port forwarding — and it will issue a real certificate for a
machine's name, which is what lets qurb use `wss://` without buying a domain.

On the laptop:

```bash
sudo pacman -S tailscale          # or your distribution's package
sudo systemctl enable --now tailscaled
sudo tailscale up
```

Enable HTTPS for the tailnet once, in the Tailscale admin console under DNS,
then publish the rendezvous service on the machine's name:

```bash
qurb signal 127.0.0.1:9000 &
sudo tailscale serve --bg --https 443 http://127.0.0.1:9000
tailscale serve status              # prints the https:// URL
```

`tailscale serve` terminates TLS with a certificate Tailscale obtains for you,
and proxies WebSockets. The rendezvous service itself stays bound to loopback
and is never exposed.

On the phone: install Tailscale from the Play Store, sign in to the same
account, then in qurb set the rendezvous address to the URL `serve status`
printed, with `wss://` in place of `https://`:

```
wss://<machine>.<tailnet>.ts.net
```

That is the whole change. qurb is then reachable from mobile data, a café, or
another country.

### What it costs

A dependency on somebody else's coordination service, which is exactly the kind
of thing qurb exists to avoid. Two things make it a reasonable trade rather
than a contradiction:

- **Files never touch it.** Tailscale carries the rendezvous WebSocket and,
  where it is used as the path, WireGuard-encrypted packets it cannot read. The
  contents are encrypted by qurb before either.
- **It is a stand-in, not an architecture.** The rendezvous service is qurb's
  own; Tailscale is only making it reachable. Moving it to a host of your own
  later changes one URL.

It is honest to say that a product would not ship this way. A person who
installs qurb should not have to install a VPN first. This is the free way to
have the feature working today, and the hosted rendezvous service is the
product answer.

## Being woken, rather than looking

A device that is connected can be told there is something for it, and acts
within a second. A phone that is asleep cannot be told anything — Android stops
a background app's socket within minutes of the screen going off — so it finds
out at its next scheduled look, about fifteen minutes away and longer while
dozing.

A push notification is the way through, and on both mobile platforms it is the
only way through. With one configured, the rendezvous service pokes the phone
the moment another device has something, and the phone syncs immediately.

Turning it on:

```bash
# On the host, with the service account JSON from your Firebase project
qurb signal 127.0.0.1:9000 --push /etc/qurb/firebase.json
```

That needs a build with the feature compiled in:

```bash
cargo build --release --features push
```

And on the phone, a `google-services.json` from the same Firebase project
dropped into `android/app/` before building. Its presence is what switches the
whole thing on: without it the Firebase SDK is not even linked, and the app
behaves exactly as it does today.

The poke carries nothing — no filenames, no sizes, not even which peer. What
Google learns is that a device was poked and when.
[decisions/0028](decisions/0028-waking-a-sleeping-device.md) sets out that
trade in full, including why there is no alternative and what it does *not*
give away.

## The product way: a host of your own

A small server with a public address, running `qurb signal` and `qurb relay`
behind a TLS-terminating reverse proxy. Roughly $5 a month, plus a domain.

The systemd units, the Caddy configuration and the step-by-step are in
[packaging/server/](../packaging/server/README.md). Nothing in qurb needs to
change for it: point the devices at `wss://rendezvous.example.com` and it
behaves identically.

This is also the only arrangement where the relay fallback works, because
tunnels and most free proxies carry TCP only. And it is where push belongs: a
service that can wake a sleeping phone is what makes the whole thing feel
immediate rather than eventual.

## What will not work

**A private address from outside the house.** `ws://192.168.1.4:9000` is the
laptop's address on its own network. From a mobile carrier it is not an address
at all, and this is the single most likely reason syncing "just stops working"
away from home.

**Plaintext to a public address.** qurb refuses it. The identifiers devices
announce under are bearer secrets, and sending them unencrypted across a
carrier's network hands them to everyone on the path. See
[decisions/0027](decisions/0027-plaintext-stops-at-the-local-network.md).

**Reachability alone does not make the laptop awake.** qurb syncs between
devices, so two devices that are never on at the same moment never meet,
however well each can be reached. Paying for a rendezvous service does not
change that: it never holds content.

The answer is on the next page — the same host can run a replica.

## Holding content for devices that are asleep

A **storage-only replica** is a device that holds chunks and nothing else. It
has no folder, shows nobody any files, and cannot read what it stores. Its
whole job is to be awake when the others are not.

```bash
qurb enrol /srv/qurb "<the same 24 words>"
qurb replica /srv/qurb
```

With one in the picture, a phone can send a photo at midnight to a machine that
is always on, and a laptop can collect it on Tuesday. The two never have to be
awake together, which is the limitation nothing else removes.

`--only` restricts what it holds:

```bash
qurb replica /srv/qurb --only work
```

Measured on the development laptop, 2026-09-22, three stores and a rendezvous
service on loopback, with the phone and laptop daemons **never running at the
same time**: a 400 KB file written on the phone reached the replica while the
laptop was stopped, and the laptop collected it byte-for-byte from the replica
after the phone had stopped.

### What a replica is not

**It is not a backup.** It holds what its peers have. A file deleted on a
device is deleted on the replica too, once the deletion reaches it — that is
what syncing means.

**It does not make a small box hold a large library.** A replica keeps every
payload, because with no folder there is nowhere else for the bytes to live.
The storage cap can bound it, but a replica over its cap currently has nothing
it is able to drop: eviction works by deleting a file from a folder, and a
replica has no folder. So a cap on a replica reports the overrun rather than
acting on it. Choose a host with room for what you ask it to hold, or use
`--only`.

**It is not the same as the rendezvous service**, though the same host can run
both. The rendezvous service introduces devices and never sees content; a
replica holds content and needs no public name beyond being reachable.
