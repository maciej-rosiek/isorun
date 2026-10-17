#!/bin/sh
# Builds the native app for the web: the app crate for wasm32 with wasm-bindgen's web target, the
# page (index.html) and the offline engine's bridge (engine-bridge.ts and packages/offline's
# worker, bundled with esbuild), into native/web/dist (or the folder given).
#
#   native/web/build.sh [out dir]        # e.g. apps/isorun/dist/native, served at /native/
#
# Needs the wasm32-unknown-unknown target, wasm-bindgen-cli of the version Cargo.lock has
# (0.2.x), and the repository's npm install (esbuild). PROFILE=dev builds unoptimised.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
NATIVE=$(cd "$HERE/.." && pwd)
ROOT=$(cd "$NATIVE/.." && pwd)
OUT=${1:-$HERE/dist}
PROFILE=${PROFILE:-release}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)

if [ "$PROFILE" = release ]; then flag=--release; dir=release; else flag=; dir=debug; fi
# getrandom's web backend, for what pulls it in (uuid, rand)
(cd "$NATIVE" && RUSTFLAGS="${RUSTFLAGS:-} --cfg getrandom_backend=\"wasm_js\"" cargo build $flag -p isorun-app --lib --target wasm32-unknown-unknown)
rm -rf "$OUT/pkg"
wasm-bindgen --target web --no-typescript --out-dir "$OUT/pkg" "$NATIVE/target/wasm32-unknown-unknown/$dir/isorun_app.wasm"
if command -v wasm-opt >/dev/null && [ "$PROFILE" = release ]; then
    wasm-opt -O2 --enable-bulk-memory --enable-nontrapping-float-to-int -o "$OUT/pkg/isorun_app_bg.wasm" "$OUT/pkg/isorun_app_bg.wasm"
fi

esbuild=$ROOT/node_modules/.bin/esbuild
"$esbuild" "$HERE/engine-bridge.ts" --bundle --format=esm --target=es2022 --outfile="$OUT/engine-bridge.js" --log-level=warning
"$esbuild" "$ROOT/packages/offline/src/worker.ts" --bundle --format=esm --target=es2022 --outfile="$OUT/engine-worker.js" --log-level=warning
cp "$HERE/index.html" "$OUT/index.html"
du -sh "$OUT/pkg/isorun_app_bg.wasm"
echo "web build -> $OUT"
