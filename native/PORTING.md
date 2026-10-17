# Isorun in Rust: the crates and how to work on them

Isorun is this folder: one code base for the desktop (Linux, Windows), Android and the web
(wasm), on egui and wgpu. It was ported from the TypeScript client (`apps/isorun`, `packages/*`),
which looked the same and ran slower; that client is gone now, and git history before the commit
"the TypeScript client goes" has it, for the reasoning behind a rule. The "from" column names
what each crate was ported from. This file is the shared brief for whoever works on a part.

## Crates

| crate | what | from |
|---|---|---|
| `scene`, `webtiles`, `gamedata`, `scene-webtiles`, `vault` | data layers (done) | `packages/*` |
| `render` | the isometric renderer `IsoRenderer` (done) | `packages/render-iso` |
| `client` | no UI: platform (storage, HTTP, sockets), `servers.rs`, `session.rs`, the offline engine (`engine.rs`) | `servers.ts`, `session.ts`, `packages/offline` |
| `ui` | egui toolkit: fonts, `theme` (the CSS colours), `text::formatted` (crawl's `<red>` text), `tiles::TileIcons` (crawl tiles as egui images; `shared_atlas` uploads each atlas once for it and the 2D maps), `view3d::View3d` (the iso view as a widget), `keys` (basic keys to crawl), `harness` | — |
| `front` | the front end: the title screen over the turning room, accounts and servers, the lobby, the settings, Get the app | `main.ts`, `menu.ts`, `room/*` |
| `hud` | the dock, its screens (inventory, character), the classic HUD's panes, chat, tooltips | `hud.ts`, `dock.ts`, `screens.ts`, `grid/*` |
| `overlays` | crawl's screens over the game: menus, popups, describe, character creation, prompts, the level-up banner | `overlays.ts`, `menu-nav.ts` |
| `input` | keys, pad and touch to crawl's messages: bindings, the runner, the pad, gestures, the on-screen keyboard | `keys.ts`, `bindings.ts`, `runner.ts`, `gamepad.ts`, `touch.ts` |
| `audio` | the sound: effects, beds and music, and the cues from the game | `audio/*` |
| `render2d` | crawl's 2D tile view: the level map (X) and the minimap | `packages/render-2d` |
| `app` | the game: the game screen, the platforms' entry points (`main.rs`, `lib.rs`) | `main.ts`, `game.ts` |

New parts go in **their own crate** under `crates/` (so a half-done part never breaks another's
build), with a `README.md` like every package here. Keep its `Cargo.toml` valid at all times:
the workspace takes every `crates/*`, and one broken manifest stops every build.

## Build and run

    cd native
    cargo build -p <crate>                      # incremental; the workspace shares one target dir
    cargo test -p <crate>

Builds of different crates wait for each other on the target dir's lock; that is normal. Never
`cargo clean`.

The offline engine: crawl built natively in `engine/.work/stable/crawl-ref/source/crawl` (found
by `client::engine` with no setup). A game runs crawl as a child process.

### See it

The harness (`ui::harness::run`) opens a window that plays an offline game on this device and
calls your closure each frame to draw over the view; keys you do not take go to crawl. Write an
example in your crate (`examples/<name>.rs`) that shows your part there.

There is no screen here: run under Xvfb with Mesa's software Vulkan, on your own display number:

    Xvfb :<N> -screen 0 1600x1000x24 &
    export DISPLAY=:<N> VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
    export ISORUN_DATA_DIR=/tmp/claude-0/<you>/data ISORUN_PROFILE=<you>
    ./target/debug/examples/<name> &          # note its PID; stop it with `kill <PID>`, never pkill
    xdotool search --name '<window title>'    # the window id
    xdotool windowfocus --sync <id>; xdotool key a; xdotool key Escape; xdotool type 'abc'
    xdotool mousemove 800 500 click 1
    import -window root /tmp/claude-0/<you>/shot.png

Keys reach the app only with `windowfocus` first and `xdotool key` without `--window`. Software
rendering is slow (a few frames a second): wait a few seconds after each key. A new profile
starts at crawl's character choice (species, background: press `a` `a` or `b a c`).

### The look

The look is the TypeScript client's, carried over: the layout, the sizes at 1600x1000 and at a
phone's size, the fonts (DejaVu Sans Mono, Cinzel for small capitals, Grenze Gotisch for titles:
`ui::fonts`), the colours (`ui::theme`, from the old `styles.css :root`), borders, shadows and
gradients (egui paints gradients with a `Mesh` of vertex colours), and the behaviour (keys,
mouse, pad, touch). Screenshots of the web build in headless Chromium (`native/web/test.mjs`,
`native/tools/shot.mjs`) and of the desktop under Xvfb compare with `compare` from ImageMagick
or PIL. To read the old `.ts` and `.css` a rule came from: `git show <commit>^:apps/isorun/src/<file>`.

## Rules

- Every rule has a reason (its comment says which): keep the reasoning in the comments, in the
  same plain style, when you change one.
- Touch only your own crate (and the files the brief names as yours). If you need something from
  another part, write it in yours behind a small interface and say so in your report.
- Commit your own paths only: `git add <your paths> && git commit -m "..." -- <your
  paths>` (another agent's commit may hold the index for a moment: retry on `index.lock`). End the
  message with the lines in `CLAUDE.md`'s session attribution if you were given them.
- Stop only processes you started, by PID.
- Tests: pure logic gets unit tests (`cargo test -p <crate>`); fixtures are in `native/fixtures`.
