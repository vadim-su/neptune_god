extends RefCounted
class_name ModRegistry

const MAIN_MOD_ID := "main"
const MODS_DIR_NAME := "mods"
const MOD_EXTENSION := "pck"
const SUPPORTED_API_VERSION := 1
const MANIFEST_PATH_TEMPLATE := "res://mods/%s/mod.json"

var _loaded_mods: Array[Dictionary] = []
var _errors: Array[String] = []
var _warnings: Array[String] = []


func load_mods() -> bool:
	_loaded_mods.clear()
	_errors.clear()
	_warnings.clear()

	var candidates := _discover_pack_files()
	var ordered_candidates := _resolve_load_order(candidates)
	if not _errors.is_empty():
		_print_reports()
		return false

	for candidate: Dictionary in ordered_candidates:
		_load_pack(candidate)

	_validate_loaded_dependencies()
	_print_reports()
	return _errors.is_empty()


func loaded_mods() -> Array[Dictionary]:
	return _loaded_mods.duplicate(true)


func main_manifest() -> Dictionary:
	return loaded_manifest(MAIN_MOD_ID)


func loaded_manifest(mod_id: String) -> Dictionary:
	for manifest: Dictionary in _loaded_mods:
		if str(manifest.get("id", "")) == mod_id:
			return manifest
	return {}


func errors() -> Array[String]:
	return _errors.duplicate()


func warnings() -> Array[String]:
	return _warnings.duplicate()


func catalog_paths(catalog_kind: String) -> Array[String]:
	var paths: Array[String] = []
	for manifest: Dictionary in _loaded_mods:
		var catalogs: Variant = manifest.get("catalogs", {})
		if not catalogs is Dictionary:
			continue
		var catalog_value: Variant = catalogs.get(catalog_kind, [])
		if catalog_value is String:
			paths.append(str(catalog_value))
		elif catalog_value is Array:
			for raw_path: Variant in catalog_value:
				var path := str(raw_path)
				if not path.is_empty():
					paths.append(path)
	return paths


func _discover_pack_files() -> Array[Dictionary]:
	var candidates: Array[Dictionary] = []
	var seen_paths := {}

	for mods_dir: String in _candidate_mod_dirs():
		var dir := DirAccess.open(mods_dir)
		if dir == null:
			continue

		dir.list_dir_begin()
		while true:
			var file_name := dir.get_next()
			if file_name.is_empty():
				break
			if dir.current_is_dir():
				continue
			if file_name.get_extension().to_lower() != MOD_EXTENSION:
				continue

			var pack_path := mods_dir.path_join(file_name)
			if seen_paths.has(pack_path):
				continue
			seen_paths[pack_path] = true

			var pack_id := file_name.get_basename()
			candidates.append({
				"id": pack_id,
				"path": pack_path,
				"sidecar_manifest": _read_sidecar_manifest(pack_path),
			})
		dir.list_dir_end()

	return candidates


func _candidate_mod_dirs() -> Array[String]:
	var dirs: Array[String] = []
	_append_unique_dir(dirs, ProjectSettings.globalize_path("res://%s" % MODS_DIR_NAME))

	var executable_dir := OS.get_executable_path().get_base_dir()
	if not executable_dir.is_empty():
		_append_unique_dir(dirs, executable_dir.path_join(MODS_DIR_NAME))

	return dirs


func _append_unique_dir(dirs: Array[String], path: String) -> void:
	if path.is_empty() or dirs.has(path):
		return
	dirs.append(path)


func _read_sidecar_manifest(pack_path: String) -> Dictionary:
	for sidecar_path: String in _sidecar_manifest_paths(pack_path):
		var manifest := _read_json_dictionary(sidecar_path)
		if not manifest.is_empty():
			manifest["sidecar_path"] = sidecar_path
			return manifest
	return {}


func _sidecar_manifest_paths(pack_path: String) -> Array[String]:
	var base_path := pack_path.get_basename()
	return [
		"%s.json" % base_path,
		"%s.mod.json" % base_path,
	]


func _resolve_load_order(candidates: Array[Dictionary]) -> Array[Dictionary]:
	var by_id := {}
	for candidate: Dictionary in candidates:
		var id := str(candidate.get("id", ""))
		if id.is_empty():
			continue
		if by_id.has(id):
			_warn("Duplicate mod pack id '%s'; ignoring %s" % [id, str(candidate.get("path", ""))])
			continue
		by_id[id] = candidate

	var ids: Array = by_id.keys()
	ids.sort()
	if ids.has(MAIN_MOD_ID):
		ids.erase(MAIN_MOD_ID)
		ids.push_front(MAIN_MOD_ID)

	var ordered: Array[Dictionary] = []
	var visiting := {}
	var visited := {}
	for id: String in ids:
		_visit_candidate(id, by_id, visiting, visited, ordered)

	return ordered


func _visit_candidate(
	id: String,
	by_id: Dictionary,
	visiting: Dictionary,
	visited: Dictionary,
	ordered: Array[Dictionary]
) -> void:
	if visited.has(id):
		return
	if visiting.has(id):
		_error("Cyclic mod dependency involving '%s'" % id)
		return

	visiting[id] = true
	var candidate: Dictionary = by_id[id]
	for dependency_id: String in _candidate_dependencies(candidate, by_id.has(MAIN_MOD_ID)):
		if dependency_id.is_empty():
			continue
		if not by_id.has(dependency_id):
			_error("Mod '%s' depends on missing mod '%s'" % [id, dependency_id])
			continue
		_visit_candidate(dependency_id, by_id, visiting, visited, ordered)

	visiting.erase(id)
	visited[id] = true
	ordered.append(candidate)


func _candidate_dependencies(candidate: Dictionary, has_main: bool) -> Array[String]:
	var dependencies: Array[String] = []
	var id := str(candidate.get("id", ""))
	if has_main and id != MAIN_MOD_ID:
		dependencies.append(MAIN_MOD_ID)

	var manifest: Dictionary = candidate.get("sidecar_manifest", {})
	if manifest.is_empty():
		return dependencies

	for dependency_id: String in _manifest_dependencies(manifest):
		if not dependencies.has(dependency_id):
			dependencies.append(dependency_id)
	return dependencies


func _load_pack(candidate: Dictionary) -> void:
	var pack_id := str(candidate.get("id", ""))
	var pack_path := str(candidate.get("path", ""))
	if pack_id.is_empty() or pack_path.is_empty():
		return

	if not ProjectSettings.load_resource_pack(pack_path, true):
		_error("Failed to load mod pack %s at %s" % [pack_id, pack_path])
		return

	var manifest := _read_manifest(pack_id)
	if manifest.is_empty():
		_error("Loaded %s, but no manifest was found at %s" % [pack_path, _manifest_path(pack_id)])
		return

	var manifest_id := str(manifest.get("id", pack_id))
	if manifest_id != pack_id:
		_error("Mod pack %s manifest id is '%s'; expected '%s'" % [pack_path, manifest_id, pack_id])
		return

	manifest["id"] = manifest_id
	manifest["pack_path"] = pack_path
	_loaded_mods.append(manifest)
	_validate_api_version(manifest)
	print("Loaded mod %s from %s" % [manifest_id, pack_path])


func _read_manifest(mod_id: String) -> Dictionary:
	return _read_json_dictionary(_manifest_path(mod_id))


func _manifest_path(mod_id: String) -> String:
	return MANIFEST_PATH_TEMPLATE % mod_id


func _read_json_dictionary(path: String) -> Dictionary:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return {}

	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary:
		_warn("JSON file is not an object: %s" % path)
		return {}

	return parsed


func _validate_api_version(manifest: Dictionary) -> void:
	var id := str(manifest.get("id", ""))
	var api_version := int(manifest.get("api_version", 0))
	if api_version <= 0:
		_error("Mod '%s' has no valid api_version" % id)
	elif api_version > SUPPORTED_API_VERSION:
		_error(
			"Mod '%s' requires api_version %d, but bootstrap supports %d" % [
				id,
				api_version,
				SUPPORTED_API_VERSION,
			]
		)


func _validate_loaded_dependencies() -> void:
	for manifest: Dictionary in _loaded_mods:
		var id := str(manifest.get("id", ""))
		for dependency_id: String in _manifest_dependencies(manifest):
			if dependency_id.is_empty():
				continue
			if loaded_manifest(dependency_id).is_empty():
				_error("Mod '%s' depends on missing mod '%s'" % [id, dependency_id])


func _manifest_dependencies(manifest: Dictionary) -> Array[String]:
	var dependencies: Array[String] = []
	var raw_dependencies: Variant = manifest.get("dependencies", [])
	if raw_dependencies is String:
		var dependency := str(raw_dependencies)
		if not dependency.is_empty():
			dependencies.append(dependency)
		return dependencies
	if not raw_dependencies is Array:
		_warn("Mod %s dependencies field is not an array" % str(manifest.get("id", "")))
		return dependencies

	for raw_dependency: Variant in raw_dependencies:
		var dependency_id := str(raw_dependency)
		if not dependency_id.is_empty() and not dependencies.has(dependency_id):
			dependencies.append(dependency_id)
	return dependencies


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
