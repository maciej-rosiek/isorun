#!/bin/sh
# Builds crawl's webtiles binary natively, as the native app (native/) runs it:
# a child process that speaks crawl's WebTiles socket. The tree is prepared as
# build.sh prepares it (upstream crawl, patches/ applied), then a plain
# `make WEBTILES=y`, for this machine or cross-compiled for another.
#
#   engine/native.sh [stable|trunk|<ref>] [--target <target>] [--out <dir>]
#
# Targets:
#   host              this machine (the default)
#   windows           x86_64-w64-mingw32 (mingw-w64's g++ on PATH)
#   android-aarch64   Android arm64-v8a (ANDROID_NDK_HOME: an NDK, r26+)
#   android-x86_64    Android x86_64, for the emulator
#
# The host build is made in engine/.work/<channel>, the tree build.sh uses, so
# a checkout's app finds it there with no setup; a cross build in
# engine/.work/<channel>-<target>, its own tree.
#
# The result is staged as an engine folder the app reads (engine.rs
# read_install), by default engine/dist/native/<target>/engine:
#   engine.json            {"version", "commit", "gamedata"}
#   crawl (crawl.exe)      the binary
#   dat/ docs/ settings/   what crawl reads at run time
#   gamedata/<commit>/     the client's files (webserver/game_data/static)
#
# Needs what a native `make WEBTILES=y` needs (a C++ compiler, python3 with
# PyYAML, perl, pkg-config with libpng and zlib for tilegen), and the
# target's compiler for a cross build. JOBS sets make's -j (default 2).
set -eu

ENGINE=$(cd "$(dirname "$0")" && pwd)
WORK=${ENGINE_WORK:-$ENGINE/.work}
REPO=$WORK/repo
JOBS=${JOBS:-2}

channel=stable
target=host
out=
while [ $# -gt 0 ]; do
    case $1 in
        --target) target=$2; shift 2 ;;
        --out) out=$2; shift 2 ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        *) channel=$1; shift ;;
    esac
done
out=${out:-$ENGINE/dist/native/$target/engine}

python3 -c 'import yaml' 2>/dev/null || { echo "python3 needs PyYAML (pip install pyyaml)" >&2; exit 1; }

# What make is given for the target. A cross build names the target with
# CROSSHOST (crawl's Makefile then builds lua, sqlite and zlib from contrib)
# and builds its host tools (tilegen) with the host's compiler.
exe=crawl
strip=strip
set --
case $target in
    host) ;;
    windows)
        cross=x86_64-w64-mingw32
        command -v $cross-g++ >/dev/null || { echo "$cross-g++ not found: install mingw-w64" >&2; exit 1; }
        # the posix thread model, for std::thread and std::mutex
        gxx=$cross-g++; gcc=$cross-gcc
        command -v $cross-g++-posix >/dev/null && { gxx=$cross-g++-posix; gcc=$cross-gcc-posix; }
        set -- CROSSHOST=$cross GXX=$gxx GCC=$gcc "EXTERNAL_LDFLAGS=-static -static-libgcc -static-libstdc++"
        exe=crawl.exe
        strip=$cross-strip
        ;;
    android-aarch64|android-x86_64)
        [ -n "${ANDROID_NDK_HOME:-}" ] || { echo "set ANDROID_NDK_HOME to an NDK" >&2; exit 1; }
        arch=${target#android-}
        api=${ANDROID_API:-26}
        bin=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
        cross=$arch-linux-android
        # crawl's own Android port (ANDROID=) is the SDL app; this is the
        # plain Unix build with the NDK's clang, a console program run by the
        # app. bionic has no nl_langinfo before API 26 and no
        # pthread_cancel; crawl uses neither in a webtiles build.
        set -- CROSSHOST=$cross "GXX=$bin/$cross$api-clang++" "GCC=$bin/$cross$api-clang" \
            "AR=$bin/llvm-ar" "RANLIB=$bin/llvm-ranlib" "STRIP=$bin/llvm-strip" \
            "EXTERNAL_LDFLAGS=-static-libstdc++"
        strip=$bin/llvm-strip
        ;;
    *) echo "unknown target $target" >&2; exit 1 ;;
esac

# One blobless clone for every channel, as build.sh has it.
mkdir -p "$WORK"
if [ ! -d "$REPO/.git" ]; then
    git clone --filter=blob:none --no-checkout https://github.com/crawl/crawl.git "$REPO"
fi
git -C "$REPO" fetch --quiet --filter=blob:none --tags --prune origin \
    '+refs/heads/*:refs/remotes/origin/*' || echo "fetch failed; building what the clone has" >&2

resolve() {
    case $1 in
        trunk) ref=origin/master ;;
        stable) ref=origin/$(git -C "$REPO" for-each-ref --format='%(refname:lstrip=3)' \
                    'refs/remotes/origin/stone_soup-*' | sort -V | tail -1) ;;
        *) if git -C "$REPO" rev-parse --verify --quiet "origin/$1^{commit}" >/dev/null; then
               ref=origin/$1
           else
               ref=$1
           fi ;;
    esac
    git -C "$REPO" rev-parse "$ref^{commit}"
}

commit=$(resolve "$channel")
slug=$(printf '%s' "$channel" | tr -c 'A-Za-z0-9._-' '-')
[ "$target" = host ] || slug=$slug-$target
src=$WORK/$slug

if [ ! -e "$src/.git" ]; then
    git -C "$REPO" worktree add --quiet --detach "$src" "$commit"
fi
if [ "$(cat "$WORK/$slug.commit" 2>/dev/null)" != "$commit" ]; then
    git -C "$src" checkout --quiet --force --detach "$commit"
    git -C "$src" clean -fdxq
    git -C "$src" submodule update --quiet --init --recursive --depth 1
    echo "$commit" > "$WORK/$slug.commit"
else
    # only the patched files go back; one that comes out of the patches the
    # same keeps its time, so make rebuilds no more than the patches changed
    changed=$(git -C "$src" diff --name-only)
    keep=$(mktemp -d)
    for f in $changed; do
        mkdir -p "$keep/$(dirname "$f")"
        cp -p "$src/$f" "$keep/$f"
    done
    [ -z "$changed" ] || git -C "$src" checkout --quiet -- $changed
fi
for p in "$ENGINE"/patches/*.patch; do
    git -C "$src" apply --whitespace=nowarn "$p" || {
        echo "$(basename "$p") no longer applies to crawl $channel ($commit)" >&2
        exit 1
    }
done
for f in ${changed:-}; do
    if cmp -s "$keep/$f" "$src/$f"; then touch -r "$keep/$f" "$src/$f"; fi
done
[ -z "${keep:-}" ] || rm -rf "$keep"

cd "$src/crawl-ref/source"
# status-icon-sizes first, by hand, as build.sh does: upstream's rule names
# `$(RLTILES)status-icon-sizes.h` (no slash), so under -j a copy can look for
# it before anything wrote it
python3 util/status-icon-sizes-gen.py rltiles/icon-sizes.txt
make -j"$JOBS" WEBTILES=y "$@"

version=$(git describe --tags --long "$commit" 2>/dev/null || echo "$commit")
rm -rf "$out"
mkdir -p "$out/gamedata"
cp "$exe" "$out/$exe"
"$strip" "$out/$exe" 2>/dev/null || true
cp -R dat "$out/dat"
cp -R ../docs "$out/docs"
cp -R ../settings "$out/settings"
cp -R webserver/game_data/static "$out/gamedata/$commit"
# the tiles are the client's (gamedata), as the wasm build leaves them out;
# the sources of the generated data are not read at run time
rm -rf "$out/dat/tiles"
find "$out/dat" \( -name '*.txt.in' -o -name '*.pl' -o -name '*.py' \) -delete
printf '{"version":"%s","commit":"%s","gamedata":"%s","target":"%s"}\n' \
    "$version" "$commit" "$commit" "$target" > "$out/engine.json"
echo "engine $channel $version ($target) -> $out"
