extends Node

const ModRegistryScript := preload("res://bootstrap/mod_registry.gd")
const CatalogRegistryScript := preload("res://bootstrap/catalog_registry.gd")
const MAIN_MOD_ID := "main"

var _mod_registry: RefCounted
var _catalog_registry: RefCounted


func _ready() -> void:
	call_deferred("_boot")


func _boot() -> void:
	_mod_registry = ModRegistryScript.new()
	if not _mod_registry.load_mods():
		_show_boot_error("Mod loading failed:\n%s" % "\n".join(_mod_registry.errors()))
		return
	get_tree().root.set_meta("mod_registry", _mod_registry)

	var main_manifest: Dictionary = _mod_registry.main_manifest()
	if main_manifest.is_empty():
		_show_boot_error("Missing required mod: mods/main.pck")
		return

	var entry_scene := str(main_manifest.get("entry_scene", ""))
	if entry_scene.is_empty():
		_show_boot_error("main mod manifest has no entry_scene")
		return

	if not _configure_game_catalogs():
		return

	_start_scene(entry_scene)


func _configure_game_catalogs() -> bool:
	_catalog_registry = CatalogRegistryScript.new()
	if not _catalog_registry.load_from_mod_registry(_mod_registry):
		_show_boot_error("Catalog loading failed:\n%s" % "\n".join(_catalog_registry.errors()))
		return false

	get_tree().root.set_meta("catalog_registry", _catalog_registry)

	return (
		_configure_catalog_script("res://game/items/item_catalog.gd", "items")
		and _configure_catalog_script("res://game/buildings/building_catalog.gd", "buildings")
		and _configure_catalog_script("res://game/recipes/recipe_catalog.gd", "recipes")
	)


func _configure_catalog_script(script_path: String, catalog_kind: String) -> bool:
	if not ResourceLoader.exists(script_path):
		_show_boot_error("Catalog script does not exist: %s" % script_path)
		return false

	var catalog_script: Variant = load(script_path)
	if catalog_script == null or not catalog_script.has_method("load_from_rows"):
		_show_boot_error("Catalog script has no load_from_rows(): %s" % script_path)
		return false

	catalog_script.load_from_rows(_catalog_registry.rows(catalog_kind))
	return true


func _start_scene(path: String) -> void:
	if not ResourceLoader.exists(path):
		_show_boot_error("Entry scene does not exist: %s" % path)
		return

	var error := get_tree().change_scene_to_file(path)
	if error != OK:
		_show_boot_error("Failed to start entry scene %s: %s" % [path, error_string(error)])


func _show_boot_error(message: String) -> void:
	push_error(message)

	var layer := CanvasLayer.new()
	add_child(layer)

	var panel := PanelContainer.new()
	panel.set_anchors_preset(Control.PRESET_FULL_RECT)
	panel.offset_left = 24.0
	panel.offset_top = 24.0
	panel.offset_right = -24.0
	panel.offset_bottom = -24.0
	layer.add_child(panel)

	var label := Label.new()
	label.text = "Neptune failed to start\n%s" % message
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	panel.add_child(label)
