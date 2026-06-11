extends Node3D

const GRID_STEP := 1.0
const TILE_SIZE := 1.0
const TERRAIN_Y := 0.0
const RESOURCE_Y := 0.035

const TERRAIN_COLORS := {
	"ground": Color(0.18, 0.26, 0.18),
	"stone": Color(0.32, 0.33, 0.31),
	"water": Color(0.10, 0.24, 0.34),
}

const RESOURCE_COLORS := {
	"iron_ore": Color(0.56, 0.49, 0.40),
	"copper_ore": Color(0.78, 0.39, 0.18),
	"coal": Color(0.05, 0.05, 0.05),
}

const TERRAIN_TEXTURES := {
	"ground": "res://assets/images/ground_tile.png",
	"stone": "res://assets/images/terrain_stone.png",
	"water": "res://assets/images/terrain_water.png",
}

const RESOURCE_TEXTURES := {
	"iron_ore": "res://assets/images/resource_iron_ore.png",
	"copper_ore": "res://assets/images/resource_copper_ore.png",
	"coal": "res://assets/images/resource_coal.png",
}

var _map_root: Node3D


func build_from_sim(sim: NeptuneSim) -> void:
	if _map_root != null:
		_map_root.queue_free()

	_map_root = Node3D.new()
	_map_root.name = "GeneratedMap"
	add_child(_map_root)

	var tiles: Array = sim.map_tiles()
	_add_terrain_batches(tiles)
	_add_resources(tiles)
	_add_grid(_bounds_for_tiles(tiles))


func _add_grid(bounds: Rect2i) -> void:
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.26, 0.32, 0.30, 0.55)
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED

	var mesh := ImmediateMesh.new()
	mesh.surface_begin(Mesh.PRIMITIVE_LINES, material)

	var min_x := float(bounds.position.x) - 0.5
	var max_x := float(bounds.end.x) - 0.5
	var min_z := float(bounds.position.y) - 0.5
	var max_z := float(bounds.end.y) - 0.5
	var x := min_x
	while x <= max_x + 0.01:
		mesh.surface_add_vertex(Vector3(x, 0.018, min_z))
		mesh.surface_add_vertex(Vector3(x, 0.018, max_z))
		x += GRID_STEP

	var z := min_z
	while z <= max_z + 0.01:
		mesh.surface_add_vertex(Vector3(min_x, 0.018, z))
		mesh.surface_add_vertex(Vector3(max_x, 0.018, z))
		z += GRID_STEP

	mesh.surface_end()

	var grid := MeshInstance3D.new()
	grid.name = "TileGrid"
	grid.mesh = mesh
	_map_root.add_child(grid)


func _add_terrain_batches(tiles: Array) -> void:
	for terrain_id: String in TERRAIN_COLORS.keys():
		var mesh := _terrain_mesh(tiles, terrain_id)
		if mesh == null:
			continue

		var instance := MeshInstance3D.new()
		instance.name = "Terrain_%s" % terrain_id
		instance.mesh = mesh
		instance.material_override = _terrain_material(terrain_id)
		_map_root.add_child(instance)


func _terrain_mesh(tiles: Array, terrain_id: String) -> ImmediateMesh:
	var has_tiles := false
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		if tile["terrain"] == terrain_id:
			has_tiles = true
			break

	if not has_tiles:
		return null

	var mesh := ImmediateMesh.new()
	mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES)

	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		if tile["terrain"] != terrain_id:
			continue
		has_tiles = true
		_add_textured_tile_quad(mesh, float(tile["x"]), float(tile["y"]), TERRAIN_Y, TILE_SIZE)

	mesh.surface_end()
	return mesh


func _terrain_material(terrain_id: String) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.roughness = 0.92
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	var texture: Texture2D = load(TERRAIN_TEXTURES.get(terrain_id, ""))
	if texture != null:
		material.albedo_color = Color.WHITE
		material.albedo_texture = texture
	else:
		material.albedo_color = TERRAIN_COLORS.get(terrain_id, Color.WHITE)
	return material


func _resource_material(resource_id: String) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.roughness = 0.86
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	var texture: Texture2D = load(RESOURCE_TEXTURES.get(resource_id, ""))
	if texture != null:
		material.albedo_color = Color.WHITE
		material.albedo_texture = texture
	else:
		material.albedo_color = RESOURCE_COLORS.get(resource_id, Color.WHITE)
	return material


func _add_textured_tile_quad(mesh: ImmediateMesh, x: float, z: float, y: float, size: float) -> void:
	var half := size * 0.5
	var a := Vector3(x - half, y, z - half)
	var b := Vector3(x + half, y, z - half)
	var c := Vector3(x + half, y, z + half)
	var d := Vector3(x - half, y, z + half)
	mesh.surface_set_normal(Vector3.UP)
	mesh.surface_set_uv(Vector2(0.0, 0.0))
	mesh.surface_add_vertex(a)
	mesh.surface_set_uv(Vector2(1.0, 0.0))
	mesh.surface_add_vertex(b)
	mesh.surface_set_uv(Vector2(1.0, 1.0))
	mesh.surface_add_vertex(c)
	mesh.surface_set_uv(Vector2(0.0, 0.0))
	mesh.surface_add_vertex(a)
	mesh.surface_set_uv(Vector2(1.0, 1.0))
	mesh.surface_add_vertex(c)
	mesh.surface_set_uv(Vector2(0.0, 1.0))
	mesh.surface_add_vertex(d)


func _add_resources(tiles: Array) -> void:
	var resource_root := Node3D.new()
	resource_root.name = "Resources"
	_map_root.add_child(resource_root)

	var positions_by_resource := {}
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		var resource_id: String = tile["resource"]
		if resource_id.is_empty():
			continue
		if not positions_by_resource.has(resource_id):
			positions_by_resource[resource_id] = []
		positions_by_resource[resource_id].append(Vector3(float(tile["x"]), RESOURCE_Y, float(tile["y"])))

	for resource_id: String in positions_by_resource.keys():
		var positions: Array = positions_by_resource[resource_id]
		if positions.is_empty():
			continue

		var mesh := PlaneMesh.new()
		mesh.size = Vector2(0.94, 0.94)

		var multimesh := MultiMesh.new()
		multimesh.transform_format = MultiMesh.TRANSFORM_3D
		multimesh.mesh = mesh
		multimesh.instance_count = positions.size()

		for index in positions.size():
			multimesh.set_instance_transform(index, Transform3D(Basis(), positions[index]))

		var instance := MultiMeshInstance3D.new()
		instance.name = "Resource_%s" % resource_id
		instance.multimesh = multimesh
		instance.material_override = _resource_material(resource_id)
		resource_root.add_child(instance)


func _bounds_for_tiles(tiles: Array) -> Rect2i:
	if tiles.is_empty():
		return Rect2i(Vector2i.ZERO, Vector2i.ONE)

	var first: Dictionary = tiles[0]
	var min_x: int = first["x"]
	var max_x: int = first["x"]
	var min_y: int = first["y"]
	var max_y: int = first["y"]

	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		min_x = mini(min_x, tile["x"])
		max_x = maxi(max_x, tile["x"])
		min_y = mini(min_y, tile["y"])
		max_y = maxi(max_y, tile["y"])

	return Rect2i(Vector2i(min_x, min_y), Vector2i(max_x - min_x + 1, max_y - min_y + 1))
