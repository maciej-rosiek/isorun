# Isorun on the desktop

`package.sh` packages the app with the offline engine beside it (`engine/`, where
`client::engine` looks first), into `native/target/release-packages/`:

- `linux`: `Isorun-linux-x86_64.tar.gz` (a folder) and `Isorun-x86_64.AppImage`
  (with a `.zsync` for AppImageUpdate, from this repository's latest release).
- `windows`: `Isorun-win-x64.zip`, cross-compiled with mingw-w64.

Build the engine first: `engine/native.sh stable` (Linux) or `engine/native.sh stable --target
windows`. `.github/workflows/isorun-desktop.yml` builds them and adds them to a published
release; the app's Get the app screen and `STEAM.md` link them by these names from the latest
release. The AppImage's update information (`gh-releases-zsync|maciej-rosiek|isorun|latest|
Isorun-x86_64.AppImage.zsync`) lets AppImageUpdate, Gear Lever or AppImageLauncher update it
in place; the app does not update itself.
