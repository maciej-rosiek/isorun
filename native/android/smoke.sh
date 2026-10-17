#!/bin/sh
# Plays the offline engine on an Android device or emulator (adb on PATH, one device), as the CI
# does after building the APK (isorun-native-android.yml):
#
#   native/android/smoke.sh <Isorun.apk> <smoke dir>
#
# The smoke dir holds `offline_smoke` (crates/client/examples, built for the device's ABI), crawl
# beside it as `libcrawl.so`, and the engine folder `engine/` (engine/native.sh's).
#
# 1. crawl on the device from a shell: offline_smoke starts a game, makes a character, plays a few
#    turns and leaves; its output has the engine's crash report if crawl stops badly.
# 2. the app itself: installed and started, the home screen's Play row taken with Enter, crawl's
#    character choice made with `a`s, a few keys played; the log (logcat) must say the game drew
#    its map and must not say it stopped badly. The app's log and a screenshot of each step are
#    left in $SMOKE_OUT (default ./smoke-out).
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
if ! adb shell "cd /data/local/tmp/smoke && ISORUN_DATA_DIR=/data/local/tmp/smoke/data ISORUN_ENGINE_DIR=/data/local/tmp/smoke/engine SMOKE_SECONDS=900 ./offline_smoke" 2>&1 | tee "$OUT/shell.txt"; then
    fail=1
fi
grep -q '^ok' "$OUT/shell.txt" || { echo "::error::crawl did not play from a shell"; fail=1; }

echo "== 2. the app"
adb install -r "$APK" || { echo "::error::the APK did not install"; exit 1; }
adb logcat -c
shot() {
    adb exec-out screencap -p > "$OUT/$1.png" 2>/dev/null || true
}
key() {
    adb shell input "$@"
    sleep 4
}
adb shell am start -W -n "$PKG/android.app.NativeActivity"
# the first start unpacks the engine's data
for i in $(seq 1 60); do
    adb logcat -d -s isorun:* | grep -q 'unpacked the offline engine\|offline engine was not unpacked' && break
    sleep 2
done
sleep 10
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
grep -E ' (isorun|Crawl|crawl|DEBUG|libc|AndroidRuntime) *:|F DEBUG|Fatal signal|tombstone' "$OUT/logcat.txt" | tail -300
grep -q 'offline game drew its first map' "$OUT/logcat.txt" || { echo "::error::the app's game drew no map"; fail=1; }
if grep -q 'the offline game stopped\|offline engine did not start\|offline game ended: \(crash\|error\)' "$OUT/logcat.txt"; then
    echo "::error::the app's game stopped badly"
    fail=1
fi
exit $fail
