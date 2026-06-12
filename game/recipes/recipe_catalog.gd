extends RefCounted
class_name RecipeCatalog

const CATALOG_PATH := "res://assets/catalog/recipes.json"
const FALLBACK_RECIPES := {
	"mine_iron_ore": {
		"label": "Iron ore",
		"machines": ["basic_miner"],
		"duration_ticks": 60,
		"inputs": [],
		"outputs": [{"kind": "iron_ore", "amount": 1}],
	},
	"mine_copper_ore": {
		"label": "Copper ore",
		"machines": ["basic_miner"],
		"duration_ticks": 60,
		"inputs": [],
		"outputs": [{"kind": "copper_ore", "amount": 1}],
	},
	"mine_coal": {
		"label": "Coal",
		"machines": ["basic_miner"],
		"duration_ticks": 60,
		"inputs": [],
		"outputs": [{"kind": "coal", "amount": 1}],
	},
	"iron_plate": {
		"label": "Iron plate",
		"machines": ["stone_furnace"],
		"duration_ticks": 120,
		"inputs": [{"kind": "iron_ore", "amount": 1}],
		"outputs": [{"kind": "iron_plate", "amount": 1}],
	},
	"copper_plate": {
		"label": "Copper plate",
		"machines": ["stone_furnace"],
		"duration_ticks": 120,
		"inputs": [{"kind": "copper_ore", "amount": 1}],
		"outputs": [{"kind": "copper_plate", "amount": 1}],
	},
	"iron_gear": {
		"label": "Iron gear",
		"machines": ["basic_assembler"],
		"duration_ticks": 30,
		"inputs": [{"kind": "iron_plate", "amount": 2}],
		"outputs": [{"kind": "iron_gear", "amount": 1}],
	},
	"copper_cable": {
		"label": "Copper cable",
		"machines": ["basic_assembler"],
		"duration_ticks": 30,
		"inputs": [{"kind": "copper_plate", "amount": 1}],
		"outputs": [{"kind": "copper_cable", "amount": 2}],
	},
	"iron_stick": {
		"label": "Iron stick",
		"machines": ["basic_assembler"],
		"duration_ticks": 30,
		"inputs": [{"kind": "iron_plate", "amount": 1}],
		"outputs": [{"kind": "iron_stick", "amount": 2}],
	},
}

static var _loaded := false
static var _recipes: Dictionary = {}


static func recipes_for_machine(machine_id: String) -> Array:
	_ensure_loaded()
	var rows: Array = []
	for recipe: Dictionary in _recipes.values():
		var machines: Array = recipe.get("machines", [])
		if machines.has(machine_id):
			rows.append(recipe)
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return str(a.get("id", "")) < str(b.get("id", ""))
	)
	return rows


static func label(recipe_id: String) -> String:
	return str(definition(recipe_id).get("label", recipe_id.replace("_", " ").capitalize()))


static func duration_ticks(recipe_id: String) -> int:
	return int(definition(recipe_id).get("duration_ticks", 1))


static func inputs(recipe_id: String) -> Array:
	return definition(recipe_id).get("inputs", [])


static func outputs(recipe_id: String) -> Array:
	return definition(recipe_id).get("outputs", [])


static func definition(recipe_id: String) -> Dictionary:
	_ensure_loaded()
	return _recipes.get(recipe_id, {"id": recipe_id, "label": recipe_id.replace("_", " ").capitalize()})


static func definitions() -> Array:
	_ensure_loaded()
	var rows := _recipes.values()
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return str(a.get("id", "")) < str(b.get("id", ""))
	)
	return rows


static func reload() -> void:
	_loaded = false
	_recipes.clear()
	_ensure_loaded()


static func _ensure_loaded() -> void:
	if _loaded:
		return

	_recipes = FALLBACK_RECIPES.duplicate(true)
	for key: Variant in _recipes.keys():
		_recipes[key]["id"] = str(key)

	var file := FileAccess.open(CATALOG_PATH, FileAccess.READ)
	if file == null:
		push_warning("Recipe catalog manifest not found at %s; using fallback recipes" % CATALOG_PATH)
		_loaded = true
		return

	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary:
		push_warning("Recipe catalog manifest is not a JSON object at %s" % CATALOG_PATH)
		_loaded = true
		return

	var recipes: Variant = parsed.get("recipes", [])
	if not recipes is Array:
		push_warning("Recipe catalog manifest has no recipes array at %s" % CATALOG_PATH)
		_loaded = true
		return

	for raw_entry: Variant in recipes:
		if not raw_entry is Dictionary:
			continue
		var entry: Dictionary = raw_entry
		var id := str(entry.get("id", ""))
		if id.is_empty():
			continue
		var existing: Dictionary = {}
		if _recipes.has(id):
			existing = _recipes[id]
		var merged: Dictionary = existing.duplicate(true)
		for key: Variant in entry.keys():
			merged[key] = entry[key]
		merged["id"] = id
		_recipes[id] = merged

	_loaded = true
