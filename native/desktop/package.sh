#!/bin/sh
# Packages the app for the desktop: the `isorun` binary with the
# offline engine beside it (engine/, as engine.rs finds it), into
# native/target/release-packages/.
#
#   native/desktop/package.sh linux      # Isorun-linux-x86_64.tar.gz and Isorun-x86_64.AppImage
#   native/desktop/package.sh windows    # Isorun-win-x64.zip (x86_64-pc-windows-gnu, mingw-w64)
#
# The engine comes from engine/native.sh (engine/dist/native/<target>/engine);
# build it first: `engine/native.sh stable` for linux, `engine/native.sh
# stable --target windows` for windows. ISORUN_ENGINE overrides where it is.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
NATIVE=$(cd "$HERE/.." && pwd)
ROOT=$(cd "$NATIVE/.." && pwd)
OUT=${OUT:-$NATIVE/target/release-packages}
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$NATIVE/Cargo.toml" | head -1)
ICON=$NATIVE/assets/icons/icon-512.png
mkdir -p "$OUT"

# The folder every package holds: the app, the engine, the licence.
stage() { # <dir> <binary> <engine dir>
    rm -rf "$1"
    mkdir -p "$1"
    cp "$2" "$1/"
    cp -R "$3" "$1/engine"
    cp "$ROOT/LICENSE" "$1/LICENSE.txt"
}

linux() {
    engine=${ISORUN_ENGINE:-$ROOT/engine/dist/native/host/engine}
    [ -f "$engine/engine.json" ] || { echo "no engine in $engine: run engine/native.sh stable" >&2; exit 1; }
    (cd "$NATIVE" && cargo build --release -p isorun-app --bin isorun)
    bin=$NATIVE/target/release/isorun
    strip "$bin" 2>/dev/null || true

    # the plain folder
    dir=$OUT/linux/Isorun
    stage "$dir" "$bin" "$engine"
    tar -C "$OUT/linux" -czf "$OUT/Isorun-linux-x86_64.tar.gz" Isorun

    # the AppImage: the same folder under usr/bin, so the engine is beside the binary
    app=$OUT/linux/Isorun.AppDir
    rm -rf "$app"
    mkdir -p "$app/usr/bin"
    cp "$bin" "$app/usr/bin/isorun"
    cp -R "$engine" "$app/usr/bin/engine"
    cp "$ICON" "$app/isorun.png"
    cat > "$app/isorun.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Isorun
Comment=An isometric client for Dungeon Crawl Stone Soup
Exec=isorun
Icon=isorun
Categories=Game;RolePlaying;
Terminal=false
X-AppImage-Version=$VERSION
EOF
    cat > "$app/AppRun" <<'EOF'
#!/bin/sh
HERE=$(dirname "$(readlink -f "$0")")
exec "$HERE/usr/bin/isorun" "$@"
EOF
    chmod +x "$app/AppRun"
    tool=$(appimagetool_path)
    # the update information lets AppImageUpdate (and Gear Lever, AppImageLauncher) update it
    # from this repository's latest release, by the .zsync written beside it
    (cd "$OUT" && ARCH=x86_64 "$tool" --appimage-extract-and-run --no-appstream \
        -u "gh-releases-zsync|maciej-rosiek|isorun|latest|Isorun-x86_64.AppImage.zsync" \
        "$app" "$OUT/Isorun-x86_64.AppImage")
    rm -rf "$OUT/linux"
    ls -la "$OUT"/Isorun-linux-x86_64.tar.gz "$OUT"/Isorun-x86_64.AppImage
}

# appimagetool, fetched once into the target dir (an AppImage itself, run extracted: no FUSE needed)
appimagetool_path() {
    tool=$NATIVE/target/tools/appimagetool-x86_64.AppImage
    if [ ! -x "$tool" ]; then
        mkdir -p "$(dirname "$tool")"
        curl -fsSL -o "$tool.part" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
        chmod +x "$tool.part"
        mv "$tool.part" "$tool"
    fi
    echo "$tool"
}

windows() {
    engine=${ISORUN_ENGINE:-$ROOT/engine/dist/native/windows/engine}
    [ -f "$engine/engine.json" ] || { echo "no engine in $engine: run engine/native.sh stable --target windows" >&2; exit 1; }
    (cd "$NATIVE" && cargo build --release -p isorun-app --bin isorun --target x86_64-pc-windows-gnu)
    bin=$NATIVE/target/x86_64-pc-windows-gnu/release/isorun.exe
    x86_64-w64-mingw32-strip "$bin" 2>/dev/null || true
    dir=$OUT/windows/Isorun
    stage "$dir" "$bin" "$engine"
    rm -f "$OUT/Isorun-win-x64.zip"
    (cd "$OUT/windows" && zip -qr9 "$OUT/Isorun-win-x64.zip" Isorun)
    rm -rf "$OUT/windows"
    ls -la "$OUT/Isorun-win-x64.zip"
}

[ $# -gt 0 ] || set -- linux
for t in "$@"; do
    case $t in
        linux) linux ;;
        windows) windows ;;
        *) echo "unknown package $t (linux, windows)" >&2; exit 1 ;;
    esac
done
