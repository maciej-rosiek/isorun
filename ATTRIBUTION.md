# Attribution and licensing

Isorun is an independent, unofficial client for Dungeon Crawl Stone Soup
(DCSS). It speaks the WebTiles protocol to standard DCSS servers and renders
the dungeon isometrically. It is not affiliated with or endorsed by the DCSS
development team.

## Upstream: Orbrun

Isorun is a hard fork of [Orbrun](https://github.com/Caeous/orbrun) by Caeous
(AGPL-3.0-or-later). The client, its packages and its code are Orbrun's:
the WebTiles connection and reducer, the scene model, the HUD, the gamepad
system, the menus, the offline engine, the front room and the tools. Isorun
replaces Orbrun's first-person renderer with the isometric one
(`native/crates/render`) and takes the project in its own direction. Orbrun's
copyright notices stay with the code they cover.

Copyright (C) 2026 the Orbrun developer, and the Isorun developers.

Licensed under the GNU Affero General Public License, version 3 or (at your
option) any later version (AGPL-3.0-or-later). See [LICENSE](LICENSE) for the
full text.

## Relationship to DCSS

Isorun connects to standard DCSS WebTiles servers and speaks the same
WebSocket protocol as the official client. Online, gameplay runs entirely on
the server. The one exception is `engine/`, which builds upstream DCSS for
WebAssembly with a small patch set, for offline play; its patches stay under
DCSS's own GPLv2+ (see `engine/README.md`). Nothing
version-specific is shipped either — tile ids, enums, flag masks and atlases
are loaded at runtime from the connected server's own `/gamedata/<version>/`
files, exactly as the official client loads them.

DCSS is Copyright 1997–2025 Linley Henzell, the dev team, and contributors,
licensed under the GNU General Public License, version 2 or (at your option)
any later version. Isorun's AGPL-3.0-or-later license is compatible with this
through that "or later" option: the DCSS-derived material below is taken
forward to GPLv3 and combined into this AGPLv3 work as AGPLv3 section 13
permits.

## Interoperability constants

The items below carry numeric values that must match the DCSS server exactly,
or the client mis-sends keys and mis-renders cells. They are fixed by the DCSS
wire protocol and reproduced from the DCSS WebTiles client purely as a
protocol-interoperability requirement.

| File | Derived from (DCSS) | What |
|------|---------------------|------|
| `native/crates/webtiles/src/protocol.rs` | `key_conversion.js`, `cio.h` | Special keycodes sent to the server (arrows, F1–F19, keypad) |
| `native/crates/webtiles/src/protocol.rs` | `tileweb.h`, `map-feature.h`, `enums.js` | Mouse-mode and minimap feature-category values |
| `native/crates/input/src/keys.rs` | `key_conversion.js` | Browser key → DCSS keycode tables, keyed by `which` |
| `native/crates/scene-webtiles` | `enums.js` (`MF_*`, `HALO_*`, cell flag masks) | Fallback tables, cross-checked against the server's own `enums.js` at load and warned about on mismatch |
| `native/crates/webtiles/src/format.rs`, `native/crates/render2d` | The official WebTiles stylesheet | The 16-colour terminal palette (the standard IBM CGA/VGA set, not specific to DCSS) |
| `native/crates/render2d` | `minimap.js` — `init_options` | Default minimap colour palette, indexed by map-feature category |

## Tile art in the front room

The front end's room (`native/assets/room`) and the test bed
(`native/crates/client/assets`) are drawn from a handful of DCSS's own tiles,
packed by `tools/build/pack-room.mjs` from one pinned gamedata version (named in that
tool; `atlas.json` records it and the source of every tile). This is the one
place Isorun ships tile art of its own rather than loading the connected
server's: the room is a build asset the menus stand in before any server is
spoken to, and gameplay never draws from it. The mark is the Orb of
Zot's tile of that same version, cut from its `main.png` into
`native/assets/orb.png`: it stands beside the name on the title screen,
and `tools/build/brand.mjs` builds the favicon, the README's wordmark and the
app icons from it. DCSS's tile art (`rltiles`) is released under CC0 by
the DCSS team and its artists.

The name beside it is set in Grenze Gotisch Bold by Omnibus-Type, under the
SIL Open Font License 1.1 (`assets/fonts/OFL.txt`): the title screen loads
`native/crates/ui/assets/fonts/grenze-gotisch-700-latin.ttf`, and the brand tool
outlines the word from `assets/fonts/GrenzeGotisch-Bold.ttf` so the wordmark
and icons need no font installed to render.

## Derived from the DCSS WebTiles client

These files port portions of the official WebTiles client (first to
TypeScript in Orbrun and Isorun's web client, now to Rust),
following its structure, draw order, and key handling. The presentation
differs (isometric view, HUD, action bar); the behaviour being reproduced is the
official client's.

| File | Ported from (DCSS WebTiles) | What |
|------|-----------------------------|------|
| `native/crates/scene-webtiles` | `cell_renderer.js` — `draw_background`, `draw_foreground`, `draw_dolls` | Cell composition, cursor order, status-icon order and `status_shift`, player-doll/mcache layer composition |
| `native/crates/scene-webtiles` | `monster_list.js` — `monster_sort`, `is_excluded`, `group_monsters`, attitude/threat classes | Monster ordering, exclusion predicate, grouping, attitude and threat classification |
| `native/crates/scene/src/bars.rs` | `cell_renderer.js` — `draw_minibars`, `determine_colours` | Health/magic minibar geometry, colours, and the "don't draw when full" rule |
| `native/crates/scene` | `monster_list.js`, `map-feature.h` | Attitude and wound class names carried by the scene model |
| `native/crates/render2d` | `cell_renderer.js` — `render_cell`; `minimap.js` — `update` | Top-down cell draw order; minimap feature → colour lookup |
| `native/crates/overlays` | `menu.js` — `update_more`, `handle_size_change`, `update_visible_indices`, `scroll_to_item`, `snap_in_page`, `update_server_scroll`, `set_hovered`, `title_prompt`; `input.js` | Server menu layout, paging, hover, scroll reporting, title prompts, and key routing |
| `native/crates/overlays/src/menu_nav.rs` | `menu.js` — `menu_keydown_handler`, `menu_keypress_handler`, `item_selectable`, `next_hoverable_item`, `get_/set_relative_hover`, `line_up`, `line_down`, `snap_hover_in_page`, `raw_arrow_keys`, `menu_has_custom_dash`; `ui-layouts.js` — `scroller_handle_key` | Menu and popup key semantics |
| `native/crates/hud/src/grid/stats.rs` | `player.js` — `update_bar`, `update_bar_noise`, `update_stat`, `update_defense`, `percentage_color`, `stat_class`, `stat_boosters`, `wielded_weapon` | The stats pane: row order, captions, bar segments, and colour rules |
| `native/crates/hud/src/grid/console.rs` | `game.js` — `layout`, `handle_set_layout`, `toggle_full_window_dungeon_view`, `stat_width`, `show_diameter`; `messages.js` — `message_pane_height` | Console pane geometry and the level-map layout |
| `native/crates/hud/src/grid/messages.rs`, `native/crates/overlays` | `messages.js` | Message-pane scroll behaviour and the `--more--` prompt |
| `native/crates/hud/src/hud.rs` | `monster_list.js` — `update`; `cell_renderer.js` — `render_cell`, `draw_quantity`; `player.js`; `mouse_control.js` — `show_tooltip`; `game.js` | Monster list rows and limits, the player/monster cell portrait, item quantity marks, cell tooltips |
| `native/crates/hud/src/chat.rs` | `chat.js`, `client.html` — `receive_message`, `toggle`, `toggle_entire_chat`, `focus`, `chat_message_send`, history and linkify | The chat box: caption, history, send/close keys, link handling |
| `native/crates/app/src/game` | `game.js` — `set_ui_state`, `init_custom_text_colours`, `layout`; `mouse_control.js`; `dungeon_renderer.js` — `handle_mouse`; `ui.js`; `menu.js` | UI-state layers, rc colour overrides, `tile_web_mouse_control` semantics, tooltip and popup pointer rules |
| `native/crates/input/src/runner.rs` | `mouse_control.js`; `tileweb.cc`; `travel.cc` — `_adjacent_cmd` | Cell-click message shape and button numbering; what a click-travel does not act on |
| `native/crates/input/src/keys.rs` | `key_conversion.js`, `client.js` | Which keys the browser keeps (F11, F12) and which reach the server |

## Derived from the DCSS engine and server

| File | Derived from (DCSS) | What |
|------|---------------------|------|
| `native/crates/webtiles/src/state.rs` | `tileweb.cc` — `redraw`, `_send_cell`, `_send_item`, `push_ui_cutoff`; `messages.js`; `menu.js`; `game.js`; `ui.js`; `chat.js`; `travel.cc` | Reducer semantics: message ordering, the UI cutoff stack, item and cell field meanings, travel-trail rules |
| `native/crates/scene-webtiles` | `tileweb.cc` — `_send_cell` | Which fields a cell carries and what a missing field means |
| `native/crates/gamedata` | `tileweb.h`; the server's `enums.js`, `tileinfo-*.js` and `status-icon-sizes.js` | UI-state values; the runtime loader for the server's own gamedata |

## Independently implemented

The rest of the codebase is an independent implementation written against the
observed wire protocol and the behaviour of the official client: the WebSocket
connection layer, the tolerant reducer's own structure, the scene model
(`native/crates/scene`), the isometric renderer (`native/crates/render`) and every
rendering decision in it, the HUD layout, the gamepad system and action bar,
the on-screen keyboard, the command menus, the server picker and lobby,
settings, the front room and the `.des` subset compiler it is written in
(`native/crates/vault`, an independent parser of a documented format), the build
tools.

## PocketZot

[PocketZot](https://pocketzot.app/about) ([source](https://github.com/pocketzot/pocketzot))
is an unofficial DCSS client for phones, also AGPL-3.0-or-later. These
documents are based on PocketZot's, and parts of Orbrun's (and so Isorun's) design took
inspiration from it. Thanks to its developer for building it in the open and
for feedback on Orbrun.

The offline engine's patches and build files (`engine/`) are derived from
[pocketzot-engine](https://github.com/pocketzot/pocketzot-engine)'s
WebAssembly port of DCSS (GPLv2+), reworked into a patch series against
upstream and a JSPI-only build.

## Third-party assets and dependencies

- [egui](https://github.com/emilk/egui) and [wgpu](https://wgpu.rs) (MIT or Apache-2.0): the
  interface and the renderer; the Rust crates `native/Cargo.lock` lists, each under its own
  licence.
- KayKit Dungeon Remastered by Kay Lousberg (CC0): the wall torch model,
  `native/crates/client/assets/torch_mounted.glb` (its licence beside it).
- Grenze Gotisch by Omnibus-Type (SIL Open Font License 1.1,
  `assets/fonts/OFL.txt`): the title face, as described above.
- Cinzel by Natanael Gama, The Cinzel Project Authors (SIL Open Font License 1.1,
  `assets/fonts/Cinzel-OFL.txt`): the small capitals of the dock, its screens and tooltips,
  `native/crates/ui/assets/fonts/cinzel-latin.ttf` and `native/crates/hud/assets/fonts/cinzel-*.ttf`.
- Sounds, in `native/assets/audio/` (re-encoded to mono or stereo Ogg Vorbis, trimmed and
  normalised in loudness, otherwise as published). Everything below is CC0 (public domain):
  - Kenney, [Impact Sounds](https://kenney.nl/assets/impact-sounds) (CC0): `sfx/step1-5`
    (footstep_concrete), `sfx/hit1-4`, `sfx/hit-heavy1-2`, `sfx/hurt1-2`, `sfx/death1-2`,
    `sfx/cast-ice` (impactGlass_light), `sfx/gameover` (impactBell_heavy) and `sfx/stairs` (four
    footstep_wood sounds mixed in a row).
  - Kenney, [RPG Audio](https://kenney.nl/assets/rpg-audio) (CC0): `sfx/swing1-2` (knifeSlice),
    `sfx/door-open1-2`, `sfx/door-close1-2`, `sfx/pickup1-2` (cloth1, handleSmallLeather),
    `sfx/gold1-2` (handleCoins), `sfx/scroll1-2` (bookFlip).
  - Kenney, [Interface Sounds](https://kenney.nl/assets/interface-sounds) (CC0): `sfx/ui-click1-2`,
    `sfx/ui-open`, `sfx/ui-close`, `sfx/ui-back`, `sfx/more` (bong_001).
  - Made for Isorun with ffmpeg's signal generators (CC0): `sfx/miss1-3`, `sfx/heartbeat`,
    `sfx/levelup`, `sfx/cast-generic`, `sfx/cast-fire`, `sfx/cast-dark`, `sfx/potion`, and the
    loops `ambient/water` and `ambient/lava`.
  - [Dungeon Ambience](https://opengameart.org/content/dungeon-ambience) by yd (CC0):
    `ambient/dungeon` (the first two minutes).
  - [Dark Cavern Ambient](https://opengameart.org/content/dark-cavern-ambient) by Paul Wortmann
    (CC0): `ambient/cavern` (dark_cavern_ambient_001).
  - Music, all CC0 from OpenGameArt: [Into the Ruined Temple](https://opengameart.org/content/into-the-ruined-temple)
    by SterlingRay (`music/ruined-temple`), [Persistence](https://opengameart.org/content/persistence)
    by cinameng (`music/persistence`), [Cave Music](https://opengameart.org/content/cave-music) by
    hornpipe2 (`music/cave`), [Within The Abandoned Castle](https://opengameart.org/content/within-the-abandoned-castle)
    by Spring Spring (`music/abandoned-castle`, the first 200 seconds of V2),
    [Evil temple Ambiance loop](https://opengameart.org/content/evil-temple-ambiance-loop) by
    HaelDB (`music/evil-temple`) and [Eternal Sleep](https://opengameart.org/content/eternal-sleep)
    by The Oracle (`music/eternal-sleep`, the first 200 seconds).
- Tile art and atlases for gameplay are **not** distributed here. They are
  fetched at runtime from the connected DCSS server, which serves its own
  copies. DCSS tiles are by the DCSS contributors under the licenses stated
  in the DCSS source tree (`crawl-ref/source/rltiles/`); the few packed into
  the front room's atlas are CC0, as described above.
- Build and test tooling (TypeScript, Vite, Vitest, happy-dom, wrangler,
  opentype.js, ws) under their own licenses, none of it shipped to players.
