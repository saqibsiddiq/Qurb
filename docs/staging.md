# Staging

Written 2026-11-01 with the owner, who has not deployed anything before and
asked for the cheapest arrangement that runs QurbSpace end to end: the
desktop app, the phone app, and a dashboard of the services' traffic and
logs. Each step says who does it, and is marked here when it is done.

## Now: on the owner's laptop

Chosen 2026-11-01, with no money or card for a server yet. Costs nothing:

- **Rendezvous and relay**: the laptop's `qurb-rendezvous` service
  (`~/.config/systemd/user/qurb-rendezvous.service`, `qurb signal
  127.0.0.1:9000` with push), published by **Tailscale Funnel** at
  `wss://saqib.tail86283c.ts.net`. The relay is the one `qurb signal` carries
  at `/relay` beside it, which every device uses when nothing else is set
  (`relay_for`) — over HTTPS, so it also passes networks that let only web
  traffic out. Checked from the public internet on 2026-11-01: a WebSocket
  at `/` and at `/relay` answered 101 through Funnel's public address.
- **Apps**: releases name this address as their default
  (`QURB_DEFAULT_SIGNAL` in `.github/workflows/release.yml`), so a tester's
  phone or computer finds the others with nothing typed; a device whose
  settings name another keeps it.
- **Dashboard**: Grafana Cloud's free tier needs an account (no card); its
  agent then runs on the laptop.

**What this lacks**, stated: staging is up only while the laptop is awake
and online — it suspends when idle, by the owner's choice — and the
laptop's upload is every tester's relay. Moving to a server later is steps
1–4 below, and a new `QURB_DEFAULT_SIGNAL`.

**Found while checking it**: the rendezvous test tool's introductions time
out intermittently — 2 to 6 of 8 back-to-back runs at worst, locally and
through Funnel alike, 8 of 8 at other times. Not Funnel; not yet explained.
Being looked into separately.

## Later: a server of its own

## What staging is

Everything a user would touch, at addresses of its own, so a change can be
tried on real networks before anyone else sees it:

```
  phone (APK)  ──┐                         ┌── laptop (desktop app)
                 │  wss://signal.staging.qurb.cloud   (finding each other)
                 ├────────────► one small server ◄────┤
                 │  relay.staging.qurb.cloud:9001     (only when no direct path)
                 │                  │
                 │                  └── agent ──► Grafana Cloud (your dashboard)
  files go device to device, encrypted; the server never holds or sees them
```

- **The server** runs two QurbSpace services, already built and measured
  ([packaging/server](../packaging/server/README.md)): *rendezvous*, which
  introduces devices, and *relay*, which carries encrypted bytes when two
  devices cannot reach each other directly. Neither holds files. Caddy in
  front gives the rendezvous service a real certificate.
- **The apps** are ordinary builds pointed at staging in their settings
  (Settings → Advanced → Rendezvous on the laptop; Settings → Connection on
  the phone). No separate staging build is needed.
- **The dashboard** is Grafana Cloud's free tier, fed by a small agent
  (Grafana Alloy) on the server: the services' logs and the server's
  traffic. Counts of QurbSpace's own — devices connected, introductions,
  bytes relayed — need the services to report them, which they do not yet
  (step 6).
- **The website** stays on Vercel; every branch already gets a preview
  address there, free.

## What it costs

| what | where | cost |
|---|---|---|
| server | Hetzner Cloud, smallest shared x86 plan (2 vCPU, 4 GB), Germany or Finland | about €4–5 a month — check the price shown when ordering — with 20 TB of traffic included, which is what the relay spends |
| domain | qurb.cloud, already yours | nothing more: two names under it |
| dashboard | Grafana Cloud free tier | nothing (14 days of logs; ample for one server) |
| website | Vercel, as now | nothing |
| app downloads | GitHub Releases, marked *pre-release* | nothing |
| phone app on Google Play | internal testing track, later | US $25 once, if and when wanted |

**About €5 a month.** Hetzner over the free tiers in the server README
because the relay's cost is bandwidth, and Hetzner includes 20 TB; Oracle's
free machines can be reclaimed when idle, which these services nearly always
are.

## The steps

**1. Server — you.** Make an account at hetzner.com/cloud (a card or PayPal
is asked for). *Create server*: location Nuremberg, Falkenstein or Helsinki;
image Ubuntu 24.04; the smallest shared x86 type; under *SSH keys*, add the
laptop's — the one line in `~/.ssh/id_ed25519.pub`. Create it, and send the
**IPv4 address** it shows. Under *Firewalls*, nothing yet: step 3 sets it.

**2. Two names — you.** Where qurb.cloud's DNS is managed (Vercel, under
*Domains*, if it was bought there): two **A records** pointing at the
server's IPv4 address — `signal.staging` and `relay.staging`.

**3. Services — me**, over SSH from the laptop, with what is in
`packaging/server/`: a user that owns nothing, the rendezvous and relay
services, Caddy for `signal.staging.qurb.cloud`, the server's firewall open
for 80, 443 and 9001 only, automatic security updates. Then a check from the
laptop across the internet, as `deploy.sh` does: two devices introduced, a
transfer through the relay.

**4. Dashboard — you, then me.** Make a free account at grafana.com and a
*stack*; under *Connections → Add new connection → Linux server*, it shows an
install command with a token — send that. I install the agent, send the
services' and Caddy's logs and the server's traffic, and make one dashboard:
traffic in and out, the services up or down, their logs searchable.

**5. Apps — me, then you.** *Done 2026-11-01, for updates:* the first beta,
[v0.2.0-beta.1](https://github.com/saqibsiddiq/Qurb/releases/tag/v0.2.0-beta.1),
built and signed by `.github/workflows/release.yml` and updating itself
([updates.md](updates.md)); the laptop runs its AppImage, the S23 its APK
(reinstalled once onto the release key). Still to do here: pointing both at
staging once the server is up. A pre-release on GitHub: the desktop app (Linux
.deb and AppImage; Windows built, untested) and the phone APK, **signed with
the release key** — see below. Then on your phone and laptop: point both at
`wss://signal.staging.qurb.cloud` and `relay.staging.qurb.cloud:9001`, and
try syncing with the phone on mobile data.

**6. QurbSpace's own numbers — me.** The services report counts on a local
port the agent reads: devices connected, introductions, relay sessions and
bytes. Never anything about a device beyond counts. Then they appear on the
dashboard.

## One decision to keep

**Sign every phone build with the release key from now on.** Android will
not update an app signed with another key without uninstalling it, which
loses the app's data — and the install marker that keeps one phone from
being counted twice ([0077](decisions/0077-one-device-is-counted-once.md))
is per signing key. The owner's S23 runs a build signed with the debug key
today; it moves to the release key once, at step 5, and is paired again.
That one move is **not** caught by the marker — Android gives an app signed
with another key another `ANDROID_ID` — so the new pairing is linked to the
old by its name, or by the owner's word on its page. The release key's file and password are
backed up somewhere other than this laptop before anything is published.

## Not here

- A Play Store listing, an iOS app, a production environment: after staging.
- Anything that holds files on the server. A *replica* can
  ([anywhere.md](anywhere.md#holding-content-for-devices-that-are-asleep)),
  and is a separate decision with its own cost in disk.
