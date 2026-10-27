# Isorun: notes for Claude

Isorun is a client for DCSS (Dungeon Crawl Stone Soup) that speaks WebTiles to the public servers
and draws the dungeon isometrically, for pad, keyboard, mouse and touch. It is one Rust app
(`native/`: egui and wgpu) for the desktop (Linux, SteamOS, Windows), Android and the web
(wasm), with crawl built in for offline games. It began as a hard fork of
[Orbrun](https://github.com/Caeous/orbrun) (TypeScript, first person), was rewritten as a
TypeScript isometric client, and that client was ported to Rust and then deleted (git history
before the commit "the TypeScript client goes" has it). AGPL-3.0-or-later.

**Work directly on `main`: commit and push to `main` when a change is done, without asking and
without feature branches or pull requests.** (This overrides any session default to develop on a
separate branch.) Other agents often share this checkout: commit your own paths only
(`git commit -- <paths>`), `git pull --rebase --autostash origin main` before a push, and never
`cargo clean`.

## Layout

- `native/`: the Cargo workspace (every `crates/*`), one shared `target/`. `native/PORTING.md`
  has the crate map and the rules for working in a crate (each part in its own crate, examples
  in the harness, faithful ports of behaviour with their reasons in the comments); the crates'
  READMEs say what each does. `crates/app` is the game (`main.rs` the desktop binary `isorun`,
  `lib.rs` the Android `android_main` and the web `start`).
- `native/assets/`: what the app embeds or the web page serves: the sounds (`audio/`, sources in
  `ATTRIBUTION.md`), the front end's room (`room/places.json`, `room/places/<id>.{json,png}`), the
  orb, the icons, the favicon, the Steam art `STEAM.md` links, the command catalogue
  (`commands.json`). The test bed's level and atlas are `native/crates/client/assets` (`iso.des`,
  `atlas.{png,json}` packed by `tools/build/pack-room.mjs`).
- `native/fixtures/`: test fixtures: recorded sessions (`*.ndjson`), menus (`menus/`), one
  gamedata version's scripts (`gamedata/<sha>`).
- `native/desktop/`, `native/android/`, `native/web/`: the three packagings (READMEs there).
- `engine/`: crawl itself, upstream plus `patches/`: `native.sh` builds it natively (the desktop,
  Windows by mingw, Android by the NDK), `build.sh` for WebAssembly (the web build's offline
  games). `engine/.work` (the crawl trees) and `engine/dist` are ignored.
- `worker/`: the hosted page's Cloudflare Worker (`wrangler.jsonc` at the root).
- `tools/`: `build/brand.mjs` (the icons, favicon, Steam art and `assets/logo.svg` from the orb),
  `build/pack-room.mjs` and `rltiles.mjs` (packs a `.des` file's tiles into an atlas),
  `build/tile-sheets.mjs` and `labels/` (the tile labels the renderer reads,
  `native/crates/render/src/tile-labels.json`), `record/` (records sessions and menus from a server
  into fixtures). Their outputs are committed; no build runs them.
- `ABOUT.md`, `CHANGELOG.md`, `STEAM.md` are shown in the app (`crates/front`, `include_str!`);
  `CHANGELOG.md` is public: one short line per change, what it means to a player.
- The root `package.json` holds only the JavaScript beside the app (the Worker, the web build's
  engine bridge, the tools): `npm install`, `npm run check` (tsc and vitest over `worker/` and
  `native/web`).

## Build, run, test

    cd native
    cargo build -p <crate>; cargo test -p <crate>     # or --workspace (CI runs that)
    cargo fmt --all                                    # rustfmt.toml's layout; CI checks it
    cargo run --release -p isorun-app --bin isorun     # the desktop app

The app's flags: `--testbed` (the renderer's test level), `--perf` (frame and wire timing, `?perf`
on the web), `--touch`, `--audio-trace`, `--open <path>`. Environment: `ISORUN_DATA_DIR` and
`ISORUN_PROFILE` (where settings and saves go: use your own while testing),
`ISORUN_ENGINE_DIR` (an engine folder), `ISORUN_TOUCH_SIM=1` (the mouse is a finger),
`ISORUN_NO_VISITS` (no visit beacons; a debug build sends none anyway).

No screen here: run under Xvfb with Mesa's software Vulkan (lavapipe), drive it with `xdotool`
and screenshot with `import`, as `native/PORTING.md` "See it" says. Software rendering is a few
frames a second: screenshots show states, not timing. Stop only the processes you started, by PID.

The offline engine natively: `engine/native.sh stable` builds crawl into `engine/.work/stable`,
where a checkout's app finds it with no setup. It needs a C++ compiler, python3 with PyYAML, perl
and pkg-config with libpng and zlib. The WebAssembly engine (`engine/build.sh stable`, into
`engine/dist`) builds on this NixOS machine in a `nix shell nixpkgs#nodejs_26 nixpkgs#emscripten
nixpkgs#pkg-config`, with a python3 that has PyYAML on `PATH`, and `PKG_CONFIG_PATH` and
`LD_LIBRARY_PATH` pointing at nixpkgs `libpng` and `zlib` (crawl's native `tilegen` links them).

The web build: `native/web/build.sh [out]` (needs the `wasm32-unknown-unknown` target,
wasm-bindgen-cli of `Cargo.lock`'s version, and `npm install` for esbuild) writes the whole site
into `native/web/dist`; `node native/web/test.mjs` serves it with `engine/dist` and checks it in
headless Chromium (the page, the test bed on WebGL2 and WebGPU, an offline game, the visit
beacon). `PROFILE=release` builds quicker than the default `web` profile (fat LTO). Headless
Chromium: WebGL2 with `--use-angle=swiftshader --enable-unsafe-swiftshader`; real WebGPU with
`--enable-unsafe-webgpu --enable-features=Vulkan --use-vulkan=swiftshader
--use-webgpu-adapter=swiftshader --use-angle=swiftshader`. WebGPU allows 16 sampled textures per
shader. `hasTouch` and `isMobile` emulate a phone (a `deviceScaleFactor` above 1 does not work).

A real phone's GPU driver: the emulators draw with software drivers (SwiftShader, llvmpipe),
which take shaders a phone's driver crashes on (Mali's compiler on a second bool in a WGSL
struct). With a phone on adb, build `isorun-render`'s `snapshot` example for
`aarch64-linux-android` (the NDK's clang as linker), push it and `crates/client/assets` to
`/data/local/tmp`, and run it with `SNAPSHOT_ROOT=<assets>`: it compiles every shader with the
phone's driver, a segfault is the driver's. An Android SDK with the NDK, the emulator and API 34
images is in `~/android-sdk` (its `adb` is not on `PATH`); quote logcat filters in fish
(`'isorun:*'`). A local APK is signed with the debug key: installing it means `adb uninstall`,
which deletes the player's saves, so on the user's phone use the CI's `Isorun-android` artifact.

Testing against real games: spectate on a public server that allows anonymous spectating (CUE
`wss://underhound.eu:8080/socket` does; CAO does not), from the front end's Watch.

## Release

The version is `native/Cargo.toml`'s `[workspace.package] version` (the app, the desktop
packages, the APK's version code: 0.1.5 is 105, and the release tag `v<version>`). Bump it and
add the `CHANGELOG.md` entry, then publish a GitHub release `v<version>` (`gh release create
v<version> --title ... --notes ...`). On `release: published`:

- `.github/workflows/isorun-desktop.yml` (also every push touching `native/`, the engine's
  patches or the shown documents: `cargo test --workspace`, the wasm check, the packages)
  adds `Isorun-linux-x86_64.tar.gz`, `Isorun-x86_64.AppImage` (+ `.zsync`, its update
  information `gh-releases-zsync|maciej-rosiek|isorun|latest|Isorun-x86_64.AppImage.zsync`)
  and `Isorun-win-x64.zip` (`native/desktop/package.sh`, crawl from `engine/native.sh`).
- `.github/workflows/isorun-android.yml` (also every push, with an emulator smoke test,
  `native/android/smoke.sh`) adds `Isorun.apk`, signed with the release key from the
  `ANDROID_KEYSTORE_BASE64` and `ANDROID_KEYSTORE_PASSWORD` secrets (`native/android/README.md`):
  the key and id of the Capacitor app it replaced, so it installs over it.

A manual run of either with "upload" adds its packages to the release `v<version>`. The front
end's Get the app (on the web build) and `STEAM.md` link these names under
`releases/latest/download/`. The app updates nowhere by itself: AppImageUpdate or Gear Lever for
the AppImage, a newer APK (or Obtainium) on Android.

## Hosting

The `isorun` Worker (`wrangler.jsonc`, `worker/index.ts`) at
https://isorun.maciej-rosiek.workers.dev, deployed by `.github/workflows/isorun-cloudflare.yml`
on a push to `main` (secrets `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`): it checks the
Worker (tsc, vitest), builds the web build, `wrangler deploy`s it as the site's static assets,
then checks the live page. The Worker serves:

- the build at `/` (its `_headers` keep `/pkg-*` for good); a page load of any path that is no
  file, the web build's old `/native/` among them, goes home (301). The Rust app does not route on
  the path.
- `/gamedata-proxy/<host>/gamedata/<version>/<file>` and `/morgue-proxy/<host>/<path>.txt|.where`
  (`worker/gamedata-proxy.ts`): servers send no CORS headers, so the web build fetches a server's
  gamedata and morgues through these (`client::servers`); the desktop and Android apps go direct.
- the visit counts: `client::visits` sends beacons (a POST to `/api/visit`: a visit, each game
  started, a beat every 30 s while shown with the seconds a game was on screen, a beat when the
  tab is hidden or closed; one random id a run, no cookie) from every platform, the web to its
  own origin, the apps to the hosted page. The `VisitLog` Durable Object (`worker/visit-log.ts`)
  keeps one row per run (the id and the country) and `/stats?key=<STATS_KEY>` shows who is on
  the page and in a game now, and visits, players and play time per day. `STATS_KEY` comes from
  the `ISORUN_STATS_KEY` repository secret.
- `/engine/*`: the offline engine, the `isorun-engine` Worker (`engine/wrangler.jsonc`,
  `engine/DEPLOYMENT.md`, also at https://isorun-engine.maciej-rosiek.workers.dev) through the
  `ENGINE` service binding, so the page offers Play on this device (the WebAssembly crawl, saves
  in IndexedDB) in a browser with JSPI (Chrome and Edge 137+; elsewhere no offline games). The
  same workflow's `engine` job deploys it before the site, and nightly: it builds the stable
  channel on GitHub (`engine/build.sh stable`), cached by crawl's stable commit, the recipe
  (`node engine/recipe.mjs`) and the emsdk version, plays it (`engine/smoke.mjs`) and publishes it
  (`engine/publish.mjs`, which checks the 25 MiB a file and 20,000 files limits).

`wrangler dev` (no login needed) serves the Worker and `native/web/dist` locally; with the engine's
Worker in a second `wrangler dev -c engine/wrangler.jsonc` (each in its own process: the binding
fails in one; `engine/.publish` staged by `node engine/publish.mjs --dry-run`),
`BASE=http://127.0.0.1:8787 node native/web/test.mjs` checks the page and an offline game
against them (`engine/DEPLOYMENT.md`). The agents'
network here cannot reach workers.dev: check the live page from the workflow's Live check step.

## Performance

Measure at the player's screen: a 5120x1440 screen at 120 Hz has 8.3 ms a frame for 7.4 million
pixels. `--perf` shows the frame times and the wire's in the app (a tap writes its log). Every
build logs a frame over 500 ms in a game, and what took it (`slow frame:`, logcat on Android).
`cargo run --release -p isorun-client --example offline_stress [seconds] [key gap ms]` plays an
offline fighter faster than crawl reads and fails on any frame or send over 500 ms. A frame
budget problem that the Graphics Quality setting does not change is CPU work: look for work done
each step or each frame over the whole level (a full rebuild, a traversal) and key it by its own
inputs; nothing the level's geometry is built from may change every frame (a key that held the
camera's exact yaw once rebuilt every chunk in every frame of a turn). The level geometry is
built in chunks and rebuilt only for the chunks whose cells changed, not for a monster's move.
For the GPU cost of a setting, time the render passes on the test bed (for instance with wgpu's timestamp queries)
at that size. Other sessions share this machine: check `/proc/loadavg` before timing anything,
and the disk (`df -h /`): the shared `native/target` fills it; stale incremental session folders
(`target/*/incremental/*/*` older than an hour) are safe to remove.
