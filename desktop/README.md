# Isorun desktop

Isorun as a desktop app: the game and the offline engine in an Electron window. It is for a
Steam Deck, a ROG Ally or any PC, and it plays offline games with no network.

The app serves the built game (`apps/isorun/dist`) and the engine (`engine/dist`) from its own
files at `https://isorun.app/`, together with the gamedata and morgue proxies (`src/serve.ts`).
Requests to other hosts go to the network as usual. Server games still need the network.

Each [release](https://github.com/maciej-rosiek/isorun/releases) has three packages:

- `Isorun-x86_64.AppImage`: Linux and SteamOS. It updates itself: on each start it looks for a
  newer release, downloads it in the background, and replaces the AppImage when the app closes
  (electron-updater). The next start is the new version.
- `Isorun-linux-x64.tar.gz`: Linux and SteamOS, a plain folder. It does not update itself.
- `Isorun-win-x64.zip`: Windows. It does not update itself.

## Play from Steam

1. Download `Isorun-x86_64.AppImage` from the latest
   [release](https://github.com/maciej-rosiek/isorun/releases/latest).
2. Make it executable: `chmod +x Isorun-x86_64.AppImage`.
3. In Steam (Desktop Mode), select "Add a Non-Steam Game" and select the AppImage.
4. In the controller settings of the game, set the layout to "Gamepad".

You do this one time. The updates replace the AppImage at the same path, so the Steam shortcut
stays correct.

The app starts in full screen. F11 toggles full screen, and `--windowed` starts it in a window.
The Quit row on the home screen closes it. If the app does not start, add `--no-sandbox` or
`--ozone-platform=x11` to the launch options.

## Build

1. Build the offline engine: `engine/build.sh stable` (see `../CLAUDE.md`).
2. Install the tools: `npm install` in this folder (this folder is not an npm workspace).
3. Make the packages: `npm run package`. They are in `dist/release`.

`npm start` opens the app from the current builds, without a package.

## Release

1. Increase `version` in `apps/isorun/package.json`. The updater installs only a higher version.
2. Run `npm run release`. It builds the packages and uploads them with `latest-linux.yml` to a
   draft release `v<version>` (the token comes from `gh auth token`).
3. Publish the draft: `gh release edit v<version> --draft=false`. The apps update from then on.
