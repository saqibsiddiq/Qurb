#!/usr/bin/env bash
# Generate the Kotlin and Swift bindings for crates/mobile-ffi.
#
#   ./scripts/mobile-bindings.sh [output-directory]
#
# UniFFI reads the *compiled* library rather than the source, which is why this
# builds first and why the generator is a binary inside the crate rather than a
# tool installed separately: it has to be built against the same uniffi version
# the library was.
#
# The output is checked by eye, not committed. Both platforms' build systems
# regenerate it — Gradle through a build task, Xcode through a build phase —
# because bindings that drift from the library they describe fail at runtime
# rather than at compile time, and that is a bad way to find out.
set -euo pipefail

OUT=${1:-target/bindings}
cd "$(dirname "$0")/.."

echo "Building qurb-mobile..."
cargo build -p qurb-mobile

LIB=target/debug/libqurb_mobile.so
[[ -f $LIB ]] || LIB=target/debug/libqurb_mobile.dylib
[[ -f $LIB ]] || { echo "no built library found" >&2; exit 1; }

for language in kotlin swift; do
    echo "Generating $language..."
    # --no-format because ktlint and swift-format are not needed to read the
    # result, and requiring them would make this fail on a machine that has
    # neither for no benefit.
    cargo run -q -p qurb-mobile --bin uniffi-bindgen -- \
        generate --library "$LIB" --language "$language" --no-format \
        --out-dir "$OUT/$language"
done

echo
echo "Written to $OUT:"
find "$OUT" -type f | sed 's|^|    |'
