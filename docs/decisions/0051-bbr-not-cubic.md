# 0051 — QUIC paces by measured bandwidth (BBR), not by loss (Cubic)

**Status:** Accepted — built; measured with the spike and through the app,
10.6–12.1 MB/s against 5 (see *Checked, and not*); explains what
[0050](0050-large-files-from-a-phone.md)'s eight-in-flight did not
**Date:** 2026-10-05

## What happened

After [0050](0050-large-files-from-a-phone.md), a large file still left the
Galaxy S23 at about 5 MB/s. That held whether the laptop asked for one chunk at
a time or eight, and on links of 468 and 351 Mbit/s. The laptop's own log of
the connection showed a 13 ms round trip and nothing lost. So the requests
were not the limit, and the question was whether the phone could not serve
faster or the path could not carry more.

`experiments/phone-serving` answered it on the phone itself, outside the app.
It ran as a shell process on the S23, with the app's two runtime threads, a
256 MiB file of random bytes, and release builds. The laptop and the phone were
on the same home Wi-Fi, 5 GHz:

| measured | result |
|---|---|
| reading every chunk as a request would, no network: sealed chunks / the file | 96.5 / 717.7 MB/s |
| serving and fetching on the phone's own loopback, QUIC and all | 63.5–73.5 MB/s |
| raw UDP, phone to laptop, no congestion control | about 15 MB/s, 0.1–0.4% lost |
| QUIC, phone to laptop, Cubic (quinn's default) | 5.16–5.34 MB/s |
| QUIC, phone to laptop, NewReno | 5.92–6.11 MB/s |
| QUIC, phone to laptop, **BBR** | **12.49–14.03 MB/s** |

The phone serves more than ten times faster than it was sending, and the path
carries three times as much. What held it back was the congestion controller.
Cubic and NewReno read a lost packet as congestion and cut their window. Wi-Fi
loses a fraction of a percent of packets for other reasons (radio
interference, power saving, retries running out), so they kept cutting and never
filled the path. BBR paces to the bandwidth and round trip it measures, and
does not treat an isolated loss as a signal.

## Decision

**Every connection that carries files uses quinn's BBR**
(`paced_by_bandwidth` in `crates/peer/src/tls.rs`), in both the server's and
the client's configuration, which every sync and every request for a chunk
goes through. The pairing listener keeps quinn's defaults: it exchanges a
handful of small messages, and there is nothing for a congestion controller to
do.

## Why this, and not something else

- **Tuning Cubic**, for example a larger initial window, changes how fast it
  climbs, not how far it falls at each loss. The loss is the problem.
- **NewReno** was measured, and is no better.
- **Fewer, larger requests** do nothing for a sender whose window is small.
  Eight in flight already left the requests far from the limit (0050).

## What it costs

- **Quinn calls its BBR experimental.** It is BBR version 1. Used for every
  connection, it runs in every test and every sync; a fault would show there,
  and switching back is one line.
- **Fairness.** BBRv1 is known to take more than its share of a bottleneck
  from loss-based flows. A large qurb transfer on a shared home link may now
  slow a video call on it more than Cubic would have. A sync program is the
  thing that should yield. Not measured; if it shows, the transfer can be
  paced below the measured bandwidth.
- **Queueing.** BBRv1 can hold more in a router's queue than a loss-based
  sender, which adds latency for everything else on the link while a transfer
  runs.

## Checked, and not

- The network tests in `crates/peer`, and every other test, run with BBR on
  loopback.
- **Through the app**, later on 2026-10-05: 11.88 MB/s with the app open,
  10.81 for 1 GiB with the app left, and 10.59 and 12.05 resuming after the
  laptop restarted. One resumed run went at 1.33 MB/s with a 124 ms round
  trip and is not explained; repeated, the same test ran at 12.05. See
  [phase 5](../phases/phase-5-mobile.md#through-the-app-with-bbr--and-a-phone-cleared).
- **Not measured**: on mobile data, through the relay, with other traffic
  sharing the link, or between two desktops on a wired network.

## Reversing it

Remove the call to `paced_by_bandwidth`; quinn falls back to Cubic.
