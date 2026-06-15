extends RefCounted
class_name BuildingRenderer

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const BuildingGeometryScript := preload("res://game/buildings/building_geometry.gd")
const ConveyorBeltSurfaceShader := preload("res://game/buildings/conveyor_belt_surface.gdshader")
const ConveyorBeltSurfaceTexture := preload("res://assets/images/buildings/belt_surface.png")
const BUILDING_VISUAL_Y := 0.24
const SURFACE_LEVEL_HEIGHT := 1.0
const BELT_CORNER_DEFAULT_OUTPUT_QUARTER_TURNS := 3
const BELT_CORNER_MIRROR_DEFAULT_OUTPUT_QUARTER_TURNS := 1

static var _belt_surface_material_cache: Dictionary = {}


static func render_from_sim(
	sim: NeptuneSim,
	buildings_root: Node3D,
	building_tile_index: Dictionary,
	blocked_building_tiles: Dictionary
) -> void:
	_clear_children(buildings_root)
	building_tile_index.clear()
	blocked_building_tiles.clear()

	var snapshots: Array = sim.buildings()
	for raw_building: Variant in snapshots:
		var building: Dictionary = raw_building
		var building_node := Node3D.new()
		building_node.name = "Building_%s" % str(building["id"])
		buildings_root.add_child(building_node)

		var def_id := str(building["def_id"])
		var material := _solid_material(BuildingCatalogScript.color(def_id))
		var footprint: Array = building["footprint"]
		_index_building_tiles(building_tile_index, building, footprint)
		if not BuildingCatalogScript.is_walkable(def_id):
			_add_blocked_building_tiles(blocked_building_tiles, footprint)

		var model_info := _building_model_info(building)
		var model := _instantiate_building_model(def_id, str(model_info["path"]))
		if model != null:
			var model_position := BuildingGeometryScript.footprint_center(footprint)
			model_position.y = _surface_y(building)
			model.position = model_position
			model.rotation.y = float(model_info["rotation_y"])
			_apply_belt_surface_materials(model, building)
			building_node.add_child(model)
		else:
			_add_fallback_building_tiles(building_node, footprint, material)


static func _index_building_tiles(index: Dictionary, building: Dictionary, footprint: Array) -> void:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		index[Vector2i(int(tile["x"]), int(tile["y"]))] = building


static func _add_blocked_building_tiles(index: Dictionary, footprint: Array) -> void:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		index[Vector2i(int(tile["x"]), int(tile["y"]))] = true


static func _building_model_info(building: Dictionary) -> Dictionary:
	var def_id := str(building["def_id"])
	var output_quarter_turns := int(building["quarter_turns"])
	var input_quarter_turns := int(building.get("input_quarter_turns", output_quarter_turns))
	if not building.has("input_quarter_turns") or input_quarter_turns == output_quarter_turns:
		return {
			"path": BuildingCatalogScript.model_variant_path(def_id, "straight"),
			"rotation_y": BuildingGeometryScript.rotation_y_for_quarter_turns(output_quarter_turns),
		}

	var variant := "corner_mirror"
	var default_output_quarter_turns := BELT_CORNER_MIRROR_DEFAULT_OUTPUT_QUARTER_TURNS
	if input_quarter_turns == posmod(output_quarter_turns + 1, 4):
		variant = "corner"
		default_output_quarter_turns = BELT_CORNER_DEFAULT_OUTPUT_QUARTER_TURNS

	return {
		"path": BuildingCatalogScript.model_variant_path(def_id, variant),
		"rotation_y": BuildingGeometryScript.rotation_y_for_quarter_turns(
			posmod(output_quarter_turns - default_output_quarter_turns, 4)
		),
	}


static func _instantiate_building_model(def_id: String, path: String) -> Node3D:
	if path.is_empty():
		return null

	var scene := load(path) as PackedScene
	if scene == null:
		push_warning("Missing building model scene for %s at %s" % [def_id, path])
		return null

	var instance := scene.instantiate() as Node3D
	if instance == null:
		push_warning("Building model scene is not Node3D for %s at %s" % [def_id, path])
		return null

	return instance


static func _apply_belt_surface_materials(node: Node, building: Dictionary) -> void:
	if not building.has("belt_speed_tiles_per_second"):
		return

	if node is MeshInstance3D:
		var mesh_instance := node as MeshInstance3D
		if _is_belt_surface_mesh(mesh_instance):
			mesh_instance.material_override = _belt_surface_material(
				str(building["def_id"]),
				float(building["belt_speed_tiles_per_second"])
			)

	for child: Node in node.get_children():
		_apply_belt_surface_materials(child, building)


static func _is_belt_surface_mesh(mesh_instance: MeshInstance3D) -> bool:
	var node_name := str(mesh_instance.name).to_lower()
	if node_name.contains("belt_surface"):
		return true
	if mesh_instance.mesh != null:
		var mesh_name := str(mesh_instance.mesh.resource_name).to_lower()
		return mesh_name.contains("belt_surface")
	return false


static func _belt_surface_material(def_id: String, speed_tiles_per_second: float) -> ShaderMaterial:
	var cache_key := "%s:%0.4f" % [def_id, speed_tiles_per_second]
	if _belt_surface_material_cache.has(cache_key):
		return _belt_surface_material_cache[cache_key]

	var material := ShaderMaterial.new()
	material.shader = ConveyorBeltSurfaceShader
	material.set_shader_parameter("belt_texture", ConveyorBeltSurfaceTexture)
	material.set_shader_parameter("belt_speed_tiles_per_second", speed_tiles_per_second)
	material.set_shader_parameter("tint", _belt_surface_tint(def_id))
	_belt_surface_material_cache[cache_key] = material
	return material


static func _belt_surface_tint(def_id: String) -> Color:
	match def_id:
		"accelerated_belt":
			return Color(0.88, 0.96, 1.0, 1.0)
		"fast_belt":
			return Color(0.76, 0.90, 1.0, 1.0)
		_:
			return Color.WHITE


static func _add_fallback_building_tiles(parent: Node3D, footprint: Array, material: Material) -> void:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		var instance := MeshInstance3D.new()
		var mesh := BoxMesh.new()
		mesh.size = Vector3(0.86, 0.48, 0.86)
		instance.mesh = mesh
		instance.position = Vector3(float(tile["x"]), _surface_y(tile, BUILDING_VISUAL_Y), float(tile["y"]))
		instance.material_override = material
		parent.add_child(instance)


static func _surface_y(building_or_tile: Dictionary, base_y: float = 0.0) -> float:
	return base_y + float(int(building_or_tile.get("surface_z", 0))) * SURFACE_LEVEL_HEIGHT


static func _solid_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.82
	return material


static func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
