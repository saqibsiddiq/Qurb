#!/usr/bin/env bash
# Run the test suite on a connected Android device or emulator.
#
#   ./scripts/android-test.sh              # x86_64, which is what an emulator is
#   ./scripts/android-test.sh aarch64      # a real phone
#
# This is the only thing that answers the question "does it work on Android?".
# Cross-compiling proves the toolchain is right and nothing more: it says
# nothing about bionic's libc, Android's filesystem semantics, its SELinux
# policy, or what its kernel does to a process that allocates.
#
# The mechanism is deliberately crude. Cargo builds the test binaries, adb
# pushes them somewhere executable, and they run. There is no app, no Gradle,
# and no JVM involved -- the FFI is a C ABI, and a test binary exercises the
# same Rust an app would call through it.
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH=${1:-x86_64}
case "$ARCH" in
    x86_64)  TARGET=x86_64-linux-android;      CLANG=x86_64-linux-android ;;
    aarch64) TARGET=aarch64-linux-android;     CLANG=aarch64-linux-android ;;
    *) echo "usage: android-test.sh [x86_64|aarch64]" >&2; exit 2 ;;
esac

API=${ANDROID_API:-26}
find_ndk() {
    if [[ -n ${ANDROID_NDK_HOME:-} ]]; then echo "$ANDROID_NDK_HOME"; return; fi
    for c in "$HOME"/Android/android-ndk-* "$HOME"/Android/Sdk/ndk/* /opt/android-ndk; do
        [[ -d $c/toolchains/llvm/prebuilt ]] && { echo "$c"; return; }
    done
    return 1
}
NDK=$(find_ndk) || { echo "No NDK. See scripts/android-build.sh." >&2; exit 1; }
HOST=$(ls "$NDK/toolchains/llvm/prebuilt" | head -1)
BIN="$NDK/toolchains/llvm/prebuilt/$HOST/bin"

ADB=${ADB:-$(command -v adb || echo "$HOME/Android/Sdk/platform-tools/adb")}
[[ -x $ADB ]] || { echo "adb not found; set ADB=" >&2; exit 1; }
"$ADB" get-state >/dev/null 2>&1 || { echo "No device. Start an emulator or plug one in." >&2; exit 1; }

upper=$(echo "$TARGET" | tr 'a-z-' 'A-Z_')
export "CC_${TARGET//-/_}=$BIN/$CLANG$API-clang"
export "AR_${TARGET//-/_}=$BIN/llvm-ar"
export "CARGO_TARGET_${upper}_LINKER=$BIN/$CLANG$API-clang"

# /data/local/tmp rather than /sdcard: the latter is mounted noexec, and the
# failure ("Permission denied" on a file that is plainly executable) sends you
# looking in the wrong place entirely.
REMOTE=/data/local/tmp/qurb
"$ADB" shell "rm -rf $REMOTE && mkdir -p $REMOTE"

# Every crate that runs on a device. The networking ones are included on
# purpose: they open real sockets, do a real QUIC handshake and really punch
# through to each other on loopback, so leaving them out would mean the answer
# to "does it work on Android?" quietly excluded the interesting half.
#
# `qurb` itself is absent because it is a desktop program with a terminal
# interface, and `phase0-spike` because it is throwaway.
CRATES=${QURB_TEST_CRATES:-"qurb-storage qurb-watcher qurb-sync qurb-engine \
    qurb-keys qurb-peer qurb-signal qurb-relay qurb-mobile"}

echo "Building test binaries for $TARGET..."
args=(); for c in $CRATES; do args+=(-p "$c"); done
mapfile -t BINARIES < <(
    cargo test "${args[@]}" --target "$TARGET" --release --no-run --message-format=json 2>/dev/null \
    | python3 -c '
import json,sys
for line in sys.stdin:
    try: m = json.loads(line)
    except ValueError: continue
    if m.get("profile", {}).get("test") and m.get("executable"):
        print(m["executable"])
'
)

(( ${#BINARIES[@]} )) || { echo "cargo produced no test binaries" >&2; exit 1; }

# Some tests spawn a helper: `crash.rs` runs `crash_writer` and kills it, which
# is the only way to test a real crash rather than a simulated one. It finds the
# helper two directories up from the test binary, in `examples/`, so the layout
# on the device has to match the layout cargo produces.
cargo build "${args[@]}" --target "$TARGET" --release --examples >/dev/null 2>&1 || true
EXAMPLES="target/$TARGET/release/examples"
if [[ -d $EXAMPLES ]]; then
    "$ADB" shell "mkdir -p /data/local/tmp/examples"
    for example in "$EXAMPLES"/*; do
        [[ -f $example && -x $example && $example != *.d ]] || continue
        "$ADB" push "$example" "/data/local/tmp/examples/$(basename "$example")" >/dev/null
        "$ADB" shell "chmod 755 /data/local/tmp/examples/$(basename "$example")"
    done
fi
echo "Running ${#BINARIES[@]} test binaries on $("$ADB" shell getprop ro.product.model | tr -d '\r')"
echo

failed=()
for binary in "${BINARIES[@]}"; do
    name=$(basename "$binary")
    "$ADB" push -q "$binary" "$REMOTE/$name" >/dev/null 2>&1 || "$ADB" push "$binary" "$REMOTE/$name" >/dev/null
    "$ADB" shell "chmod 755 $REMOTE/$name"

    printf '=== %s\n' "$name"
    # TMPDIR matters: Rust's tempfile falls back to /tmp, which does not exist
    # on Android. Without it every test that makes a temporary directory fails
    # for a reason that has nothing to do with the code under test.
    #
    # Captured rather than piped straight into grep: `grep -q` exits on its
    # first match, which closes the pipe and loses the rest of the output --
    # so a run would report a pass and show almost none of the results.
    output=$("$ADB" shell "cd $REMOTE && TMPDIR=$REMOTE RUST_BACKTRACE=1 ./$name --test-threads=2" 2>&1 || true)
    printf '%s\n' "$output" | tr -d '\r'

    # Every binary must actually say it passed. A binary that crashed before
    # printing anything produces no "test result" line at all, and treating a
    # missing line as success is how a suite reports green on a device where
    # nothing ran.
    if ! printf '%s' "$output" | grep -q "^test result: ok"; then
        failed+=("$name")
    fi
    echo
done

"$ADB" shell "rm -rf $REMOTE /data/local/tmp/examples"

if (( ${#failed[@]} )); then
    echo "FAILED on device: ${failed[*]}" >&2
    exit 1
fi
echo "All test binaries passed on device."
