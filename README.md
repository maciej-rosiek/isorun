<p align="center"><img src="assets/logo.svg" alt="Isorun" width="560"></p>

# Isorun

**Dungeon Crawl Stone Soup drawn isometrically, played with a gamepad, keyboard or mouse.**

Isorun is an unofficial client for [Dungeon Crawl Stone
Soup](https://crawl.develz.org/). It talks to the public DCSS servers over
[WebTiles](https://crawl.develz.org/wordpress/howto), the same protocol the
official client uses, and replaces everything you see and touch: the DCSS
tiles are stood up in 3D and drawn from an isometric camera that turns, tilts
and zooms, with lights, shadows, fog of war, water, doors and stairs. It runs
in the browser, on your own WebTiles account (or offline, with no server).

Isorun is a fork of [Orbrun](https://github.com/Caeous/orbrun) by Caeous.

## Run it

```sh
npm install
npm run dev
```

Then open `http://localhost:5173/` for the client, or
`http://localhost:5173/testbed/iso.html` for the renderer test bed. Online play
works on the public WebTiles servers. Offline play needs the engine built, see
[engine/README.md](engine/README.md).

Other commands: `npm test` (vitest), `npm run typecheck` (`tsc -b`),
`npm run build` (`apps/isorun/dist`). Public servers don't send CORS headers,
so the dev and preview servers include a small proxy that fetches gamedata
(tile tables and atlases) from the connected server.

The desktop app (Linux and SteamOS, for a Steam Deck or a ROG Ally) is in
[desktop](desktop/README.md): the game and the offline engine in a self-updating AppImage, from
the [releases](https://github.com/maciej-rosiek/isorun/releases).

## Packages

Isorun is an npm workspace. The game lives in `apps/isorun`; what it knows
about DCSS lives in small packages under `packages/`, each with a README.

| Package | What it does |
|---------|--------------|
| [`@isorun/webtiles`](packages/webtiles) | The WebTiles protocol: keycodes and messages, a WebSocket connection, a reducer from server messages to a `GameState` |
| [`@isorun/gamedata`](packages/gamedata) | Loads a server's own `enums.js`, `tileinfo-*.js` and atlases at runtime |
| [`@isorun/scene`](packages/scene) | The world model a renderer draws: cells, billboards, camera, directions |
| [`@isorun/scene-webtiles`](packages/scene-webtiles) | `GameState` + gamedata to `Scene`: all the DCSS knowledge |
| [`@isorun/render-iso`](packages/render-iso) | The isometric three.js renderer of a `Scene` |
| [`@isorun/render-2d`](packages/render-2d) | Canvas 2D renderer of a `Scene`; draws the level map |
| [`@isorun/vault`](packages/vault) | A subset of DCSS's `.des` vault format, compiled to a `Scene` |
| [`@isorun/offline`](packages/offline) | Runs the WebAssembly engine in a Web Worker behind a WebTiles connection |

[CONTRIBUTING.md](CONTRIBUTING.md) is the short guide, [ABOUT.md](ABOUT.md)
the tour and the credits, [CHANGELOG.md](CHANGELOG.md) what's new.

## License

[AGPL-3.0-or-later](LICENSE). Isorun is an independent project, not affiliated
with or endorsed by the DCSS development team. [ATTRIBUTION.md](ATTRIBUTION.md)
explains how it relates to Orbrun and DCSS and lists the third-party credits.
