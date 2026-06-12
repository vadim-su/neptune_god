extends RefCounted
class_name BuildingRenderer

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const BuildingGeometryScript := preload("res://game/buildings/building_geometry.gd")
const BUILDING_VISUAL_Y := 0.24


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

		var model := _instantiate_building_model(def_id)
		if model != null:
			model.position = BuildingGeometryScript.footprint_center(footprint)
			model.rotation.y = BuildingGeometryScript.rotation_y_for_quarter_turns(int(building["quarter_turns"]))
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


static func _instantiate_building_model(def_id: String) -> Node3D:
	var path := BuildingCatalogScript.model_path(def_id)
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


static func _add_fallback_building_tiles(parent: Node3D, footprint: Array, material: Material) -> void:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		var instance := MeshInstance3D.new()
		var mesh := BoxMesh.new()
		mesh.size = Vector3(0.86, 0.48, 0.86)
		instance.mesh = mesh
		instance.position = Vector3(float(tile["x"]), BUILDING_VISUAL_Y, float(tile["y"]))
		instance.material_override = material
		parent.add_child(instance)


static func _solid_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.82
	return material


static func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
