# Isorun native on the web

The native app (`native/crates/app`) built for wasm32: eframe's web runner on the page's canvas
(`index.html`), on WebGPU where the browser has it and WebGL2 otherwise. Offline play is the
WebAssembly crawl of `engine/dist`, run in a Web Worker by `packages/offline`'s worker:
`engine-bridge.ts` puts it on the page as `globalThis.isorunEngine`, which `client::engine`
(`web`) drives, with the saves in IndexedDB as the TypeScript build keeps them (the same save
folders, notes and rc files on one origin).

    native/web/build.sh [out dir]      # default native/web/dist; PROFILE=release or dev for quicker builds
    node native/web/test.mjs [dir]     # headless Chromium: the page, ?testbed on WebGL2 and WebGPU, the engine

The page looks for the engine at `/engine/` on its origin, or at `?engine=<url>`, and for the
sounds at `/audio/` (the TypeScript build's `public/audio`). The hosted page serves the build at
`/native/` (`.github/workflows/isorun-cloudflare.yml`); the hosted page has no engine, so it
offers no offline play there.

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
