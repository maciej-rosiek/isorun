#!/bin/sh
# Builds the native app for the web: the app crate for wasm32 with wasm-bindgen's web target, the
# page (index.html) and the offline engine's bridge (engine-bridge.ts and packages/offline's
# worker, bundled with esbuild), into native/web/dist (or the folder given).
#
#   native/web/build.sh [out dir]        # e.g. apps/isorun/dist/native, served at /native/
#
# Needs the wasm32-unknown-unknown target, wasm-bindgen-cli of the version Cargo.lock has
# (0.2.x), and the repository's npm install (esbuild). PROFILE=web (the default: release with
# fat LTO, one codegen unit and no symbols, Cargo.toml) or release or dev. wasm-opt, where it
# is on PATH, runs over the wasm; it gains little once the server compresses it.
#
# The app's module goes into pkg-<hash of the wasm>/, which index.html and tile-worker.js are
# pointed at, so the hosted page can let browsers keep it for good (public/_headers): a new
# build is a new folder.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
NATIVE=$(cd "$HERE/.." && pwd)
ROOT=$(cd "$NATIVE/.." && pwd)
OUT=${1:-$HERE/dist}
PROFILE=${PROFILE:-web}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)

case "$PROFILE" in
    dev) flag=; dir=debug ;;
    release) flag=--release; dir=release ;;
    *) flag="--profile $PROFILE"; dir=$PROFILE ;;
esac
# getrandom's web backend, for what pulls it in (uuid, rand)
(cd "$NATIVE" && RUSTFLAGS="${RUSTFLAGS:-} --cfg getrandom_backend=\"wasm_js\"" cargo build $flag -p isorun-app --lib --target wasm32-unknown-unknown)
rm -rf "$OUT"/pkg "$OUT"/pkg-*
wasm-bindgen --target web --no-typescript --out-dir "$OUT/pkg" "$NATIVE/target/wasm32-unknown-unknown/$dir/isorun_app.wasm"
if command -v wasm-opt >/dev/null && [ "$PROFILE" != dev ]; then
    wasm-opt -O2 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue -o "$OUT/pkg/isorun_app_bg.wasm" "$OUT/pkg/isorun_app_bg.wasm"
fi
hash=$(cat "$OUT/pkg/isorun_app_bg.wasm" "$OUT/pkg/isorun_app.js" | sha256sum | cut -c1-12)
mv "$OUT/pkg" "$OUT/pkg-$hash"

esbuild=$ROOT/node_modules/.bin/esbuild
"$esbuild" "$HERE/engine-bridge.ts" --bundle --format=esm --target=es2022 --outfile="$OUT/engine-bridge.js" --log-level=warning
"$esbuild" "$ROOT/packages/offline/src/worker.ts" --bundle --format=esm --target=es2022 --outfile="$OUT/engine-worker.js" --log-level=warning
for f in index.html tile-worker.js; do sed "s|\./pkg/|./pkg-$hash/|g" "$HERE/$f" > "$OUT/$f"; done
cp "$HERE/tile-workers.js" "$OUT/"
du -sh "$OUT/pkg-$hash/isorun_app_bg.wasm"
echo "web build -> $OUT (pkg-$hash)"
