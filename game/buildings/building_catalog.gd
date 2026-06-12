extends RefCounted
class_name BuildingCatalog

const CATALOG_PATH := "res://assets/catalog/buildings.json"
const FALLBACK_DEFINITIONS := {
	"basic_miner": {
		"display_name": "Miner",
		"ui_type": "Machine",
		"state_label": "machine",
		"model": "res://assets/models/buildings/basic_mining_drill.blend",
		"color": "#617A5C",
		"walkable": false,
	},
	"wooden_chest": {
		"display_name": "Chest",
		"ui_type": "Container",
		"state_label": "idle",
		"model": "",
		"color": "#7A512E",
		"walkable": false,
	},
	"basic_belt": {
		"display_name": "Belt",
		"ui_type": "Transport",
		"state_label": "transport",
		"model": "res://assets/models/logistics/conveyor_belt_straight.blend",
		"color": "#2E383A",
		"walkable": true,
	},
	"stone_furnace": {
		"display_name": "Stone Furnace",
		"ui_type": "Machine",
		"state_label": "machine",
		"model": "res://assets/models/buildings/stone_industrial_furnace.blend",
		"color": "#6B665C",
		"walkable": false,
	},
	"basic_inserter": {
		"display_name": "Inserter",
		"ui_type": "Inserter",
		"state_label": "inserter",
		"model": "res://assets/models/buildings/industrial_robot_arm.blend",
		"color": "#8A7538",
		"walkable": false,
	},
	"basic_assembler": {
		"display_name": "Assembler",
		"ui_type": "Machine",
		"state_label": "machine",
		"model": "",
		"color": "#475C75",
		"walkable": false,
	},
	"accelerated_belt": {
		"display_name": "Accelerated Belt",
		"ui_type": "Transport",
		"state_label": "transport",
		"model": "res://assets/models/logistics/conveyor_belt_straight.blend",
		"color": "#3D4C57",
		"walkable": true,
	},
	"fast_belt": {
		"display_name": "Fast Belt",
		"ui_type": "Transport",
		"state_label": "transport",
		"model": "res://assets/models/logistics/conveyor_belt_straight.blend",
		"color": "#2E475C",
		"walkable": true,
	},
	"basic_splitter": {
		"display_name": "Splitter",
		"ui_type": "Transport",
		"state_label": "transport",
		"model": "res://assets/models/logistics/conveyor_splitter.blend",
		"color": "#3D3D4C",
		"walkable": true,
	},
	"basic_underground_belt": {
		"display_name": "Underground Belt",
		"ui_type": "Transport",
		"state_label": "transport",
		"model": "",
		"color": "#2E2E38",
		"walkable": true,
	},
}

static var _loaded := false
static var _definitions: Dictionary = {}


static func model_path(def_id: String) -> String:
	return str(definition(def_id).get("model", ""))


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


static func _ensure_loaded() -> void:
	if _loaded:
		return

	_definitions = FALLBACK_DEFINITIONS.duplicate(true)
	for key: Variant in _definitions.keys():
		_definitions[key]["id"] = str(key)

	var file := FileAccess.open(CATALOG_PATH, FileAccess.READ)
	if file == null:
		push_warning("Building catalog manifest not found at %s; using fallback definitions" % CATALOG_PATH)
		_loaded = true
		return

	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary:
		push_warning("Building catalog manifest is not a JSON object at %s" % CATALOG_PATH)
		_loaded = true
		return

	var buildings: Variant = parsed.get("buildings", [])
	if not buildings is Array:
		push_warning("Building catalog manifest has no buildings array at %s" % CATALOG_PATH)
		_loaded = true
		return

	for raw_entry: Variant in buildings:
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

	_loaded = true


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
