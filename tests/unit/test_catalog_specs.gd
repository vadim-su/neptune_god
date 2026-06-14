extends "res://addons/gut/test.gd"

const CatalogRegistryScript := preload("res://bootstrap/catalog_registry.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const RecipeCatalogScript := preload("res://game/recipes/recipe_catalog.gd")


class FakeModRegistry:
	var _paths_by_kind: Dictionary

	func _init(paths_by_kind: Dictionary) -> void:
		_paths_by_kind = paths_by_kind

	func catalog_paths(catalog_kind: String) -> Array[String]:
		var paths: Array[String] = []
		for raw_path: Variant in _paths_by_kind.get(catalog_kind, []):
			paths.append(str(raw_path))
		return paths


func before_each() -> void:
	ItemCatalogScript.load_from_rows([])
	BuildingCatalogScript.load_from_rows([])
	RecipeCatalogScript.load_from_rows([])


func test_catalog_registry_merges_mod_rows_as_catalog_contract() -> void:
	var base_path := "user://gut_catalog_base_items.json"
	var override_path := "user://gut_catalog_override_items.json"
	_write_json(base_path, {
		"items": [
			{
				"id": "ore",
				"display_name": "Ore",
				"meta": {"hardness": 1, "tags": ["raw"]},
				"drops": [
					{"id": "stone", "amount": 1},
					{"resource": "iron", "amount": 2},
				],
				"flags": ["smeltable"],
			},
			{"id": "wood", "display_name": "Wood"},
		],
	})
	_write_json(override_path, {
		"items": [
			{
				"id": "ore",
				"color": "#123456",
				"meta": {"tier": 2, "tags": ["raw", "modded"]},
				"drops": [
					{"id": "stone", "amount": 3},
					{"resource": "copper", "amount": 1},
				],
				"flags": ["smeltable", "burnable"],
			},
		],
	})

	var registry = CatalogRegistryScript.new()
	var loaded := registry.load_from_mod_registry(FakeModRegistry.new({
		"items": [base_path, override_path],
	}))

	assert_true(loaded)
	var rows: Array = registry.rows("items")
	assert_eq(rows.size(), 2)
	assert_eq(rows[0]["id"], "ore")
	assert_eq(rows[1]["id"], "wood")

	var ore: Dictionary = rows[0]
	assert_eq(ore["display_name"], "Ore")
	assert_eq(ore["color"], "#123456")
	assert_eq(ore["meta"]["hardness"], 1.0)
	assert_eq(ore["meta"]["tier"], 2.0)
	assert_eq(ore["meta"]["tags"], ["raw", "modded"])
	assert_eq(ore["drops"], [
		{"id": "stone", "amount": 3.0},
		{"resource": "iron", "amount": 2.0},
		{"resource": "copper", "amount": 1.0},
	])
	assert_eq(ore["flags"], ["smeltable", "burnable"])


func test_catalog_registry_rows_are_deep_copies() -> void:
	var path := "user://gut_catalog_deep_copy_items.json"
	_write_json(path, {
		"items": [
			{"id": "ore", "meta": {"hardness": 1}, "drops": [{"id": "stone", "amount": 1}]},
		],
	})

	var registry = CatalogRegistryScript.new()
	assert_true(registry.load_from_mod_registry(FakeModRegistry.new({"items": [path]})))

	var first_read: Array = registry.rows("items")
	first_read[0]["meta"]["hardness"] = 99
	first_read[0]["drops"][0]["amount"] = 99

	var second_read: Array = registry.rows("items")
	assert_eq(second_read[0]["meta"]["hardness"], 1.0)
	assert_eq(second_read[0]["drops"][0]["amount"], 1.0)


func test_building_catalog_defaults_describe_gameplay_role_contracts() -> void:
	BuildingCatalogScript.load_from_rows([
		{"id": "belt", "display_name": "Belt", "walkable": true, "model": "res://belt_straight.tscn"},
		{"id": "assembler", "ui_type": "Machine"},
		{"id": "inserter", "ui_type": "Inserter"},
		{"id": "chest"},
		{"id": "cornerless", "model": "res://straight.tscn"},
		{
			"id": "cornered",
			"model": "res://straight.tscn",
			"model_corner": "res://corner.tscn",
			"model_corner_mirror": "res://corner_mirror.tscn",
		},
	])

	assert_eq(BuildingCatalogScript.display_name("chest"), "Chest")
	assert_eq(BuildingCatalogScript.ui_type("chest"), "Building")
	assert_eq(BuildingCatalogScript.state_label("belt"), "transport")
	assert_eq(BuildingCatalogScript.state_label("assembler"), "machine")
	assert_eq(BuildingCatalogScript.state_label("inserter"), "inserter")
	assert_eq(BuildingCatalogScript.state_label("chest"), "idle")
	assert_eq(BuildingCatalogScript.model_variant_path("cornerless", "corner"), "res://straight.tscn")
	assert_eq(BuildingCatalogScript.model_variant_path("cornered", "corner"), "res://corner.tscn")
	assert_eq(BuildingCatalogScript.model_variant_path("cornered", "corner_mirror"), "res://corner_mirror.tscn")


func test_recipe_catalog_filters_by_machine_and_sorts_by_recipe_id() -> void:
	RecipeCatalogScript.load_from_rows([
		{"id": "zeta_plate", "label": "Zeta", "machines": ["furnace"]},
		{"id": "gamma_gear", "label": "Gamma", "machines": ["assembler", "furnace"]},
		{"id": "alpha_wire", "label": "Alpha", "machines": ["assembler"]},
	])

	var recipes := RecipeCatalogScript.recipes_for_machine("assembler")
	assert_eq(_ids(recipes), ["alpha_wire", "gamma_gear"])
	assert_eq(RecipeCatalogScript.recipes_for_machine("miner"), [])


func _write_json(path: String, payload: Dictionary) -> void:
	var file := FileAccess.open(path, FileAccess.WRITE)
	assert_not_null(file, "Expected to open %s for writing" % path)
	file.store_string(JSON.stringify(payload))
	file = null


func _ids(rows: Array) -> Array:
	var ids: Array = []
	for row: Dictionary in rows:
		ids.append(str(row.get("id", "")))
	return ids
