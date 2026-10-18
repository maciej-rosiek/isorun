# Building and deploying the engine

Nothing here is a copy of crawl, and no crawl version is pinned. There are two
channels, both resolved on upstream at build time: **trunk** is `master`, and
**stable** is the newest `stone_soup-*` release branch, as servers run it.
`build.sh` fetches the commit, applies `patches/`, adds `wasm/`, and builds;
each channel's `engine.json` records the commit it built. One patch series
serves both.

```sh
engine/build.sh              # stable and trunk
engine/build.sh stable       # one channel
engine/build.sh 0.34.1       # or any crawl tag, branch or commit
node engine/smoke.mjs        # play every built channel under node
```

Needs emsdk active (`em++` on PATH), node 25+ (JSPI), python3 with PyYAML,
and what a native `make WEBTILES=y` needs. Each channel builds in its own
worktree of one clone under `engine/.work/`, and lands in
`engine/dist/builds/<commit>/`, its gamedata in `engine/dist/gamedata/<commit>/`,
with `engine/dist/<channel>/engine.json` pointing at them (all gitignored).
Builds live under their commit so a device can download the next one beside
the one it plays (`@isorun/offline` `EngineStore`):

| File | What |
|---|---|
| `crawl.js`, `crawl.wasm` | The engine: an ES module factory and its wasm |
| `crawl.data` | `dat/` and `docs/`, preloaded read-only |
| `prewarm/` | The first-boot caches, baked at build time (`wasm/bake-caches.mjs`) |
| `../gamedata/<commit>/` | The atlas sheets and tile info a server serves as `/gamedata/<version>/`, beside the builds so one base URL serves every channel |
| `../<channel>/engine.json` | The channel, the crawl commit built, its version string, the cache stamp, its gamedata version, the recipe (`recipe.mjs`: a hash of `patches/`, `wasm/` and `build.sh`), and every file of the build and its gamedata with its size (`pointer.mjs`) |

A host gives each release, and trunk, its own save dir (`Module.saveDir`, e.g.
`/crawl-0.34`, `/crawl-trunk`): IDBFS names its IndexedDB database after the
mount point, and a save carried into a newer crawl never loads in an older one.

## How it differs from the server build

The binary normally sends JSON over a Unix datagram socket and takes keys from
a pty. Here it runs headless, and `wasm/bridge.h` + `wasm/pre.js` put a JS
queue where the socket was:

| | Server build | This build |
|---|---|---|
| Output | `sendto()` in `finish_message` | `Module.onBridgeOutput(lines)` |
| Control in | `recvfrom()` | `Module.bridge.pushControl(json)` |
| Typed keys | pty | `Module.bridge.pushKeys(text)`, sent as `text_input` |
| Waiting for input | `pselect()` | JSPI suspension: no CPU while the player thinks |
| Terminal | ncurses | `wasm/fake-curses.h`, inert |
| Saves | files | IDBFS at the save dir, flushed at each save commit |

A host must send `{"msg":"attach","primary":true}` after boot, as the server
does, or the engine never sends the map.

Beyond upstream's messages, it sends two: `checkpoint`, once a save commit has
reached IndexedDB, and `ending`, once a game's end has; and it takes one,
`checkpoint`, which saves at the next moment the player has control (for a
tab going to the background).

## Patches

One per crawl file, each saying what it does at the top, every change behind
`#ifdef __EMSCRIPTEN__` so the native build compiles upstream's code.
`tileweb.cc` carries the socket swap; the rest fix headless play for a
watching client (popups painted, pre-game messages sent), keep the caches
valid across boots, and make saves land in IndexedDB whole.

When upstream moves, `build.sh` names the patch that no longer applies.
Fix it against the new source, regenerate it with `git diff` from the
channel's worktree in `.work/<channel>`, and keep the header.

## Deploying

The builds are served by their own Worker, `isorun-engine` (`wrangler.jsonc`,
`worker/`), at https://isorun-engine.maciej-rosiek.workers.dev, which the
hosted page (https://isorun.maciej-rosiek.workers.dev) forwards `/engine/*`
to through its `ENGINE` service binding (the root `wrangler.jsonc`). The page
offers offline games where `/engine/stable/engine.json` answers with JSON and
the browser has JSPI (`native/web/engine-bridge.ts`).

`.github/workflows/isorun-cloudflare.yml` deploys it, before the site, on
every push to `main` and nightly. Its `engine` job builds the stable channel
on GitHub (`build.sh stable`, about eight minutes with its tools: emsdk at `emsdk-version`,
node at `.node-version`, PyYAML, libpng and zlib for tilegen), cached by
crawl's stable commit, the recipe (`recipe.mjs`) and the emsdk version, so a
run builds only when one of them moved: a new stable commit upstream, which
the nightly run picks up, or a change to `patches/`, `wasm/` or `build.sh`.
A new build is played under node (`smoke.mjs`) before it is published; a
failed build or smoke publishes nothing, so players keep the build they have,
and the site still deploys on it. Then `publish.mjs` deploys it, and the job
checks that the Worker serves the new `stable/engine.json`. The same
`CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` secrets as the site's.

By hand:

```sh
engine/build.sh stable && node engine/publish.mjs
```

A deploy replaces every file, so `publish.mjs` stages what was built here
and copies any channel that was not from the live Worker (trunk, if it was
ever published), into `engine/.publish`, which is what Wrangler uploads, and
checks Cloudflare's limits first: every file under 25 MiB (the largest,
`crawl.data` and `crawl.wasm`, are about 12 MB) and at most 20,000 files a
version (a channel is about 50). Only files whose content Cloudflare does not
have go up. Never run a bare `wrangler deploy` here.

### What a browser needs

- **JSPI** (`WebAssembly.Suspending`): the engine waits for the player in a
  JSPI suspension. Chrome and Edge 137 and newer have it. Where it is missing
  (Safari, Firefox without it), the page does not offer offline games, and
  watching and playing on servers work as anywhere.
- **No cross-origin isolation**: the engine has no threads, so it needs no
  SharedArrayBuffer and no COOP/COEP headers.
- **Headers** (`worker/index.ts`): `crawl.wasm` as `application/wasm` (for
  streaming compilation); `crawl.data` and `prewarm.bin` as
  `application/x-protobuf`, which Cloudflare compresses (about 5 MB on the
  wire instead of 24); a build's and a commit's gamedata files kept for good,
  a channel's `engine.json` asked again each time.

Locally, each Worker under its own `wrangler dev`: the site's binding finds
the engine's through Wrangler's dev registry. (One `wrangler dev` with both
configs answers every request through the binding with an empty 500 when
both Workers have static assets, as Wrangler 4.129 does.) With a proxy in the
environment, `NO_PROXY=127.0.0.1,localhost`.

```sh
ENGINE_LIVE=<a base that 404s> node engine/publish.mjs --dry-run   # stages engine/.publish
npx wrangler dev -c engine/wrangler.jsonc --port 8788 &
npx wrangler dev --port 8787 &                                      # the site, with native/web/dist
BASE=http://127.0.0.1:8787 node native/web/test.mjs                 # the page, and an offline game through the binding
```
