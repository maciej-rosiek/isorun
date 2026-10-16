# Isorun desktop

Isorun as a desktop app: the game and the offline engine in an Electron window. It is for a
Steam Deck, a ROG Ally or any PC, and it plays offline games with no network.

The app serves the built game (`apps/isorun/dist`) and the engine (`engine/dist`) from its own
files at `https://isorun.app/`, together with the gamedata and morgue proxies (`src/serve.ts`).
Requests to other hosts go to the network as usual. Server games still need the network.

## Build

1. Build the offline engine: `engine/build.sh stable` (see `../CLAUDE.md`).
2. Install the tools: `npm install` in this folder (this folder is not an npm workspace).
3. Make the packages: `npm run package`.

The packages are in `dist/release`:

- `Isorun-win32-x64.zip`: Windows.
- `Isorun-linux-x64.tar.gz`: Linux and SteamOS.

`npm start` opens the app from the current builds, without packages.

## Play from Steam

1. Extract the package.
2. In Steam, select "Add a Non-Steam Game" and select `Isorun.exe` (Windows) or `Isorun` (Linux).
3. In the controller settings of the game, set the layout to "Gamepad".

The app starts in full screen. F11 toggles full screen, and `--windowed` starts it in a window.
The Quit row on the home screen closes it. If the Linux app does not start on SteamOS, add
`--no-sandbox` to the launch options.
