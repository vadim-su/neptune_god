extends RefCounted
class_name BuildingCatalog

static var _loaded := false
static var _definitions: Dictionary = {}


static func model_path(def_id: String) -> String:
	return str(definition(def_id).get("model", ""))


static func model_variant_path(def_id: String, variant: String) -> String:
	var def := definition(def_id)
	match variant:
		"corner":
			return str(def.get("model_corner", def.get("model", "")))
		"corner_mirror":
			return str(def.get("model_corner_mirror", def.get("model", "")))
		_:
			return str(def.get("model", ""))


static func color(def_id: String) -> Color:
	return _parse_color(definition(def_id).get("color", ""), Color(0.34, 0.36, 0.34))


static func display_name(def_id: String) -> String:
	return str(definition(def_id).get("display_name", def_id.replace("_", " ").capitalize()))


static func ui_type(def_id: String) -> String:
	return str(definition(def_id).get("ui_type", "Building"))


static func state_label(def_id: String) -> String:
	return str(definition(def_id).get("state_label", _default_state_label(def_id)))


static func is_walkable(def_id: String) -> bool:
	return bool(definition(def_id).get("walkable", false))


static func definitions() -> Array:
	_ensure_loaded()
	var rows := _definitions.values()
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return str(a.get("id", "")) < str(b.get("id", ""))
	)
	return rows


static func definition(def_id: String) -> Dictionary:
	_ensure_loaded()
	return _definitions.get(def_id, {"id": def_id})


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

	push_error("BuildingCatalog used before load_from_rows()")
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


static func _default_state_label(def_id: String) -> String:
	if is_walkable(def_id):
		return "transport"
	if ui_type(def_id) == "Machine":
		return "machine"
	if ui_type(def_id) == "Inserter":
		return "inserter"
	return "idle"


static func _parse_color(value: Variant, fallback: Color) -> Color:
	if value is Color:
		return value
	if value is String and not str(value).is_empty():
		return Color.from_string(str(value), fallback)
	return fallback
