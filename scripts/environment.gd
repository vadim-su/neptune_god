extends Node3D

const GRID_STEP := 1.0
const TILE_SIZE := 1.0
const TERRAIN_Y := 0.0

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

		var material := StandardMaterial3D.new()
		material.albedo_color = TERRAIN_COLORS[terrain_id]
		material.roughness = 0.92

		var instance := MeshInstance3D.new()
		instance.name = "Terrain_%s" % terrain_id
		instance.mesh = mesh
		instance.material_override = material
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
		_add_tile_quad(mesh, float(tile["x"]), float(tile["y"]), TERRAIN_Y)

	mesh.surface_end()
	return mesh


func _add_tile_quad(mesh: ImmediateMesh, x: float, z: float, y: float) -> void:
	var half := TILE_SIZE * 0.5
	var a := Vector3(x - half, y, z - half)
	var b := Vector3(x + half, y, z - half)
	var c := Vector3(x + half, y, z + half)
	var d := Vector3(x - half, y, z + half)
	mesh.surface_set_normal(Vector3.UP)
	mesh.surface_add_vertex(a)
	mesh.surface_add_vertex(c)
	mesh.surface_add_vertex(b)
	mesh.surface_add_vertex(a)
	mesh.surface_add_vertex(d)
	mesh.surface_add_vertex(c)


func _add_resources(tiles: Array) -> void:
	var resource_root := Node3D.new()
	resource_root.name = "Resources"
	_map_root.add_child(resource_root)

	var mesh_cache := {}
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		var resource_id: String = tile["resource"]
		if resource_id.is_empty():
			continue

		var mesh: BoxMesh = mesh_cache.get(resource_id)
		if mesh == null:
			mesh = BoxMesh.new()
			mesh.size = Vector3(0.68, 0.055, 0.68)
			mesh_cache[resource_id] = mesh

		var material := StandardMaterial3D.new()
		material.albedo_color = RESOURCE_COLORS.get(resource_id, Color(0.55, 0.48, 0.40))
		material.roughness = 0.86

		var instance := MeshInstance3D.new()
		instance.name = "Resource_%s_%d_%d" % [resource_id, tile["x"], tile["y"]]
		instance.mesh = mesh
		instance.material_override = material
		instance.position = Vector3(float(tile["x"]), 0.05, float(tile["y"]))
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
