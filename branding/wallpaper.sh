#!/usr/bin/env bash
# Renders Iskra's default wallpaper (DESIGN.md): navy #0d1326 -> indigo #1d2b57 -> plum #3a1f4d,
# soft glows (coral top right, blue bottom left, violet centre) and faint flowing lines.
# Usage: wallpaper.sh WIDTH HEIGHT OUTPUT   (ImageMagick 7)
set -euo pipefail
W=$1 H=$2 OUT=$3
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
glow() { # colour cx cy radius opacity
  magick -size "${W}x${H}" xc:none -fill "$1" -draw "circle $2,$3 $(( $2 + $4 )),$3" \
    -blur 0x$(( $4 * 7 / 10 )) -channel A -evaluate multiply "$5" +channel "$T/g$RANDOM.png"
}
magick -size "${H}x${W}" gradient:'#0d1326-#1d2b57' -rotate -90 "$T/a.png"
magick -size "${W}x${H}" radial-gradient:'#3a1f4d-none' -resize 160% -gravity southeast -crop "${W}x${H}+0+0" +repage "$T/b.png"
glow '#ff7a6b' $((W * 86 / 100)) $((H * 8 / 100)) $((H * 34 / 100)) 0.55
glow '#3d7bf0' $((W * 10 / 100)) $((H * 92 / 100)) $((H * 38 / 100)) 0.50
glow '#8b5cf6' $((W * 50 / 100)) $((H * 52 / 100)) $((H * 30 / 100)) 0.35
# Flowing lines: a few sine waves, thin and faint.
lines=""
for i in 0 1 2 3 4; do
  pts=""
  for x in $(seq 0 40 $W); do
    y=$(awk -v x=$x -v i=$i -v W=$W -v H=$H 'BEGIN{printf "%d", H*(0.55+0.05*i) + H*0.09*sin(x/W*6.28*0.8 + i*0.7) - x*H*0.00012}')
    pts+="$x,$y "
  done
  lines+="polyline $pts "
done
magick -size "${W}x${H}" xc:none -fill none -stroke 'rgba(200,210,255,0.10)' -strokewidth 1.5 \
  -draw "$lines" -blur 0x0.6 "$T/lines.png"
magick "$T/a.png" "$T/b.png" -composite $(ls "$T"/g*.png | sed 's/$/ -composite/' | tr '\n' ' ') \
  "$T/lines.png" -composite -attenuate 0.15 +noise Gaussian -quality 92 "$OUT"
