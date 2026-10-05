# Phone serving

Throwaway. Where a phone's serving of a large file spends its time, measured
on the phone itself rather than through the app. It found the cause of the
5 MB/s that [decision 0050](../../docs/decisions/0050-large-files-from-a-phone.md)
did not fix: [decision 0051](../../docs/decisions/0051-bbr-not-cubic.md).

`src/main.rs` builds a device holding one file of random bytes, either as
sealed chunks in its store or as a file in its folder (the two ways a phone
holds content), and then:

- `read` takes every chunk the way the server answers a request for it, with
  the visibility check, read, decrypt, decompress and hash, and no network;
- `loop` serves it and fetches it over QUIC on loopback, in one process;
- `serve` and `fetch` do the same between two machines;
- `blast-send` and `blast-recv` send raw UDP, paced or not, with no congestion
  control, to find what the path itself carries.

```bash
cargo build --release -p phone-serving
# for the phone: the NDK's clang as linker, as scripts/android-build.sh sets up
cargo build --release -p phone-serving --target aarch64-linux-android
adb push target/aarch64-linux-android/release/phone-serving /data/local/tmp/

# on the phone
./phone-serving prepare ps/sealed 256 sealed
./phone-serving read ps/sealed
./phone-serving loop ps/sealed
./phone-serving serve ps/sealed 47555 <the laptop's fingerprint>   # from: phone-serving id <dir>
./phone-serving blast-send 47557
# on the laptop
phone-serving fetch <dir> 192.168.1.4:47555 <the phone's fingerprint>
phone-serving blast-recv 192.168.1.4:47557 40 20                    # 40 MB at 20 MB/s
```

`WORKERS=n` sets the runtime's worker threads; the default is two, as in the
app. The congestion controller was compared by a temporary switch in
`crates/peer/src/tls.rs`, since removed; BBR is now what the code uses.

## Conditions

2026-10-05. A Galaxy S23 (SM-S911B), the spike running as a shell process over
ADB, not in the app; the development laptop, Intel i5-13420H. Both on the
home Wi-Fi, 5 GHz channel 36: the phone's link 468/526 Mbit/s, the laptop's
351/243. Release builds, 256 MiB files, each fetch three times in a row. The
phone was in use for other apps during some runs.

## Results

| | laptop | S23 |
|---|---:|---:|
| `read`, sealed chunks | 584.9 MB/s | 96.5 MB/s |
| `read`, the file | 951.0 MB/s | 717.7 MB/s |
| `loop`, sealed | 275–375 MB/s | 63.5–72.1 MB/s |
| `loop`, the file | 305–390 MB/s | 68.5–73.5 MB/s |

Phone to laptop over Wi-Fi, 256 MiB sealed:

| | rate |
|---|---:|
| raw UDP asked for at 5 / 10 / 20 / 40 MB/s | 5.00 / 9.97 / 14.90 / 15.08 MB/s received, 0.0–0.4% lost |
| raw UDP, unpaced | 11.69 MB/s received, 1.3% lost |
| QUIC, Cubic | 5.34, 5.16 MB/s |
| QUIC, NewReno | 6.11, 5.94, 5.92 MB/s |
| QUIC, BBR | 13.25, 12.49, 14.03 MB/s |

## Corners cut

- Measured outside the app: no foreground service, no Android scheduling of
  an app process. The app built with BBR was not measured; the phone left the
  network first.
- One phone, one laptop, one Wi-Fi network, on one afternoon.
- The UDP blaster busy-waits to pace, and its unpaced run overflows the
  phone's socket buffer. It shows the path's ceiling, not a clean
  measurement of it.
- The laptop's firewall refused an inbound TCP connection, so every test has
  the laptop dialling the phone. Nothing here measured laptop to phone.
