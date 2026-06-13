@tool
extends EditorPlugin

const DEV_BUILD_SCRIPT := "res://tools/dev_build_mods.sh"
const DEV_DIST_DIR := "res://build/dev_dist"
const MENU_BUILD_DEV_PACKS := "Neptune: Build Dev Mod Packs"
const MENU_BUILD_AND_RUN_PCK := "Neptune: Build & Run PCK Layout"


func _enter_tree() -> void:
	add_tool_menu_item(MENU_BUILD_DEV_PACKS, Callable(self, "_on_build_dev_mod_packs"))
	add_tool_menu_item(MENU_BUILD_AND_RUN_PCK, Callable(self, "_on_build_and_run_pck"))


func _exit_tree() -> void:
	remove_tool_menu_item(MENU_BUILD_DEV_PACKS)
	remove_tool_menu_item(MENU_BUILD_AND_RUN_PCK)


func _build() -> bool:
	return _build_dev_mod_packs()


func _on_build_dev_mod_packs() -> void:
	_build_dev_mod_packs()


func _on_build_and_run_pck() -> void:
	if not _build_dev_mod_packs():
		return

	var godot_path := OS.get_executable_path()
	if godot_path.is_empty():
		godot_path = "godot"
	var main_pack := ProjectSettings.globalize_path(DEV_DIST_DIR.path_join("neptune_god.pck"))
	var pid := OS.create_process(godot_path, PackedStringArray(["--main-pack", main_pack]))
	if pid < 0:
		push_error("Failed to start Godot with main pack: %s" % main_pack)
	else:
		print("Started PCK runtime process %d: %s" % [pid, main_pack])


func _build_dev_mod_packs() -> bool:
	var script_path := ProjectSettings.globalize_path(DEV_BUILD_SCRIPT)
	var dist_path := ProjectSettings.globalize_path(DEV_DIST_DIR)
	var args := PackedStringArray([dist_path])
	var output: Array = []

	print("Building Rust extension and dev PCK mod packs...")
	var exit_code := OS.execute(
		script_path,
		args,
		output,
		true,
		false
	)

	for line in output:
		print(line)

	if exit_code != 0:
		push_error("Dev PCK build failed with exit code %d" % exit_code)
		return false

	print("Dev PCK mod packs are ready")
	return true
