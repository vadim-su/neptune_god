#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-"$PROJECT_ROOT/build/dev_dist"}"

if [[ -n "${CARGO_BIN:-}" ]]; then
  "$CARGO_BIN" build -p neptune_godot --release --manifest-path "$PROJECT_ROOT/Cargo.toml"
elif [[ -n "${RUSTUP_TOOLCHAIN:-}" ]]; then
  cargo build -p neptune_godot --release --manifest-path "$PROJECT_ROOT/Cargo.toml"
else
  cargo +stable build -p neptune_godot --release --manifest-path "$PROJECT_ROOT/Cargo.toml"
fi
"$PROJECT_ROOT/tools/build_pck_layout.sh" "$OUT_DIR"

mkdir -p "$PROJECT_ROOT/mods"
shopt -s nullglob
for pack in "$OUT_DIR"/mods/*.pck; do
  cp -f "$pack" "$PROJECT_ROOT/mods/$(basename "$pack")"
done

cat <<EOF
Built dev mod packs:
  $PROJECT_ROOT/mods/*.pck

Full PCK layout:
  $OUT_DIR
EOF
