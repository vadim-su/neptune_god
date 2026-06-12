extends RefCounted
class_name ItemCatalog

const CATALOG_PATH := "res://assets/catalog/items.json"
const FALLBACK_DEFINITIONS := {
	"iron_ore": {
		"name": "Iron ore",
		"display_name": "Iron ore",
		"image": "res://assets/images/resource_iron_ore.png",
		"model": "res://assets/models/resources/iron_ore.blend",
		"color": "#8C8790",
		"max_stack": 100,
		"weight_grams": 500,
		"bulk_units": 3,
		"size_class": "Small",
		"tags": ["ore", "raw_resource"],
	},
	"copper_ore": {
		"name": "Copper ore",
		"display_name": "Copper ore",
		"image": "res://assets/images/resource_copper_ore.png",
		"model": "res://assets/models/resources/copper_ore.blend",
		"color": "#B87342",
		"max_stack": 100,
		"weight_grams": 500,
		"bulk_units": 3,
		"size_class": "Small",
		"tags": ["ore", "raw_resource"],
	},
	"coal": {
		"name": "Coal",
		"display_name": "Coal",
		"image": "res://assets/images/resource_coal.png",
		"model": "res://assets/models/resources/pile_of_ore.blend",
		"color": "#242629",
		"max_stack": 100,
		"weight_grams": 500,
		"bulk_units": 3,
		"size_class": "Small",
		"tags": ["ore", "raw_resource"],
	},
	"wood": {
		"name": "Wood",
		"display_name": "Wood",
		"image": "",
		"model": "res://assets/models/resources/wood.blend",
		"color": "#7A4D2A",
		"max_stack": 100,
		"weight_grams": 500,
		"bulk_units": 4,
		"size_class": "Medium",
		"tags": ["raw_resource"],
	},
	"iron_plate": {
		"name": "Iron plate",
		"display_name": "Iron plate",
		"image": "",
		"model": "res://assets/models/items/iron_sheet.blend",
		"color": "#B5B8B6",
		"max_stack": 100,
		"weight_grams": 1000,
		"bulk_units": 2,
		"size_class": "Small",
		"tags": ["component", "small_part"],
	},
	"copper_plate": {
		"name": "Copper plate",
		"display_name": "Copper plate",
		"image": "",
		"model": "res://assets/models/items/iron_sheet.blend",
		"color": "#C97945",
		"max_stack": 100,
		"weight_grams": 1000,
		"bulk_units": 2,
		"size_class": "Small",
		"tags": ["component", "small_part"],
	},
	"iron_gear": {
		"name": "Iron gear",
		"display_name": "Iron gear",
		"image": "",
		"model": "res://assets/models/items/steel_beams.blend",
		"color": "#A8ADB0",
		"max_stack": 100,
		"weight_grams": 1000,
		"bulk_units": 2,
		"size_class": "Small",
		"tags": ["component", "small_part"],
	},
	"copper_cable": {
		"name": "Copper cable",
		"display_name": "Copper cable",
		"image": "",
		"model": "",
		"color": "#D48745",
		"max_stack": 200,
		"weight_grams": 1000,
		"bulk_units": 2,
		"size_class": "Small",
		"tags": ["component", "small_part"],
	},
	"iron_stick": {
		"name": "Iron stick",
		"display_name": "Iron stick",
		"image": "",
		"model": "res://assets/models/items/steel_beams.blend",
		"color": "#AEB2B0",
		"max_stack": 100,
		"weight_grams": 1000,
		"bulk_units": 2,
		"size_class": "Small",
		"tags": ["component", "small_part"],
	},
}

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


static func _ensure_loaded() -> void:
	if _loaded:
		return

	_definitions = FALLBACK_DEFINITIONS.duplicate(true)
	for key: Variant in _definitions.keys():
		_definitions[key]["id"] = str(key)

	var file := FileAccess.open(CATALOG_PATH, FileAccess.READ)
	if file == null:
		push_warning("Item catalog manifest not found at %s; using fallback definitions" % CATALOG_PATH)
		_loaded = true
		return

	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary:
		push_warning("Item catalog manifest is not a JSON object at %s" % CATALOG_PATH)
		_loaded = true
		return

	var items: Variant = parsed.get("items", [])
	if not items is Array:
		push_warning("Item catalog manifest has no items array at %s" % CATALOG_PATH)
		_loaded = true
		return

	for raw_entry: Variant in items:
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
