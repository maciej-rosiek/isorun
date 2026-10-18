# Isorun on the web

The app (`native/crates/app`) built for wasm32: eframe's web runner on the page's canvas
(`index.html`), on WebGPU where the browser has it and WebGL2 otherwise. Offline play is the
WebAssembly crawl of `engine/dist`, run in a Web Worker (`engine/worker.ts`, `engine/engine.ts`):
`engine-bridge.ts` puts it on the page as `globalThis.isorunEngine`, which `client::engine`
(`web`) drives, with the saves in IndexedDB as the TypeScript client kept them (the same save
folders, notes and rc files on one origin, so its saves play on).

    native/web/build.sh [out dir]      # default native/web/dist (the whole site); PROFILE=release or dev for quicker builds
    node native/web/test.mjs [dir]     # headless Chromium: the page, ?testbed on WebGL2 and WebGPU, the engine
    BASE=<url> node native/web/test.mjs  # the same against a page served elsewhere (wrangler dev)

The build is the whole site: the page, the app's module, the bridge, and the files the page
fetches by path (the sounds at `/audio/`, the icons, the favicon and the Steam art that
`STEAM.md` links, copied from `native/assets`), with `_headers` (the cache rules) and `sw.js`
(which removes the TypeScript client's service worker from a browser that still has it). The
page looks for the engine at `/engine/` on its origin, or at `?engine=<url>`.

The hosted page (https://isorun.maciej-rosiek.workers.dev) serves it at its root, through the
Worker in `worker/` (`wrangler.jsonc`, deployed by `.github/workflows/isorun-cloudflare.yml`); its
old place, `/native/`, redirects to `/`. Its `/engine/` is the engine's own Worker
(`engine/wrangler.jsonc`, `engine/DEPLOYMENT.md`) through a service binding, deployed by the same
workflow. The page offers offline games where `stable/engine.json` under the engine base answers
with JSON and the browser has JSPI (`WebAssembly.Suspending`: Chrome and Edge 137+); elsewhere
`isorunEngine.channel()` is null and Offline is not offered. The engine needs no cross-origin
isolation (no threads). The page sends the visit counts of `/stats` (`client::visits`) to its own
origin.

## What the page does that the desktop does not

- **Threads.** wasm32 has none here, so the tiles are upscaled on Web Workers that load the
  app's own module (`tile-workers.js`, `tile-worker.js`; the app's `enhance_job` export,
  `isorun_render::remote` for the bytes). The module is compiled once on the page and posted to
  them. Without workers the renderer enhances a few tiles each frame itself.
- **Sound** starts with the first key, click or touch (`app.rs`), as the browsers want; the
  beds and the music stream from `<audio>` elements.
- **Full screen** is the page's (`ui::fullscreen`): eframe's web runner ignores the viewport's
  command.
- **Text input** and IME go through eframe's hidden text field; the clipboard through the
  page's copy and paste events.

## The build

`PROFILE=web` (the default, `native/Cargo.toml`) is release with fat LTO, one codegen unit and
no symbols: 19 MB of wasm, 6.9 MB gzipped, 5.0 MB with brotli (the servers compress it). About
4 MB of it is data: the room's places (the JSON and PNGs) and the fonts. wasm-opt, where it is
on `PATH`, runs over it but gains little once compressed. The module goes into
`pkg-<hash>/`, which `index.html` and `tile-worker.js` are pointed at, so `public/_headers` lets
browsers keep it for good. A panic shows its message on the console.

## Testing in headless Chromium

- WebGL2: `--use-angle=swiftshader --enable-unsafe-swiftshader`.
- WebGPU: `--enable-unsafe-webgpu --enable-features=Vulkan --use-vulkan=swiftshader
  --use-webgpu-adapter=swiftshader` **and `--use-angle=swiftshader`**: without the last the
  canvas has no shared image to present to, the GPU process dies and the page stays white
  ("A valid external Instance reference no longer exists").
- `?testbed&debug-view=N` shows one of the screen effects alone (post.wgsl: 1 the glow, 2 the
  occlusion, 3 the normals, 4 the blur).
- A phone: `hasTouch` and `isMobile` work; a `deviceScaleFactor` above 1 does not, as
  Chromium's emulation reports the canvas's device-pixel box in CSS pixels and eframe sizes the
  canvas by it (a real phone reports it right).
- SwiftShader draws a 1280x800 frame of the test bed in about a second, so screenshots show
  states, and a step needs a few seconds.
