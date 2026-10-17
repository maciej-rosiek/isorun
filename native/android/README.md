# Isorun for Android

The app (`native/crates/app`) as an APK: a `NativeActivity` that loads
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

It is `io.github.maciejrosiek.isorun`, the id of the Capacitor app it replaced, with the version
code that app had (from the version in `native/Cargo.toml`: 0.1.5 is 105) and the same release
key, so it installs over that app; releases carry it as `Isorun.apk`. It does not read the
Capacitor app's saves (they were in the WebView's IndexedDB). It does not update itself: a newer
APK installs over it, saves kept, and [Obtainium](https://obtainium.imranr.dev) (source
`https://github.com/maciej-rosiek/isorun`) finds `Isorun.apk` in each release.

## Build

    native/android/build.sh            # arm64-v8a and x86_64 (the emulator)
    native/android/build.sh aarch64    # a phone only

It needs the Android SDK (`ANDROID_HOME`, platform 35), an NDK r26 or newer
(`ANDROID_NDK_HOME`), `gradle` 8.9 or newer, a JDK 17 or 21, and the Rust targets
`aarch64-linux-android` and `x86_64-linux-android`. The APK is
`native/target/release-packages/Isorun.apk`. It is signed with the release key (below) when
`ISORUN_KEYSTORE` is set, and with the debug key otherwise.

## The release key

Every build must be signed with the same key, or Android will not install it over the last one
(a lost key means a new app, and the players' saves stay behind with the old one). It was made
once with

    keytool -genkeypair -v -keystore isorun.keystore -alias isorun -keyalg RSA -keysize 4096 -validity 20000

The build reads it from the environment: `ISORUN_KEYSTORE` (the file), `ISORUN_KEYSTORE_PASSWORD`,
and optionally `ISORUN_KEY_ALIAS` (default `isorun`) and `ISORUN_KEY_PASSWORD` (default the
store's). Without `ISORUN_KEYSTORE` the APK is signed with the debug key: it installs, but neither
it nor a release-signed APK will update the other. On GitHub the key is two repository secrets,
`ANDROID_KEYSTORE_BASE64` (`base64 -w0 isorun.keystore`) and `ANDROID_KEYSTORE_PASSWORD`.

## CI

`.github/workflows/isorun-android.yml` builds it on every push to `main` (the run's
artifact) and adds it to a release when one is published, or on a manual run with "upload".

## Smoke test

`smoke.sh <Isorun.apk> <kit>` plays the engine on a device or emulator over adb: crawl from a
shell, crawl in the app's own domain (run-as, with a debuggable build that carries
`offline_smoke` as `libsmoke.so`), and the app itself driven with key events, checking its log.
The workflow's `smoke` job runs it on an x86_64 emulator at a phone's size after every build; its
`smoke-out` artifact has the logs and screenshots.
