extends RefCounted
class_name RecipeCatalog

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


static func load_from_rows(rows: Array) -> void:
	_recipes.clear()
	_merge_rows(rows)
	_loaded = true


static func _ensure_loaded() -> void:
	if _loaded:
		return

	push_error("RecipeCatalog used before load_from_rows()")
	_recipes.clear()
	_loaded = true


static func _merge_rows(rows: Array) -> void:
	for raw_entry: Variant in rows:
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
