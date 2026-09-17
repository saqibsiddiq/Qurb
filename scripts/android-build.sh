#!/usr/bin/env bash
# Cross-compile the engine for Android.
#
# The core is portable Rust with one exception: SQLite is C, and building it
# needs a cross-compiler from the Android NDK. Everything else builds with the
# Rust targets alone. This script finds the NDK and points cargo at it.
#
#   ./scripts/android-build.sh              # all four architectures, debug
#   ./scripts/android-build.sh --release    # all four, release
#   ANDROID_NDK_HOME=/path ./scripts/android-build.sh
#
# The four exist because Android has shipped all of them: arm64 is every phone
# sold in the last decade, armv7 is older and cheaper hardware, and the two x86
# targets are emulators, which is where most testing actually happens.
set -euo pipefail

# API 26 (Android 8.0, 2017) is the floor. It is where the NDK's 64-bit file
# APIs are complete, which SQLite needs to address files over 2 GB.
API=${ANDROID_API:-26}

find_ndk() {
    if [[ -n ${ANDROID_NDK_HOME:-} ]]; then echo "$ANDROID_NDK_HOME"; return; fi
    for candidate in "$HOME"/Android/android-ndk-* "$HOME"/Android/Sdk/ndk/* \
                     /opt/android-ndk /usr/lib/android-ndk; do
        [[ -d $candidate/toolchains/llvm/prebuilt ]] && { echo "$candidate"; return; }
    done
    return 1
}

NDK=$(find_ndk) || {
    cat >&2 <<'MSG'
No Android NDK found.

Set ANDROID_NDK_HOME, or put one in ~/Android/. Download it from
https://developer.android.com/ndk/downloads — it is about 2 GB.

Without it, everything except qurb-storage still cross-compiles, because
SQLite is the only C dependency in the tree.
MSG
    exit 1
}

HOST=$(ls "$NDK/toolchains/llvm/prebuilt" | head -1)
BIN="$NDK/toolchains/llvm/prebuilt/$HOST/bin"
[[ -d $BIN ]] || { echo "NDK at $NDK has no $HOST toolchain" >&2; exit 1; }

echo "NDK:    $NDK"
echo "API:    $API"
echo

# Rust target triple -> the NDK's name for the same thing. They differ for
# armv7, where the NDK spells the ABI suffix and Rust does not.
declare -A CLANG=(
    [aarch64-linux-android]=aarch64-linux-android
    [armv7-linux-androideabi]=armv7a-linux-androideabi
    [i686-linux-android]=i686-linux-android
    [x86_64-linux-android]=x86_64-linux-android
)

# Crates that make up the engine. The CLI is excluded on purpose: it is a
# desktop program with a terminal interface, and the phone will call the
# library directly rather than shell out to it.
CRATES=(qurb-storage qurb-watcher qurb-sync qurb-engine qurb-keys qurb-peer qurb-mobile)

failed=()
for target in "${!CLANG[@]}"; do
    prefix="${CLANG[$target]}$API"
    upper=$(echo "$target" | tr 'a-z-' 'A-Z_')

    export "CC_${target//-/_}=$BIN/$prefix-clang"
    export "CXX_${target//-/_}=$BIN/$prefix-clang++"
    export "AR_${target//-/_}=$BIN/llvm-ar"
    export "CARGO_TARGET_${upper}_LINKER=$BIN/$prefix-clang"

    echo "=== $target"
    args=()
    for c in "${CRATES[@]}"; do args+=(-p "$c"); done
    if cargo build "${args[@]}" --target "$target" "$@"; then
        echo "    ok"
    else
        failed+=("$target")
    fi
    echo
done

if (( ${#failed[@]} )); then
    echo "failed: ${failed[*]}" >&2
    exit 1
fi
echo "All targets built."
