#!/usr/bin/env bash
# Put Iskra's patched Redox kernel at recipes/core/kernel/source, where config/iskra-hv.toml
# builds it from (kernel = "local"). Run from the repo root:
#   recipes/iskra/kernel/prepare.sh
set -euo pipefail
cd "$(dirname "$0")/../../.."
BASE=8d5f77538b42d4468da5e821b373ac9df512e393   # upstream kernel commit the patches apply to
SRC=recipes/core/kernel/source
P="$PWD/recipes/iskra/kernel/patches"

if [ ! -d "$SRC/.git" ]; then
  git clone -q https://gitlab.redox-os.org/redox-os/kernel.git "$SRC"
fi
git -C "$SRC" checkout -q -B iskra-hv "$BASE"
git -C "$SRC" -c user.email=iskra@localhost -c user.name=iskra am -q "$P"/*.patch
echo "kernel source ready: $(git -C "$SRC" log -1 --format='%h %s')"
