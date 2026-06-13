#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GODOT_BIN="${GODOT_BIN:-godot}"
OUT_DIR="${1:-"$PROJECT_ROOT/build/pck_dist"}"
STAGE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/neptune_pck_stage.XXXXXX")"

mkdir -p "$OUT_DIR/mods"

copy_path() {
  local stage="$1"
  local rel_path="$2"
  local src="$PROJECT_ROOT/$rel_path"
  local dst="$stage/$rel_path"

  if [[ ! -e "$src" ]]; then
    return 0
  fi

  mkdir -p "$(dirname "$dst")"
  cp -a "$src" "$dst"
}

write_project_file() {
  local stage="$1"
  local main_scene="$2"

  cat > "$stage/project.godot" <<EOF
config_version=5

[application]

config/name="Neptune"
run/main_scene="$main_scene"
config/features=PackedStringArray("4.6", "Forward Plus")
config/icon="res://icon.svg"

[physics]

3d/physics_engine="Jolt Physics"

[rendering]

rendering_device/driver.windows="d3d12"
lights_and_shadows/directional_shadow/size=8192
lights_and_shadows/directional_shadow/soft_shadow_filter_quality=3
EOF
}

write_export_preset() {
  local stage="$1"

  cat > "$stage/export_presets.cfg" <<'EOF'
[preset.0]

name="Pack"
platform="Linux"
runnable=false
dedicated_server=false
custom_features=""
export_filter="all_resources"
include_filter=""
exclude_filter=""
export_path=""
encryption_include_filters=""
encryption_exclude_filters=""
encrypt_pck=false
encrypt_directory=false
script_export_mode=2

[preset.0.options]

binary_format/embed_pck=false
texture_format/bptc=true
texture_format/s3tc=true
texture_format/etc=false
texture_format/etc2=false
EOF
}

copy_extension_runtime() {
  local stage="$1"

  copy_path "$stage" "neptune_godot.gdextension"
  copy_path "$stage" "neptune_godot.gdextension.uid"
  copy_path "$stage" "target/release/libneptune_godot.so"
  copy_path "$stage" "target/release/neptune_godot.dll"
  copy_path "$stage" "target/release/libneptune_godot.dylib"
}

copy_godot_import_cache() {
  local stage="$1"

  if [[ ! -d "$PROJECT_ROOT/.godot/imported" ]]; then
    echo "warning: .godot/imported is missing; exported packs may miss imported textures/models" >&2
    return 0
  fi

  copy_path "$stage" ".godot/imported"
  copy_path "$stage" ".godot/uid_cache.bin"
}

copy_runtime_libraries_to_dist() {
  local out_dir="$1"

  copy_dist_path "$out_dir" "target/release/libneptune_godot.so"
  copy_dist_path "$out_dir" "target/release/neptune_godot.dll"
  copy_dist_path "$out_dir" "target/release/libneptune_godot.dylib"
}

copy_dist_path() {
  local out_dir="$1"
  local rel_path="$2"
  local src="$PROJECT_ROOT/$rel_path"
  local dst="$out_dir/$rel_path"

  if [[ ! -e "$src" ]]; then
    return 0
  fi

  mkdir -p "$(dirname "$dst")"
  cp -a "$src" "$dst"
}

prepare_bootstrap_stage() {
  local stage="$STAGE_ROOT/bootstrap"
  mkdir -p "$stage"

  write_project_file "$stage" "res://bootstrap/bootstrap.tscn"
  write_export_preset "$stage"
  copy_path "$stage" "bootstrap"
  copy_path "$stage" "icon.svg"
  copy_path "$stage" "icon.svg.import"
  copy_extension_runtime "$stage"
  copy_godot_import_cache "$stage"

  echo "$stage"
}

prepare_main_stage() {
  local stage="$STAGE_ROOT/main"
  mkdir -p "$stage"

  write_project_file "$stage" "res://game/main/main.tscn"
  write_export_preset "$stage"
  copy_path "$stage" "game"
  copy_path "$stage" "assets"
  copy_path "$stage" "mods/main"
  copy_path "$stage" "icon.svg"
  copy_path "$stage" "icon.svg.import"
  copy_extension_runtime "$stage"
  copy_godot_import_cache "$stage"

  echo "$stage"
}

prepare_mod_stage() {
  local mod_id="$1"
  local stage="$STAGE_ROOT/$mod_id"
  mkdir -p "$stage"

  write_project_file "$stage" ""
  write_export_preset "$stage"
  copy_path "$stage" "mods/$mod_id"
  copy_path "$stage" "icon.svg"
  copy_path "$stage" "icon.svg.import"
  copy_godot_import_cache "$stage"

  echo "$stage"
}

export_pack() {
  local stage="$1"
  local output="$2"

  mkdir -p "$(dirname "$output")"
  set +e
  "$GODOT_BIN" --headless --path "$stage" --export-pack "Pack" "$output"
  local status=$?
  set -e

  if [[ $status -ne 0 ]]; then
    if [[ -s "$output" ]]; then
      echo "warning: Godot exited with status $status after writing $output" >&2
      return 0
    fi
    return "$status"
  fi
}

BOOTSTRAP_STAGE="$(prepare_bootstrap_stage)"
MAIN_STAGE="$(prepare_main_stage)"

export_pack "$BOOTSTRAP_STAGE" "$OUT_DIR/neptune_god.pck"
export_pack "$MAIN_STAGE" "$OUT_DIR/mods/main.pck"

for mod_dir in "$PROJECT_ROOT"/mods/*; do
  if [[ ! -d "$mod_dir" ]]; then
    continue
  fi
  mod_id="$(basename "$mod_dir")"
  if [[ "$mod_id" == "main" ]]; then
    continue
  fi
  if [[ ! -f "$mod_dir/mod.json" ]]; then
    continue
  fi
  mod_stage="$(prepare_mod_stage "$mod_id")"
  export_pack "$mod_stage" "$OUT_DIR/mods/$mod_id.pck"
  copy_dist_path "$OUT_DIR" "mods/$mod_id.json"
  copy_dist_path "$OUT_DIR" "mods/$mod_id.mod.json"
done

copy_runtime_libraries_to_dist "$OUT_DIR"

cat <<EOF
Built PCK layout:
  $OUT_DIR/neptune_god.pck
  $OUT_DIR/mods/main.pck
  optional user mod packs from mods/*/
  $OUT_DIR/target/release/

Staging projects were left at:
  $STAGE_ROOT
EOF
