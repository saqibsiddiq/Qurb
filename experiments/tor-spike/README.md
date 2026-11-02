# tor-spike

Throwaway, for decision 0082's last resort: can Arti (Tor in Rust) host an
onion service on the laptop and reach it from a client, and at what cost in
start time, speed and size?

    cargo run --release -- host /tmp/tor-host          # prints the onion address
    cargo run --release -- client <onion> /tmp/tor-cli

Corners cut: an echo server, not qurb's protocol; one stream; no error
handling beyond `?`; no persistence of the onion key across runs beyond Arti's
own state directory. Kept out of the workspace so Tor's crates stay out of the
product's lockfile.

Since 2026-11-02 Tor is in the product, rewritten in `crates/peer/src/tor.rs`;
this spike stays as the record of the first measurements (decision 0082).
