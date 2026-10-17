#!/bin/sh
# The Android launcher icons and splash, from the brand (tools/build/brand.mjs
# writes the sources). The adaptive icon's foreground is the Orb of Zot's tile
# scaled without smoothing into the middle 66 of its 108 dp, where every
# launcher's mask leaves it whole; the background is the web icon's dark.
#
#   sh mobile/icons.sh        (needs ImageMagick's convert)
set -eu
MOBILE=$(cd "$(dirname "$0")" && pwd)
PUBLIC=$MOBILE/../apps/isorun/public
RES=$MOBILE/android/app/src/main/res

for d in mdpi:1 hdpi:1.5 xhdpi:2 xxhdpi:3 xxxhdpi:4; do
    name=${d%%:*}
    scale=${d#*:}
    fg=$(awk "BEGIN { print int(108 * $scale) }")
    orb=$(awk "BEGIN { print int(60 * $scale) }")
    legacy=$(awk "BEGIN { print int(48 * $scale) }")
    convert "$PUBLIC/orb.png" -filter point -resize "${orb}x${orb}" -background none -gravity center -extent "${fg}x${fg}" "$RES/mipmap-$name/ic_launcher_foreground.png"
    convert "$PUBLIC/icons/icon-512.png" -resize "${legacy}x${legacy}" "$RES/mipmap-$name/ic_launcher.png"
    convert "$PUBLIC/icons/icon-512.png" -resize "${legacy}x${legacy}" \( +clone -alpha extract -fill black -colorize 100 -fill white -draw "circle $((legacy / 2)),$((legacy / 2)) $((legacy / 2)),0" \) -alpha off -compose copy-opacity -composite "$RES/mipmap-$name/ic_launcher_round.png"
done
