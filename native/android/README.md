# Isorun native for Android

The native app (`native/crates/app`) as an APK: a `NativeActivity` that loads
`libisorun_app.so` (`android_main` in `crates/app/src/lib.rs`), with no Java or Kotlin. The
offline engine is crawl cross-compiled with the NDK (`engine/native.sh --target android-<arch>`,
with `engine/patches/14-android-webtiles.patch`), shipped as `lib/<abi>/libcrawl.so`: Android
lets an app run programs only from its native library folder, so the manifest asks for the
libraries to be unpacked (`extractNativeLibs`). crawl's data (`dat`, `docs`, `settings`) and the
client's gamedata are assets under `engine/`, listed in `engine/files.txt`; the app unpacks them
into its files (`files/engine`) on the first start and whenever the APK carries another build,
and `client::engine` runs crawl from there. crawl's WebTiles channel is a Unix datagram socket in the abstract namespace
(`engine/patches/15-abstract-socket.patch`): Android's SELinux policy refuses socket files in
the data folders (EACCES in the shell's on the emulator). A game that ends badly logs crawl's
report (its signal, its error output, its crash file) under the tag `isorun`: `adb logcat -s
isorun:* Crawl:*`.

It is `io.github.maciejrosiek.isorun`, the Capacitor app's id (`mobile/`), with the same version
code (the game's version in `apps/isorun/package.json`) and the same release key, so it installs
over the Capacitor app and replaces it; releases carry it as `Isorun.apk`. It does not read the
Capacitor app's saves (they are in the WebView's IndexedDB).

## Build

    native/android/build.sh            # arm64-v8a and x86_64 (the emulator)
    native/android/build.sh aarch64    # a phone only

It needs the Android SDK (`ANDROID_HOME`, platform 35), an NDK r26 or newer
(`ANDROID_NDK_HOME`), `gradle` 8.9 or newer, a JDK 17 or 21, and the Rust targets
`aarch64-linux-android` and `x86_64-linux-android`. The APK is
`native/target/release-packages/Isorun.apk`. It is signed with the release key of
`mobile/README.md` (`ISORUN_KEYSTORE`, `ISORUN_KEYSTORE_PASSWORD`, `ISORUN_KEY_ALIAS`,
`ISORUN_KEY_PASSWORD`) when they are set, and with the debug key otherwise.

`.github/workflows/isorun-native-android.yml` builds it on every push to `main` (the run's
artifact) and adds it to a release when one is published, or on a manual run with "upload".

## Smoke test

`smoke.sh <Isorun.apk> <kit>` plays the engine on a device or emulator over adb: crawl from a
shell, crawl in the app's own domain (run-as, with a debuggable build that carries
`offline_smoke` as `libsmoke.so`), and the app itself driven with key events, checking its log.
The workflow's `smoke` job runs it on an x86_64 emulator at a phone's size after every build; its
`smoke-out` artifact has the logs and screenshots.
