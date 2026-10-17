# Isorun native for Android

The native app (`native/crates/app`) as an APK: a `NativeActivity` that loads
`libisorun_app.so` (`android_main` in `crates/app/src/lib.rs`), with no Java or Kotlin. The
offline engine is crawl cross-compiled with the NDK (`engine/native.sh --target android-<arch>`,
with `engine/patches/14-android-webtiles.patch`), shipped as `lib/<abi>/libcrawl.so`: Android
lets an app run programs only from its native library folder, so the manifest asks for the
libraries to be unpacked (`extractNativeLibs`). crawl's data (`dat`, `docs`, `settings`) and the
client's gamedata are assets under `engine/`, listed in `engine/files.txt`; the app unpacks them
into its files (`files/engine`) on the first start and whenever the APK carries another build,
and `client::engine` runs crawl from there.

It is `io.github.maciejrosiek.isorun`, the Capacitor app's id (`mobile/`), with the same version
code (the game's version in `apps/isorun/package.json`) and the same release key, so it installs
over the Capacitor app (whose saves it does not read: they are in the WebView's IndexedDB).
Releases carry it as `Isorun-native.apk` beside the Capacitor app's `Isorun.apk` until it is
complete.

## Build

    native/android/build.sh            # arm64-v8a and x86_64 (the emulator)
    native/android/build.sh aarch64    # a phone only

It needs the Android SDK (`ANDROID_HOME`, platform 35), an NDK r26 or newer
(`ANDROID_NDK_HOME`), `gradle` 8.9 or newer, a JDK 17 or 21, and the Rust targets
`aarch64-linux-android` and `x86_64-linux-android`. The APK is
`native/target/release-packages/Isorun-native.apk`. It is signed with the release key of
`mobile/README.md` (`ISORUN_KEYSTORE`, `ISORUN_KEYSTORE_PASSWORD`, `ISORUN_KEY_ALIAS`,
`ISORUN_KEY_PASSWORD`) when they are set, and with the debug key otherwise.

`.github/workflows/isorun-native-android.yml` builds it on every push to `main` (the run's
artifact) and adds it to a release when one is published, or on a manual run with "upload".
