#!/usr/bin/env bash
# Build a release of qurb: every package this machine can make, into
# dist/<version>/, with SHA256SUMS. Builds only; nothing is published.
#
#   ./scripts/release.sh                  # all of it
#   ./scripts/release.sh linux            # deb, rpm, Arch package
#   ./scripts/release.sh server           # the qurb program alone, static, x86_64 and arm64
#   ./scripts/release.sh android          # every ABI: an APK each, one for all, an app bundle
#   ./scripts/release.sh windows          # the window and the qurb program, zipped -- not run on Windows
#   ./scripts/release.sh sums             # SHA256SUMS of what is there
#
# What each package is for, what has been run on what, and what has not, is
# docs/releasing.md. Read it before handing anything out: a Windows build here
# has never run on Windows.
#
# Needs: cargo-tauri (`cargo install tauri-cli`), makepkg (Arch), llvm-strip, the Android
# NDK and SDK and the release key (android/README.md, "Signing"), and for the
# static and Windows builds Zig and cargo-zigbuild -- ZIG names the zig binary
# if it is not on PATH.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/desktop/Cargo.toml | head -1)
OUT=$ROOT/dist/$VERSION
mkdir -p "$OUT"
WHAT=${1:-all}

# Function names kept, for a readable backtrace; the rest of the debug
# information is not shipped: with it the qurb program alone is 327 MB.
export CARGO_PROFILE_RELEASE_DEBUG=false CARGO_PROFILE_RELEASE_STRIP=debuginfo

if [[ -n ${ZIG:-} ]]; then export PATH=$(dirname "$ZIG"):$PATH; fi

linux() {
    # No AppImage: it carries the building machine's libraries, and needs a
    # glibc as new as that machine's -- 2.44 here, which only the newest
    # distributions have (docs/releasing.md, "Known gaps").
    echo "== Linux: deb, rpm"
    # Apart from the everyday target/, so a release never ships whatever a
    # quick build left there, and a quick build never waits on a release.
    export CARGO_TARGET_DIR=$ROOT/target/package
    cargo build --release --locked -p qurb-cli --features push
    cargo build --release --locked -p qurb-tray
    (cd crates/desktop && cargo tauri build --config tauri.release.conf.json)
    local bundle=$CARGO_TARGET_DIR/release/bundle
    cp "$bundle"/deb/*.deb "$bundle"/rpm/*.rpm "$OUT/"
    unset CARGO_TARGET_DIR

    echo "== Linux: Arch package"
    (cd packaging/arch && QURB_RELEASE=$VERSION makepkg -f --noconfirm)
    cp packaging/arch/qurb-"$VERSION"-1-*.pkg.tar.zst "$OUT/"
}

server() {
    echo "== The qurb program, static"
    export CARGO_TARGET_DIR=$ROOT/target/cross
    # Zig's linker takes any stripping as all of it, function names too; with
    # no debug information built there is nothing else to strip.
    local CARGO_PROFILE_RELEASE_STRIP=none
    export CARGO_PROFILE_RELEASE_STRIP
    for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
        cargo zigbuild --release --locked -p qurb-cli --features push --target "$target"
        local arch=${target%%-*}
        local dir=$ROOT/target/cross/qurb-$VERSION-$arch-linux-static
        rm -rf "$dir" && mkdir -p "$dir"
        cp "$CARGO_TARGET_DIR/$target/release/qurb" "$dir/"
        # The standard library's debug information, which it comes with, out;
        # the function names stay.
        llvm-strip --strip-debug "$dir/qurb"
        cp packaging/server/README.md packaging/server/*.service packaging/server/Caddyfile "$dir/"
        tar -C "$(dirname "$dir")" -czf "$OUT/$(basename "$dir").tar.gz" "$(basename "$dir")"
    done
    unset CARGO_TARGET_DIR
}

android() {
    echo "== Android: every ABI"
    ./scripts/android-app.sh release-all
    local apk=android/app/build/outputs/apk/release bundle=android/app/build/outputs/bundle/release
    cp "$apk/app-universal-release.apk" "$OUT/qurb-$VERSION-android.apk"
    for abi in arm64-v8a armeabi-v7a x86 x86_64; do
        cp "$apk/app-$abi-release.apk" "$OUT/qurb-$VERSION-android-$abi.apk"
    done
    cp "$bundle/app-release.aab" "$OUT/qurb-$VERSION-android.aab"
    # Built since: the next everyday release is arm64 alone again.
    rm -rf android/app/src/main/jniLibs/{armeabi-v7a,x86,x86_64}
}

windows() {
    echo "== Windows -- built here, never run on Windows"
    export CARGO_TARGET_DIR=$ROOT/target/cross
    local target=x86_64-pc-windows-gnu
    # The window's icon and version go in a Windows resource, compiled by
    # MinGW's windres; LLVM's takes the same arguments, if MinGW is not here.
    if ! command -v x86_64-w64-mingw32-windres >/dev/null && command -v llvm-windres >/dev/null; then
        mkdir -p "$CARGO_TARGET_DIR/bin"
        printf '#!/bin/sh\nexec llvm-windres "$@"\n' > "$CARGO_TARGET_DIR/bin/x86_64-w64-mingw32-windres"
        chmod +x "$CARGO_TARGET_DIR/bin/x86_64-w64-mingw32-windres"
        export PATH=$CARGO_TARGET_DIR/bin:$PATH
    fi
    cargo zigbuild --release --locked -p qurb-cli --features push --target "$target"
    cargo zigbuild --release --locked -p qurb-desktop --target "$target"
    local dir=$ROOT/target/cross/qurb-$VERSION-windows-x86_64
    rm -rf "$dir" && mkdir -p "$dir"
    cp "$CARGO_TARGET_DIR/$target/release/qurb.exe" "$CARGO_TARGET_DIR/$target/release/qurb-desktop.exe" "$dir/"
    # The window asks for WebView2Loader.dll, which Windows does not have: a
    # program made with MinGW's tools ships it beside itself, from Microsoft's
    # WebView2 SDK, which allows that. Without it the window would not start
    # (found reading the program's imports, 2026-10-19).
    cp "$CARGO_TARGET_DIR/$target/release/WebView2Loader.dll" "$dir/"
    cp packaging/windows/README.txt "$dir/"
    (cd "$(dirname "$dir")" && rm -f "$OUT/$(basename "$dir").zip" && python3 -m zipfile -c "$OUT/$(basename "$dir").zip" "$(basename "$dir")")
    unset CARGO_TARGET_DIR
}

sums() {
    (cd "$OUT" && rm -f SHA256SUMS && sha256sum -- * > SHA256SUMS && cat SHA256SUMS)
}

case "$WHAT" in
    all) linux; server; android; windows; sums ;;
    linux|server|android|windows|sums) "$WHAT"; sums ;;
    *) echo "usage: release.sh [all|linux|server|android|windows|sums]" >&2; exit 2 ;;
esac
echo
echo "In $OUT"
