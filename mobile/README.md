# Isorun for Android

Isorun as an Android app: the game and the offline engine in an APK, made with
[Capacitor](https://capacitorjs.com). It plays offline games with no network; server games
and watching need the network, as on the web.

The APK carries the game built for the app (`VITE_PLATFORM=android`, see below) and the stable
channel of the engine (`engine/dist`), and Capacitor's WebView serves them at
`https://isorun.localhost/`. Saves are in the WebView's IndexedDB, in the app's own data, so they
stay through updates and go when the app is uninstalled.

The engine waits for the player in a JSPI suspension (`WebAssembly.Suspending`), which the
Android System WebView has from version 137 (it updates from the Play Store). With an older one
the app opens, and Offline asks for a newer browser, as the web page does (`servers.ts`
`offlineRuns`); server games still play.

## What the app build changes

`VITE_PLATFORM=android` (`servers.ts` `androidApp`):

- No service worker and no Cache Storage (`engines.ts`): the engine is played from the APK's
  files as they are (the `EngineStore` with no `caches`), not copied again. The app updates
  with a new APK, not on a deploy.
- The gamedata and morgue proxies and the visit beacon are the hosted page's
  (`hostedBase`, `https://isorun.maciej-rosiek.workers.dev`), whose Worker answers them with
  CORS: the app has no server of its own.

`android/` is the native project (`npx cap add android`, then ours): `MainActivity` hides the
system bars (a swipe from the edge shows them for a moment), the phone's Back reaches the page,
which takes it as Escape (`back-button.ts`), the splash is black, the launcher icon is the orb
(`sh icons.sh` writes it from the brand), and `app/build.gradle` takes the version from
`apps/isorun/package.json` (`versionCode` 0.1.5 is 105) and the release key from the
environment.

## Build

Needs Node 22+, a JDK 21 and the Android SDK (Android Studio, or the command-line tools with
`ANDROID_HOME` set).

1. Build the offline engine: `engine/build.sh stable` (see `../CLAUDE.md`).
2. Install the tools: `npm install` in this folder (this folder is not an npm workspace).
3. Make the APK: `npm run package`. It is `dist/release/Isorun.apk`.

`npm run stage` builds only the files (`dist/www`), and `npx cap sync android && npm run open`
opens the project in Android Studio, to run it on a phone or an emulator. With the phone on USB,
`chrome://inspect` on the computer shows the WebView's console.

## The release key

Every build of the app must be signed with the same key, or Android will not install it over the
last one. Make one once and keep it safe (a lost key means a new app, and the players' saves stay
behind with the old one):

```sh
keytool -genkeypair -v -keystore isorun.keystore -alias isorun -keyalg RSA -keysize 4096 -validity 20000
```

The build reads it from the environment: `ISORUN_KEYSTORE` (the file), `ISORUN_KEYSTORE_PASSWORD`,
and optionally `ISORUN_KEY_ALIAS` (default `isorun`) and `ISORUN_KEY_PASSWORD` (default the store's).
Without `ISORUN_KEYSTORE` the release is signed with the debug key: it installs, but neither it nor
a release-signed APK will update the other.

For GitHub Actions, add two repository secrets: `ANDROID_KEYSTORE_BASE64` (`base64 -w0 isorun.keystore`)
and `ANDROID_KEYSTORE_PASSWORD`.

## Release

The workflow `.github/workflows/isorun-android.yml` builds the engine and the APK on GitHub. It runs
when a release is published, and adds `Isorun.apk` to it; a manual run (Actions, Build the Isorun
Android app, Run workflow) keeps the APK as the run's artifact, and with "upload" adds it to the
release `v<version>` too.

So, beside the desktop release (`../desktop/README.md`): increase `version` in
`apps/isorun/package.json`, run `npm run release` in `desktop/`, and publish the draft. The APK
follows a few minutes later. `npm run release` here does the same from this machine.

A player installs the APK from the release page (Android asks to allow installs from the browser
once). The app does not update itself: a newer APK installs over it, saves kept.

To update from the releases, add the app to [Obtainium](https://obtainium.imranr.dev): Add App, the
source `https://github.com/maciej-rosiek/isorun`. It looks at each release for an `.apk` (only
`Isorun.apk` is one), reads the version from the tag (`v0.1.5`) and installs the newer one.
