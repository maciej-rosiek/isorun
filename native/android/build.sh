#!/bin/sh
# Builds the native app's APK: the Rust app (crates/app, a NativeActivity cdylib) for each ABI,
# crawl cross-compiled for each (engine/native.sh --target android-<arch>, shipped as
# lib/<abi>/libcrawl.so), the engine's data as assets, and the Gradle project here around them.
#
#   native/android/build.sh [aarch64] [x86_64]     # default: aarch64 x86_64
#
# Writes native/target/release-packages/Isorun.apk. Needs:
#   ANDROID_HOME          the SDK (platforms;android-35, build-tools), with `gradle` on PATH
#   ANDROID_NDK_HOME      an NDK (r26 or newer)
#   rustup targets aarch64-linux-android, x86_64-linux-android
# Signed with the release key when ISORUN_KEYSTORE and ISORUN_KEYSTORE_PASSWORD are set
# (mobile/README.md), with the debug key otherwise. SKIP_ENGINE=1 reuses the staged crawl builds.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
NATIVE=$(cd "$HERE/.." && pwd)
ROOT=$(cd "$NATIVE/.." && pwd)
OUT=${OUT:-$NATIVE/target/release-packages}
API=${ANDROID_API:-26}
[ $# -gt 0 ] || set -- aarch64 x86_64
[ -n "${ANDROID_NDK_HOME:-}" ] || { echo "set ANDROID_NDK_HOME to an NDK" >&2; exit 1; }
BIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin

STAGE=$HERE/build/stage
rm -rf "$STAGE"
mkdir -p "$STAGE/jniLibs" "$STAGE/assets" "$STAGE/res/mipmap-xxxhdpi" "$OUT"
cp "$ROOT/apps/isorun/public/icons/icon-512.png" "$STAGE/res/mipmap-xxxhdpi/ic_launcher.png"

for arch in "$@"; do
    case $arch in
        aarch64) abi=arm64-v8a ;;
        x86_64) abi=x86_64 ;;
        *) echo "unknown ABI $arch (aarch64, x86_64)" >&2; exit 1 ;;
    esac
    triple=$arch-linux-android
    upper=$(echo "$triple" | tr 'a-z-' 'A-Z_')
    # the app
    (
        cd "$NATIVE"
        export "CARGO_TARGET_${upper}_LINKER=$BIN/$triple$API-clang"
        export "CC_$(echo "$triple" | tr '-' '_')=$BIN/$triple$API-clang"
        export "AR_$(echo "$triple" | tr '-' '_')=$BIN/llvm-ar"
        cargo build --release -p isorun-app --lib --target "$triple"
    )
    mkdir -p "$STAGE/jniLibs/$abi"
    cp "$NATIVE/target/$triple/release/libisorun_app.so" "$STAGE/jniLibs/$abi/"
    "$BIN/llvm-strip" "$STAGE/jniLibs/$abi/libisorun_app.so"
    # crawl, as a library file Android unpacks and lets the app run
    engine=$ROOT/engine/dist/native/android-$arch/engine
    [ -n "${SKIP_ENGINE:-}" ] || ANDROID_API=$API "$ROOT/engine/native.sh" stable --target "android-$arch"
    cp "$engine/crawl" "$STAGE/jniLibs/$abi/libcrawl.so"
done

# the data, the same for every ABI, and its list (lib.rs unpack_engine)
mkdir -p "$STAGE/assets/engine"
(cd "$engine" && tar --exclude ./crawl -cf - .) | (cd "$STAGE/assets/engine" && tar -xf -)
(cd "$STAGE/assets/engine" && find . -type f ! -name engine.json ! -name files.txt | sed 's#^\./##' | sort > files.txt)

# the game's version, as the Capacitor app has it (mobile/android/app/build.gradle), so one
# orders after the other: 0.1.5 -> 105
version=$(sed -n 's/^  "version": "\(.*\)",/\1/p' "$ROOT/apps/isorun/package.json" | head -1)
code=$(echo "$version" | awk -F. '{ print $1 * 10000 + $2 * 100 + $3 }')
(cd "$HERE" && gradle --no-daemon -q assembleRelease -PisorunVersion="$version" -PisorunVersionCode="$code")
cp "$HERE/app/build/outputs/apk/release/app-release.apk" "$OUT/Isorun.apk"
ls -la "$OUT/Isorun.apk"
