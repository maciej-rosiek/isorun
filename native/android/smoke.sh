#!/bin/sh
# Plays the offline engine on an Android device or emulator (adb on PATH, one device), as the CI
# does after building the APK (isorun-android.yml):
#
#   native/android/smoke.sh <Isorun.apk> <smoke dir>
#
# The smoke dir holds `offline_smoke` (crates/client/examples, built for the device's ABI), crawl
# beside it as `libcrawl.so`, the engine folder `engine/` (engine/native.sh's), and optionally
# `Isorun-smoke.apk`: the app, debuggable, with offline_smoke in it as `libsmoke.so`.
#
# 1. crawl from a shell: offline_smoke starts a game, makes a character, plays a few turns and
#    leaves; its output has the engine's crash report if crawl stops badly.
# 2. crawl as the app runs it (Isorun-smoke.apk): the same, with run-as, so in the app's own
#    SELinux domain and folder, crawl run from the app's native library folder, its data the
#    one the app unpacked. Once with socket files (ISORUN_SOCKET=fs, to see whether Android
#    allows them; it does not fail the run) and once as the app has it.
# 3. the app itself: started, the home screen's Play row taken with Enter, crawl's character
#    choice made with `a`s, a few keys played; the log (logcat) must say the game drew its map
#    and must not say it stopped badly.
# The logs and a screenshot of each step are left in $SMOKE_OUT (default ./smoke-out).
set -u

APK=$1
DIR=$2
OUT=${SMOKE_OUT:-smoke-out}
PKG=io.github.maciejrosiek.isorun
mkdir -p "$OUT"
fail=0

adb wait-for-device
adb shell getprop ro.product.cpu.abi
adb shell getprop ro.build.version.release

echo "== 1. crawl from a shell"
adb shell rm -rf /data/local/tmp/smoke
adb push "$DIR" /data/local/tmp/smoke > /dev/null
adb shell chmod 755 /data/local/tmp/smoke/offline_smoke /data/local/tmp/smoke/libcrawl.so
adb shell "cd /data/local/tmp/smoke && ISORUN_DATA_DIR=/data/local/tmp/smoke/data ISORUN_ENGINE_DIR=/data/local/tmp/smoke/engine SMOKE_SECONDS=900 ./offline_smoke" 2>&1 | tee "$OUT/shell.txt"
grep -q '^ok' "$OUT/shell.txt" || { echo "::error::crawl did not play from a shell"; fail=1; }

wait_unpacked() {
    for i in $(seq 1 60); do
        adb logcat -d -s isorun:* | grep -q 'unpacked the offline engine\|offline engine was not unpacked' && return
        sleep 2
    done
}

if [ -f "$DIR/Isorun-smoke.apk" ]; then
    echo "== 2. crawl as the app runs it (run-as)"
    adb install -r "$DIR/Isorun-smoke.apk" || { echo "::error::the smoke APK did not install"; exit 1; }
    adb logcat -c
    adb shell am start -n "$PKG/android.app.NativeActivity" > /dev/null
    wait_unpacked
    adb shell am force-stop "$PKG"
    lib=$(adb shell pm path "$PKG" | tr -d '\r' | sed -n 's#^package:\(.*\)/base.apk$#\1#p')/lib/x86_64
    files=/data/data/$PKG/files
    for sock in fs app; do
        env="ISORUN_DATA_DIR=$files ISORUN_ENGINE_DIR=$files/engine SMOKE_SECONDS=900"
        [ $sock = fs ] && env="$env ISORUN_SOCKET=fs"
        adb shell "run-as $PKG sh -c 'cd $files && $env $lib/libsmoke.so Runas$sock'" 2>&1 | tee "$OUT/run-as-$sock.txt"
        if ! grep -q '^ok' "$OUT/run-as-$sock.txt"; then
            if [ $sock = fs ]; then
                echo "::warning::with socket files, crawl did not play in the app's domain"
            else
                echo "::error::crawl did not play in the app's domain"
                fail=1
            fi
        fi
    done
    adb logcat -d | grep -E 'avc: *denied' | tail -40 | tee "$OUT/denials.txt"
    APK="$DIR/Isorun-smoke.apk"
fi

echo "== 3. the app"
adb install -r "$APK" || { echo "::error::the APK did not install"; exit 1; }
adb logcat -c
shot() {
    adb exec-out screencap -p > "$OUT/$1.png" 2>/dev/null || true
}
key() {
    adb shell input "$@"
    sleep 4
}
adb shell am start -n "$PKG/android.app.NativeActivity"
wait_unpacked
sleep 20
shot 1-home
key keyevent KEYCODE_ENTER
# crawl's first start builds its caches: wait for the character choice, or the map
for i in $(seq 1 90); do
    adb logcat -d -s isorun:* | grep -q 'offline game drew its first map\|offline game ended\|did not start' && break
    adb shell input text a
    sleep 4
done
shot 2-start
for k in "text o" "keyevent KEYCODE_ESCAPE" "text i" "keyevent KEYCODE_ESCAPE" "text s" "text s" "text o" "keyevent KEYCODE_ESCAPE"; do
    key $k
done
shot 3-played
adb shell ps -A | grep -i 'crawl\|isorun' || true
adb logcat -d > "$OUT/logcat.txt"
echo "-- the app's log"
grep -E ' (isorun|Crawl|crawl|DEBUG|AndroidRuntime|RustStdoutStderr) *:|Fatal signal|ANR in|avc: *denied.*isorun' "$OUT/logcat.txt" | grep -v 'Dropped frame' | tail -200
# the emulator's GPU (SwiftShader through gfxstream) times out most of the app's frames, so the
# app may not get as far as a game: only a game that stopped badly fails the run
grep -q 'offline game drew its first map' "$OUT/logcat.txt" || echo "::warning::the app's game drew no map (the emulator's GPU drops its frames: $(grep -c 'Dropped frame' "$OUT/logcat.txt") dropped)"
if grep -q 'the offline game stopped\|offline engine did not start\|offline game ended: \(crash\|error\)' "$OUT/logcat.txt"; then
    echo "::error::the app's game stopped badly"
    fail=1
fi
exit $fail
