# Isorun: notes for Claude

Isorun is a fork of [Orbrun](https://github.com/Caeous/orbrun) (a controller-first WebTiles client
for DCSS) that draws the dungeon isometrically instead of in first person. AGPL-3.0-or-later, like
upstream.

**Work directly on `main`: commit and push to `main` when a change is done, without asking and
without feature branches or pull requests.** (This overrides any session default to develop on a
separate branch.) The repository moved here from `maciej-rosiek/vg` (its `isorun/` folder) as one
snapshot commit, without the history.

- A hard fork of Orbrun: no upstream pulls; the code is ours to change and clean up.
- Every package has a README; `CONTRIBUTING.md` is the short guide.
- The client (`apps/isorun`, `npm run dev`, then `/`): `game.ts` draws the game with `IsoRender3d`
  and an `IsoCamera` (`packages/render-iso/src/camera.ts`: follow spring, 45-degree turns, tilt,
  zoom, drag; its angles kept in `isorun.camera`). Directions are screen-relative (`runner.ts`
  `absolute` rotates by the camera's `facing`); `runner.heading` is the way the player last stepped,
  what the contextual action calls "ahead". The level map (X) opens in 2D (`Render2d`) on its own
  canvas over the iso view. Fights come from the message lines (`combat.ts` `combatEvents`, read on
  each `state` event before the scene they come with) and play through `IsoRender3d.act`. Camera:
  wheel zoom, Shift-wheel and Alt-Left / Right turn, Alt-Up / Down tilt, a drag pans, the right
  stick turns and zooms, R3 resets. The settings (`servers.ts`, `isorun.settings`) have a Graphics
  group: Quality (`QUALITY`), Look (`LIGHTS`, or Custom) and Tune the look, which opens the lights
  panel (`src/lights-panel.ts`, shared with the test bed) over the game; its values are Custom. The front end's room (`src/room`) draws the places
  in iso with a camera that turns slowly.
- The HUD setting (`hud`, Dock by default) puts the dock along the foot of the screen (`src/dock.ts`,
  styles in `src/dock.css`): orbs, the status lights as chips with their descriptions, AC, EV, SH and the
  attributes, quick slots (`quick-slots.ts`, kept per character in
  `dock-memory.ts`), the square's actions (`dock-actions.ts`) and buttons for every screen. The
  inventory and character screens are the client's own (`screens.ts`, `Overlays.showScreen`); what is
  worn is read off crawl's pack, opened unseen behind the inventory (`game.ts` `probeGear`, `gear.ts`),
  since the `player` message names only the weapons. Spells and abilities are learned from crawl's
  menus whenever they are up. L3 puts the pad's cursor on the dock. An action from a screen puts it
  away and brings it back once crawl is done (`trackResume`).
  The dock and its screens size by `--hu` (dock.css; `hud-unit.ts` computes the same for the tile
  canvases): 1px at 1080 lines, more on a taller window, times the UI scale. Tooltips are cards
  (`tooltip.ts`: title, rule, lines, keycaps and the pad glyph for the real key each thing sends). In the
  inventory a click, Enter or A opens an item's actions beside it (`Screen.input` takes the keys while
  it is up), and hovering an item shows its card beside what it would replace, with base stats
  from crawl's item-prop.cc (`item-compare.ts`). In command mode `i` and `@` open the inventory and
  character screens.
- Touch screens and phones (`touch.ts`, `touch.css`): a window under 900 x 560 css px gets the compact dock
  (`Dock` `compact`, bars in place of the orbs, two message rows in landscape and four in portrait, the view's
  centre lifted above the panes with `IsoRender3d.setCentreLift`). On a coarse pointer: the full screen and settings
  buttons in the top-left corner, a close button on every popup (`touch-close`), the touch bar (`touch-bar.ts`) with
  the pad's bindings as buttons in every mode but command mode, a long press describes a cell, two fingers zoom,
  the walls drop to stubs over every floor cell within 6 cells of the player (`setCutAwayReach`), and the phone's
  Back is Escape (`back-button.ts`, `GameScreen.back`). Test it with `isMobile` and `hasTouch` in Playwright.
- A level up (the message "You have reached level N") shows a banner (`level-up.ts`, `level-up.css`, owned by
  `Overlays.levelUp`) with the gains read off the `player` message; it fades on its own, a key or click dismisses it
  (one that answers a --more-- lets it stay a moment for the gains). Crawl's stat-gain prompt
  (`ParsedPrompt.statGain`) is drawn as three cards on the same screen instead of the prompt card.
- `IsoRender3d` builds the level geometry again only when a cell, an item or a doorway changes
  (`staticsRevision`) or the set of cut-away walls changes (`cutAwayKey`), never for a monster's
  move. The geometry is in chunks of 8x8 cells (`CHUNK_CELLS`); a rebuild builds only the chunks
  whose input key changed, and doors, stairs (`TileFeatures`) and water build only when their
  cells change. A chunk's matrices are set once (`matrixWorldAutoUpdate = false`). Tiles are upscaled on
  first use onto 4096-texel pages (`enhance.ts` `EnhancedPage`, `materials.ts`), not whole atlases,
  so every GPU's texture limit holds.
- The offline engine (`engine/build.sh stable`, into the ignored `engine/dist`, served by the dev
  server at `/engine/`) builds on this NixOS machine in a `nix shell nixpkgs#nodejs_26
  nixpkgs#emscripten nixpkgs#pkg-config`, with a python3 that has PyYAML on `PATH`, and
  `PKG_CONFIG_PATH` and `LD_LIBRARY_PATH` pointing at nixpkgs `libpng` and `zlib` (crawl's native
  `tilegen` links them). With it the title screen offers Play on this device: real games to test.
- Testing against real games: spectate on a public server that allows anonymous spectating (CUE
  `wss://underhound.eu:8080/socket` does; CAO does not): open `/watch/cue/<anyone>` and click a
  player.
- Our code: `packages/render-iso` (the isometric renderer `IsoRender3d`, and `stickToDir` for
  screen-relative steering) and the test bed `apps/isorun/testbed/iso.html` (`npm run dev`, then
  `/testbed/iso.html`). The test bed has one look and one camera (rotation 45, elevation 34 at zoom
  68, lower when zoomed in and higher when zoomed out; wall height 1; the view follows the player on
  a spring that is slower during mouse travel; Q / E turn it by 45 degrees, R / F tilt it, and a Camera box on the view (`testbed/camera-panel.ts`, kept in `localStorage` `isorun.camera`) has sliders and presets for them and for Perspective, the field of view: 0 is the isometric view, more switches `IsoRender3d` to a perspective camera, `IsoView.fov`), an FPS counter, and a lights menu (`testbed/lights-panel.ts`) for the
  values in `packages/render-iso/src/lights.ts`. The menu keeps its values in `localStorage`
  (`isorun.lights.current`, and named presets in `isorun.lights.saved`).
- `IsoRender3d` runs on three's `WebGPURenderer` (WebGL2 when WebGPU is missing, as in headless
  Chromium), with node materials and a TSL `RenderPipeline` (`post.ts`: GTAO, outlines, a painted
  Kuwahara filter, bloom, a tilt-shift blur, a colour grade and a vignette). Every tile material is
  a `ClayMaterial` (`shading.ts`, a Phong node material with its own lighting model); its uniforms
  are shared, so clones follow the settings. Sprites (actors, items) use a second material per
  atlas whose colour follows the sprite light (`spriteColour`), so bright sprites keep their detail.
  The ground and wall material steps into the tile's height (`parallaxUV`, the Shading Depth, 0.075
  by default): the normal page's blue channel is the height (`enhance.ts` `tileDepth`), the normal's z is
  rebuilt from x and y, and each quad carries its texture rect (`UV_RECT`) so the steps stay on it.
  The loop has no early exit, as WGSL wants the same path for every pixel after it.
  The aim pointer writes 1 to the pass's `overlay` output (`overlayOutput()` as its `mrtNode`), so
  the screen effects leave it as it is; it is depth tested, so the player hides it. The presets
  in `lights.ts` are variations of `DIORAMA`.
  Fog of war: the renderer hides `unseen` cells, draws `remembered` cells darker, shows monsters
  only on `visible` cells, and stacks layers of drifting fog (`fog.ts`: a dense body with a puffy edge that the noise moves; Softness is the width of that edge) over the unexplored cells
  around the explored area; the fog finds the ground cell behind each of its points on the screen (along the ray from the camera)
  and fades over explored cells; the fog's shape and the explored mask are textures over the
  map, and a new explored area blends them from the shape on screen to the new one (`FADE_SECONDS`). Water is drawn (`water.ts`): the nearest land's floor tile lies
  under it, and a surface with a rounded shore, shallow to deep colour, drifting light lines and
  foam covers it. The mouse picks the ground cell, or the nearest floor cell on the
  screen over a wall or the fog; walls in front of that cell drop to stubs, as for the player
  (`cutAwayFocus`). The paint settings (`enhance.ts`) change the
  upscaled tiles, so a change rebuilds the atlas materials and the actor sprites.
  `NodeMaterial.clone()` drops `alphaTest`; set it again on clones.
- Liquids: `liquidOf` (`packages/scene-webtiles`) names a cell's liquid from its tile (crawl puts
  `TILE_FLAG_WATER` on lava too): water, murky water, starwater, toxic bog, mud (liquefied ground),
  slime and acid floor, or lava, deep or shallow. `WaterLayer` draws the waters, the bog, the mud and
  the slime and acid films over their floor tile (`water.ts` palettes, shallow cells stay light),
  `StarwaterLayer` the stars, `LavaLayer` the lava; starwater and lava write the `emission` target
  that `IsoPost` blooms only while one is in view (`setEmission`). The liquid grids rebuild only when
  the renderer's cell revision changes. The test bed has a patch of each.
- The test bed's level is `apps/isorun/testbed/iso.des`. Its atlas (`apps/isorun/public/iso`) is
  packed with `node tools/build/pack-room.mjs <images dir> apps/isorun/testbed/iso.des
  apps/isorun/public/iso`; `apps/isorun/public/room/README.md` says how to fetch the images.
- The look ("DCSS HD-2D"): DCSS tiles upscaled 4x with generated normal maps (`enhance.ts`),
  lights and shadows, doors and stairs in 3D (`tile-features.ts`). The wall torch is
  `torch_mounted.glb` from KayKit Dungeon Remastered (CC0, Kay Lousberg), copied as it is into
  `apps/isorun/public/iso/models`.
- Actors (the player and monsters with an id on `ref`) persist between scenes and animate: a move
  hops, `act(id, 'attack' | 'hit' | 'death', target)` plays an action. An attack pulls back, then
  strikes; a hit or a kill lands when the strike reaches the target (`IMPACT_DELAY`), with blood
  that stays on the floor, sparks, a flash of light, a camera shake and a hit stop (`combat.ts`, the
  Combat menu section). `setDanger(0..1)` turns the screen edge softly red, for low hit points.
  Damage numbers (`damage-numbers.ts`, `.css`, the Interface setting) pop over the struck actor in
  `onShown`: a DOM layer over the canvas, placed by `screenOf` each frame only while one is up. The
  amount comes from the hit line's " for N", which only our engine writes
  (`engine/patches/11-damage-numbers.patch`); on a server's crawl a hit on the player takes the drop of
  the hit points instead, and a monster's hit shows crawl's "!"s.
  A hop raises dust when the actor lifts off and lands, and leaves footprints that fade
  (`steps.ts`, the Steps menu section).
- Graphics quality (`quality.ts`, `setQuality`, kept apart from the look): a limit on the pixel
  ratio, the ambient occlusion's resolution (0 turns it off), shadows on or off, and how many
  torches cast shadows, and whether the monsters cast torch shadows too (High only); presets Low,
  Medium (the default) and High. The test bed's Graphics box (`testbed/quality-panel.ts`,
  `localStorage` `isorun.quality`) sits under the Camera box. Shadow maps are drawn only when they
  go stale: the statics are built again, a light moves to another torch, a door swings or an actor
  that casts a shadow moves (not its breath; on Medium that is the sun's map alone). The sun
  follows the camera a whole cell at a time. A torch shadow is six renders, so one of them is drawn
  a frame, and a light that goes to another torch waits at its last one until its turn. The shadow
  cameras see only the casters (`SHADOW_LAYER`, `ACTOR_SHADOW_LAYER` in `shading.ts`), and the
  scene's matrices are updated once a frame, not again in each shadow render. A new tile is
  uploaded alone and copied into its atlas page on the GPU (`AtlasMaterials.upload`): a page is
  4096 texels square. The statics are built again only for a cell change the 3D view draws, not
  for its glyph or colour (`UNDRAWN_CELL_FIELDS`).
- Headless Chromium (no GPU) renders the look at about 2 frames per second, so screenshots
  show states, not animation timing.
- Headless Chromium falls back to WebGL2, which hides WebGPU limits. Test real WebGPU with
  `--enable-unsafe-webgpu --enable-features=Vulkan --use-vulkan=swiftshader
  --use-webgpu-adapter=swiftshader`. WebGPU allows 16 sampled textures per shader, and every
  shadow-casting light takes one.
- The desktop app (`desktop/`, not an npm workspace, so `npm install` there; README there):
  Electron, for a Steam Deck or a ROG Ally. `STEAM.md` (shown in the game at `/app/steam`, under Get the
  app) puts the AppImage in Steam; `tools/build/brand.mjs` writes its artwork to `public/steam/art`. `src/main.ts` takes every https request and answers the
  ones to `https://isorun.localhost` from `src/serve.ts` (the built app, `engine/dist` at `/engine/`, the
  gamedata and morgue proxies), so the page has a real https origin: Cache Storage and the service
  worker work as on the web. It loads `/?fullscreen`, so the Quit row shows. `npm run package`
  (`package.mjs`, electron-builder) builds the app with offline on and writes a Linux AppImage, a Linux tar.gz and a
  Windows zip to `desktop/dist/release`; the app is staged in the temp folder, since electron-builder takes in
  every `node_modules` above its project. Only the AppImage updates itself, from this repository's
  GitHub releases (electron-updater, `updateInBackground`): `npm run release` uploads a draft
  release `v<version>` (the version of `apps/isorun/package.json`), and publishing the draft
  ships it. The prebuilt Electron does not run on NixOS: test with the nixpkgs `electron`
  (`electron desktop`) under `xvfb-run`, with `--ozone-platform=x11` and `ELECTRON_RUN_AS_NODE`
  unset (this shell sets it). The AppImage runs with `--appimage-extract-and-run` and an
  `LD_LIBRARY_PATH` of the nixpkgs libraries Electron needs (nss, nspr, gtk3, alsa-lib, libgbm).
- Hosting: the `isorun` Worker (`wrangler.jsonc`, `apps/isorun/worker/index.ts`) at
  https://isorun.maciej-rosiek.workers.dev, deployed by `.github/workflows/isorun-cloudflare.yml` on
  a push to `main`. The offline engine's Worker is not deployed, so `/engine/*` is a 404 there and
  the build has `VITE_OFFLINE=off` (`offlineOffered`): no offline player, and no service worker (one
  installed before is unregistered). The home screen's Get the app (`appDownloadsOffered`, `showApp`) links
  the desktop app's packages of the latest release there.
  Visit counts: the page (`src/visits.ts`, not in dev) sends beacons to `/api/visit`
  (`src/visit-beacon.ts`): a visit, each game started, and a beat every 30 s while the tab is
  visible, with the seconds a game was on screen. The `VisitLog` Durable Object
  (`worker/visit-log.ts`) keeps one row per page load (a random id, no cookie, the country) and
  `/stats?key=<STATS_KEY>` shows who is on the page and in a game now, and visits, players and play
  time per day. `STATS_KEY` comes from the `ISORUN_STATS_KEY` repository secret.
- `npm run check` runs the typecheck and the tests. Two test files fail without the network and a
  built engine (`packages/offline/test/engine.test.ts` and one live test); they fail without them.
- Performance: `node tools/perf/explore.mjs` (set `CHROMIUM` to the Chromium binary; the dev server
  and a built engine must run) starts an offline game, explores with `o` and prints the frame times
  of each explore burst with the CPU 4 times slower (`--throttle`), as a weaker machine. `--profile`
  adds a CPU profile (Chrome DevTools Protocol `Profiler`) and prints the functions with the most
  time. A frame budget problem that the Graphics Quality setting does not change is CPU work: look
  for work done each step or each frame over the whole level (a full rebuild, a traversal), and key
  it by its own inputs. Found this way: the full level rebuild on each explore step (now chunks).
  `node tools/perf/play.mjs` plays as a player does (explore, fight, go down) and prints the slow
  frames and a profile of them; its header lists the options. Measure at the player's screen: a
  5120x1440 screen at 120 Hz has 8.3 ms a frame for 7.4 million pixels, which only
  `--width 5120 --height 1440 --dpr 1 --unlimited --slow 8.4` shows (at 1600x1000 and 60 Hz every
  frame fits). A slow frame whose time is in `writeBuffer`, `submit` or `createCommandEncoder`
  waits for the GPU. For the GPU cost of a setting, measure the test bed (`/testbed/iso.html`) at
  that size with `isorun.quality` and `isorun.lights.current` set by an init script. Found this
  way: the tilt-shift blur of the shaded image drew it whole once more (it now blurs the scene's
  colour and shades after) and took 21 taps a pass where 9 do (`BLUR_STEP`). Other sessions share
  this machine: check `/proc/loadavg` before a run, and check that the dev server serves the
  edited file (`curl` the module), as its watcher can miss edits.
  To count the objects three.js updates each frame, expose the `THREE.Scene` on `globalThis` for
  one run and walk it with `page.evaluate`.
  To check a walk, log the player's `Actor.moveTo` and each hop's start, length and lateness with
  `console.debug` for one run and read them with `page.on('console')`: a hop that starts in the past
  is a skip, a gap between hops a stop. Found this way: hops that started where the last one ended,
  however late the next step came (a walk that skipped more and more), and crawl's wait before the
  first step of a travel or explore (`engine/patches/10-travel-delay.patch`). A hop takes the time
  between the server's steps (`pace`), and fight sounds play when the renderer shows the blow
  (`IsoRender3d.act`'s `onShown`), not on a timer.
- Sound (`apps/isorun/src/audio/`): `SoundEngine` (`engine.ts`, one `audio` for the page, started by the
  first key or click; WebAudio, effects decoded once into voices with a pitch and volume spread and
  limits per sound and in total, the beds and music streamed from `<audio>` elements, buses under one
  master gain that follows the Audio settings, asleep while the tab is hidden). `catalog.ts` lists the
  sounds, beds and tracks; `cues.ts` has the pure mappings (message line to sound, combat event to
  cues, hearing by distance and screen side, mood and bed by place) and `director.ts` (`GameAudio`,
  one per `GameScreen`) feeds them from the message lines, `IsoRender3d.onHop` and `onDoor`, the level
  and the hit points. Files are in `public/audio` (sources in `ATTRIBUTION.md`). `?audio-trace` on the
  page keeps the names of the sounds played in `window.isorunAudioTrace`, and
  `node tools/perf/audio.mjs` plays a game and counts them.
