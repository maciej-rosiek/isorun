# Isorun native on the web

The native app (`native/crates/app`) built for wasm32: eframe's web runner on the page's canvas
(`index.html`), on WebGPU where the browser has it and WebGL2 otherwise. Offline play is the
WebAssembly crawl of `engine/dist`, run in a Web Worker by `packages/offline`'s worker:
`engine-bridge.ts` puts it on the page as `globalThis.isorunEngine`, which `client::engine`
(`web`) drives, with the saves in IndexedDB as the TypeScript build keeps them (the same save
folders, notes and rc files on one origin).

    native/web/build.sh [out dir]      # default native/web/dist; PROFILE=dev for a quick build
    node native/web/test.mjs [dir]     # headless Chromium (WebGL2): the page, ?testbed, the engine

The page looks for the engine at `/engine/` on its origin, or at `?engine=<url>`. The hosted page
serves the build at `/native/` (`.github/workflows/isorun-cloudflare.yml`); the hosted page has no
engine, so it offers no offline play there.
