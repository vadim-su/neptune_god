#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GODOT_BIN="${GODOT_BIN:-godot}"
OUT_DIR="${1:-"$PROJECT_ROOT/build/dev_dist"}"

"$PROJECT_ROOT/tools/dev_build_mods.sh" "$OUT_DIR"
cd "$OUT_DIR"
exec "$GODOT_BIN" --main-pack "$OUT_DIR/neptune_god.pck"
