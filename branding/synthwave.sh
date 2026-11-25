#!/usr/bin/env bash
# Renders the Iskra synthwave artwork with ImageMagick 7: dusk sky, striped sun,
# perspective grid, chrome "ISKRA" lettering.
# Usage: synthwave.sh WIDTH HEIGHT OUTPUT [FONT]
set -euo pipefail
W=$1 H=$2 OUT=$3
FONT=${4:-"/System/Library/Fonts/Supplemental/Arial Black.ttf"}
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT

HZ=$((H * 60 / 100))   # horizon line
SR=$((H * 24 / 100))   # sun radius
SY=$((HZ - SR * 35 / 100))  # sun centre: mostly above the horizon
CX=$((W / 2))
LW=$(( (H + 539) / 540 ))   # grid line width, 1px at 540p

# Sky: deep violet to magenta at the horizon.
magick -size "${W}x${HZ}" gradient:'#0b0221-#7a1a6a' "$T/sky.png"

# Sun: yellow-to-pink disc, lower half cut by widening horizontal bands.
magick -size "$((SR * 2))x$((SR * 2))" gradient:'#ffe45c-#ff2a8a' "$T/sungrad.png"
bands=""
for i in 0 1 2 3 4 5 6; do
  y=$((SR + SR * i * 15 / 100))
  h=$((SR * (2 + i * 2) / 100 + 1))
  bands+="rectangle 0,$y $((SR * 2)),$((y + h)) "
done
magick -size "$((SR * 2))x$((SR * 2))" xc:black -fill white \
  -draw "circle $SR,$SR $SR,0" -fill black -draw "$bands" "$T/sunmask.png"
magick "$T/sungrad.png" "$T/sunmask.png" -alpha off -compose CopyOpacity -composite "$T/sun.png"
magick "$T/sun.png" -channel A -evaluate multiply 0.9 +channel \
  -background '#ff2a8a' -shadow "100x$((SR / 6))+0+0" "$T/sunglow.png"

# Ground: near-black with a magenta grid converging on the vanishing point.
GH=$((H - HZ))
grid=""
for k in $(seq -10 10); do
  grid+="line $CX,0 $((CX + k * W / 8)),$GH "
done
y=$((GH / 30)); step=$((GH / 25 + 1))
while [ "$y" -lt "$GH" ]; do
  grid+="line 0,$y $W,$y "
  y=$((y + step)); step=$((step * 13 / 10 + 1))
done
magick -size "${W}x${GH}" gradient:'#1a0533-#05010f' "$T/ground.png"
magick -size "${W}x${GH}" xc:none -stroke '#ff3cac' -strokewidth "$LW" -fill none -draw "$grid" "$T/grid.png"
magick "$T/grid.png" \( +clone -channel A -blur 0x$((LW * 4)) +channel \) -compose Over -composite "$T/gridglow.png"
magick "$T/ground.png" "$T/gridglow.png" -compose Over -composite "$T/groundfull.png"

# Scene: sky + glow + sun, then ground below the horizon, then a thin horizon glow.
magick -size "${W}x${H}" xc:'#0b0221' \
  "$T/sky.png" -geometry +0+0 -composite \
  "$T/sunglow.png" -gravity NorthWest -geometry "+$((CX - SR - SR / 6 * 2))+$((SY - SR - SR / 6 * 2))" -compose Over -composite \
  "$T/sun.png" -geometry "+$((CX - SR))+$((SY - SR))" -compose Over -composite \
  "$T/groundfull.png" -geometry "+0+$HZ" -composite \
  -fill '#ff7ad9' -draw "rectangle 0,$((HZ - LW)) $W,$((HZ + LW))" \
  "$T/scene.png"

# Lettering: italic chrome fill with a cyan outline and pink neon glow.
PT=$((H * 26 / 100))
magick -background none -font "$FONT" -pointsize "$PT" -fill white label:ISKRA \
  -shear -14x0 -trim +repage -bordercolor none -border $((H * 9 / 100)) "$T/textmask.png"
TW=$(magick identify -format %w "$T/textmask.png"); TH=$(magick identify -format %h "$T/textmask.png")
magick -size "${TW}x${TH}" gradient:'#ffffff-#7ee8ff' \
  \( -size "${TW}x$((TH / 2))" gradient:'#d9f7ff-#ffffff' \) -gravity South -composite \
  "$T/textmask.png" -compose CopyOpacity -composite "$T/textfill.png"
magick "$T/textmask.png" -fill '#00f0ff' -colorize 100 -morphology Dilate "Disk:$((LW * 3))" "$T/outline.png"
magick "$T/outline.png" -fill '#ff2a8a' -colorize 100 -channel A -blur "0x$((LW * 14))" -level 4%,60% +channel "$T/neon.png"

TY=$((-H * 2 / 100))
magick "$T/scene.png" \
  "$T/neon.png" -gravity North -geometry "+0+$TY" -compose Over -composite \
  "$T/outline.png" -gravity North -geometry "+0+$TY" -compose Over -composite \
  "$T/textfill.png" -gravity North -geometry "+0+$TY" -compose Over -composite \
  -quality 92 "$OUT"
echo "$OUT ${W}x${H}"
