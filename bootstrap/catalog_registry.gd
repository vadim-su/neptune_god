extends RefCounted
class_name CatalogRegistry

const CATALOG_ROOT_KEYS := {
	"items": "items",
	"buildings": "buildings",
	"recipes": "recipes",
	"terrain": "terrain",
	"resources": "resources",
	"worldgen": "profiles",
	"player": "player",
}

var _rows_by_kind := {}
var _source_paths_by_kind := {}
var _errors: Array[String] = []
var _warnings: Array[String] = []


func load_from_mod_registry(mod_registry: RefCounted) -> bool:
	_rows_by_kind.clear()
	_source_paths_by_kind.clear()
	_errors.clear()
	_warnings.clear()

	for catalog_kind: String in CATALOG_ROOT_KEYS.keys():
		_load_catalog_kind(catalog_kind, mod_registry.catalog_paths(catalog_kind))

	_print_reports()
	return _errors.is_empty()


func rows(catalog_kind: String) -> Array:
	return _rows_by_kind.get(catalog_kind, []).duplicate(true)


func source_paths(catalog_kind: String) -> Array[String]:
	var paths: Array[String] = []
	for raw_path: Variant in _source_paths_by_kind.get(catalog_kind, []):
		paths.append(str(raw_path))
	return paths


func errors() -> Array[String]:
	return _errors.duplicate()


func warnings() -> Array[String]:
	return _warnings.duplicate()


func _load_catalog_kind(catalog_kind: String, paths: Array[String]) -> void:
	var root_key := str(CATALOG_ROOT_KEYS.get(catalog_kind, catalog_kind))
	var rows_by_id := {}
	var anonymous_rows: Array = []
	var loaded_paths: Array[String] = []

	for path: String in paths:
		var parsed := _read_json_dictionary(path)
		if parsed.is_empty():
			continue

		var raw_rows: Variant = parsed.get(root_key, [])
		if raw_rows is Dictionary:
			raw_rows = raw_rows.values()
		if not raw_rows is Array:
			_warn("Catalog %s has no array '%s' at %s" % [catalog_kind, root_key, path])
			continue

		loaded_paths.append(path)
		for raw_row: Variant in raw_rows:
			if not raw_row is Dictionary:
				continue
			var row: Dictionary = raw_row
			var id := str(row.get("id", ""))
			if id.is_empty():
				anonymous_rows.append(row.duplicate(true))
				continue
			var merged: Dictionary = {}
			if rows_by_id.has(id):
				merged = rows_by_id[id].duplicate(true)
			merged = _merge_dictionaries(merged, row)
			merged["id"] = id
			rows_by_id[id] = merged

	var rows: Array = anonymous_rows
	for id: String in rows_by_id.keys():
		rows.append(rows_by_id[id])
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return str(a.get("id", "")) < str(b.get("id", ""))
	)

	_rows_by_kind[catalog_kind] = rows
	_source_paths_by_kind[catalog_kind] = loaded_paths


func _merge_dictionaries(base: Dictionary, override: Dictionary) -> Dictionary:
	var merged := base.duplicate(true)
	for key: Variant in override.keys():
		if key == "id":
			merged[key] = override[key]
			continue

		var override_value: Variant = override[key]
		if merged.has(key):
			var base_value: Variant = merged[key]
			if base_value is Dictionary and override_value is Dictionary:
				merged[key] = _merge_dictionaries(base_value, override_value)
				continue
			if base_value is Array and override_value is Array:
				merged[key] = _merge_arrays(base_value, override_value)
				continue

		merged[key] = _duplicate_variant(override_value)
	return merged


func _merge_arrays(base: Array, override: Array) -> Array:
	var merged := base.duplicate(true)
	var keyed_indices := {}
	for index in merged.size():
		var key := _array_item_key(merged[index])
		if not key.is_empty():
			keyed_indices[key] = index

	for override_value: Variant in override:
		var key := _array_item_key(override_value)
		if not key.is_empty() and keyed_indices.has(key):
			var index: int = keyed_indices[key]
			var base_value: Variant = merged[index]
			if base_value is Dictionary and override_value is Dictionary:
				merged[index] = _merge_dictionaries(base_value, override_value)
			else:
				merged[index] = _duplicate_variant(override_value)
			continue

		if key.is_empty() and not (override_value is Dictionary) and merged.has(override_value):
			continue

		if not key.is_empty():
			keyed_indices[key] = merged.size()
		merged.append(_duplicate_variant(override_value))
	return merged


func _array_item_key(value: Variant) -> String:
	if not value is Dictionary:
		return ""
	var dictionary: Dictionary = value
	for key_name: String in ["id", "resource"]:
		var key := str(dictionary.get(key_name, ""))
		if not key.is_empty():
			return "%s:%s" % [key_name, key]
	return ""


func _duplicate_variant(value: Variant) -> Variant:
	if value is Dictionary:
		return value.duplicate(true)
	if value is Array:
		return value.duplicate(true)
	return value


func _read_json_dictionary(path: String) -> Dictionary:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		_error("Catalog file not found: %s" % path)
		return {}

	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary:
		_error("Catalog file is not a JSON object: %s" % path)
		return {}

	return parsed


func _error(message: String) -> void:
	if _errors.has(message):
		return
	_errors.append(message)


func _warn(message: String) -> void:
	if _warnings.has(message):
		return
	_warnings.append(message)


func _print_reports() -> void:
	for warning: String in _warnings:
		push_warning(warning)
	for error: String in _errors:
		push_error(error)
