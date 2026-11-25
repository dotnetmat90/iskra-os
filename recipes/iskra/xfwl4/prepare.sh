#!/usr/bin/env bash
# Recreate build/iskra-src: the patched smithay, winit, winit-orbital and redox_event that the
# xfwl4 recipe builds against. Run from the repo root before building xfwl4:
#   recipes/iskra/xfwl4/prepare.sh
set -euo pipefail
cd "$(dirname "$0")/../../.."
P="$PWD/recipes/iskra/xfwl4/patches"
OUT=build/iskra-src
SMITHAY_REV=d09642ccba702c772f9a4837b78c63ce7f14229d   # what xfwl4 main pins
mkdir -p "$OUT"
cd "$OUT"

apply() {   # apply <dir> <patch dir>
  git -C "$1" -c user.email=iskra@localhost -c user.name=iskra am -q "$2"/*.patch
}

if [ ! -d smithay ]; then
  git clone -q https://github.com/smithay/smithay smithay
  git -C smithay checkout -q -B iskra-xfwl4 "$SMITHAY_REV"
  apply smithay "$P/smithay"
fi

for crate in redox_event-0.4.8 winit-orbital-0.31.0-beta.4 winit-0.31.0-beta.4; do
  [ -d "$crate" ] && continue
  name=${crate%-*}; version=${crate##*-}
  case "$crate" in *-beta.*) name=${crate%-0.31.0-beta.4}; version=0.31.0-beta.4 ;; esac
  curl -fsSL "https://static.crates.io/crates/$name/$name-$version.crate" | tar xz
  git -C "$crate" init -q && git -C "$crate" add -A
  git -C "$crate" -c user.email=iskra@localhost -c user.name=iskra commit -qm "upstream $crate"
  apply "$crate" "$P/$crate"
done
echo "build/iskra-src ready"
