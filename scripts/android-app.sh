#!/usr/bin/env bash
# Build the Android app.
#
#   ./scripts/android-app.sh            # debug APK
#   ./scripts/android-app.sh release    # unsigned release APK
#   ./scripts/android-app.sh install    # debug APK, installed on a connected device
#
# Three steps, kept separate on purpose. Cargo is not wired into Gradle: a Rust
# change is an explicit step here rather than something that happens invisibly
# inside an IDE, and the app builds from a clean checkout on a machine with no
# NDK as long as the .so files are already in place.
set -euo pipefail
cd "$(dirname "$0")/.."

MODE=${1:-debug}
export JAVA_HOME=${JAVA_HOME:-$(ls -d "$HOME"/Android/jdk-17* 2>/dev/null | head -1)}
[[ -d ${JAVA_HOME:-} ]] || { echo "No JDK 17. Set JAVA_HOME." >&2; exit 1; }

# The NDK, for the cross-compiler and for `llvm-strip` below.
find_ndk() {
    if [[ -n ${ANDROID_NDK_HOME:-} ]]; then echo "$ANDROID_NDK_HOME"; return; fi
    for c in "$HOME"/Android/android-ndk-* "$HOME"/Android/Sdk/ndk/* /opt/android-ndk; do
        [[ -d $c/toolchains/llvm/prebuilt ]] && { echo "$c"; return; }
    done
    return 1
}
NDK=$(find_ndk) || { echo "No Android NDK. See scripts/android-build.sh." >&2; exit 1; }

echo "== 1/3  native libraries"
./scripts/android-build.sh --release

# Only the two ABIs the app ships. The others build and have never been run on
# hardware, and shipping an untested binary is a claim this project has not
# earned -- see app/build.gradle.kts.
declare -A ABI=( [arm64-v8a]=aarch64-linux-android [x86_64]=x86_64-linux-android )

# Stripped here rather than by Gradle. A release build carries debug
# information -- 61 MB per architecture against 3.7 MB without -- and an APK
# holding both unstripped is 271 MB for an app whose whole engine is 4 MB.
# Gradle can strip, but only with an NDK on the build machine, and this script
# has already found one.
STRIP=$(find_ndk >/dev/null 2>&1 && true)
NDK_BIN=$(ls -d "$NDK/toolchains/llvm/prebuilt"/*/bin 2>/dev/null | head -1)

for abi in "${!ABI[@]}"; do
    src="target/${ABI[$abi]}/release/libqurb_mobile.so"
    [[ -f $src ]] || { echo "missing $src" >&2; exit 1; }
    mkdir -p "android/app/src/main/jniLibs/$abi"
    dest="android/app/src/main/jniLibs/$abi/libqurb_mobile.so"
    cp "$src" "$dest"

    if [[ -x $NDK_BIN/llvm-strip ]]; then
        "$NDK_BIN/llvm-strip" "$dest"
    fi
    printf '   %-12s %s -> %s\n' "$abi" "$(du -h "$src" | cut -f1)" "$(du -h "$dest" | cut -f1)"
done

echo "== 2/3  Kotlin bindings"
# Regenerated every build. Bindings that drift from the library they describe
# fail at runtime rather than at compile time, which is a bad way to find out.
rm -rf android/app/src/main/java/uniffi
./scripts/mobile-bindings.sh target/bindings >/dev/null
cp -r target/bindings/kotlin/uniffi android/app/src/main/java/
echo "   $(find android/app/src/main/java/uniffi -name '*.kt' | wc -l) file(s)"

echo "== 3/3  Gradle"
GRADLE=$(find "$HOME/.gradle/wrapper/dists" -name gradle -type f -path '*/bin/*' 2>/dev/null | head -1)
GRADLE=${GRADLE:-$(command -v gradle || true)}
[[ -x ${GRADLE:-} ]] || { echo "No gradle found." >&2; exit 1; }

cd android
case "$MODE" in
    release) "$GRADLE" --no-daemon assembleRelease ;;
    install) "$GRADLE" --no-daemon installDebug ;;
    *)       "$GRADLE" --no-daemon assembleDebug ;;
esac

echo
find app/build/outputs/apk -name '*.apk' 2>/dev/null | while read -r apk; do
    printf '%s  (%s)\n' "$apk" "$(du -h "$apk" | cut -f1)"
done
