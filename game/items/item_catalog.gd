extends RefCounted
class_name ItemCatalog

static var _loaded := false
static var _definitions: Dictionary = {}


static func display_name(item_id: String) -> String:
	var def := definition(item_id)
	return str(def.get("display_name", def.get("name", item_id.replace("_", " ").capitalize())))


static func image_path(item_id: String) -> String:
	return str(definition(item_id).get("image", ""))


static func model_path(item_id: String) -> String:
	return str(definition(item_id).get("model", ""))


static func color(item_id: String) -> Color:
	return Color.from_string(str(definition(item_id).get("color", "#888888")), Color(0.54, 0.56, 0.54))


static func definition(item_id: String) -> Dictionary:
	_ensure_loaded()
	return _definitions.get(item_id, {"id": item_id, "display_name": item_id.replace("_", " ").capitalize()})


static func definitions() -> Array:
	_ensure_loaded()
	var rows := _definitions.values()
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return str(a.get("id", "")) < str(b.get("id", ""))
	)
	return rows


static func reload() -> void:
	_loaded = false
	_definitions.clear()
	_ensure_loaded()


static func load_from_rows(rows: Array) -> void:
	_definitions.clear()
	_merge_rows(rows)
	_loaded = true


static func _ensure_loaded() -> void:
	if _loaded:
		return

	push_error("ItemCatalog used before load_from_rows()")
	_definitions.clear()
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
		if _definitions.has(id):
			existing = _definitions[id]
		var merged: Dictionary = existing.duplicate(true)
		for key: Variant in entry.keys():
			merged[key] = entry[key]
		merged["id"] = id
		_definitions[id] = merged
