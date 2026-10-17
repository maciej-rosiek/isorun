# Isorun native on the desktop

`package.sh` packages the native app with the offline engine beside it (`engine/`, where
`client::engine` looks first), into `native/target/release-packages/`:

- `linux`: `Isorun-native-linux-x86_64.tar.gz` (a folder) and `Isorun-native-x86_64.AppImage`
  (with a `.zsync` for AppImageUpdate, from this repository's latest release).
- `windows`: `Isorun-native-win-x64.zip`, cross-compiled with mingw-w64.

Build the engine first: `engine/native.sh stable` (Linux) or `engine/native.sh stable --target
windows`. The names differ from the Electron app's (`desktop/`), so both fit in one release;
`.github/workflows/isorun-native.yml` builds them and adds them to a published release.
