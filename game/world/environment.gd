extends Node3D

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

const TERRAIN_BLEND_SHADER := preload("res://game/world/terrain_blend.gdshader")
const CHUNK_BLEND_MARGIN := 1

var _map_root: Node3D
var _terrain_chunks := {}
var _resource_chunks := {}


func build_from_sim(sim: NeptuneSim) -> void:
	clear_generated_map()

	var tiles: Array = sim.map_tiles()
	_terrain_chunks[Vector2i.ZERO] = _add_terrain_chunk(Vector2i.ZERO, tiles)
	_resource_chunks[Vector2i.ZERO] = _add_resource_chunk(Vector2i.ZERO, tiles)


func clear_generated_map() -> void:
	if _map_root != null:
		_map_root.queue_free()

	_map_root = Node3D.new()
	_map_root.name = "GeneratedMap"
	add_child(_map_root)
	_terrain_chunks.clear()
	_resource_chunks.clear()


func sync_chunks(sim: NeptuneSim, visible_chunks: Array) -> void:
	if _map_root == null:
		clear_generated_map()

	var visible_lookup := {}
	for raw_chunk: Variant in visible_chunks:
		var chunk: Vector2i = raw_chunk
		visible_lookup[chunk] = true
		if not _terrain_chunks.has(chunk):
			var tiles: Array = sim.chunk_tiles_with_margin(chunk.x, chunk.y, CHUNK_BLEND_MARGIN)
			_terrain_chunks[chunk] = _add_terrain_chunk(chunk, tiles)
			_resource_chunks[chunk] = _add_resource_chunk(chunk, tiles)

	for chunk: Vector2i in _terrain_chunks.keys():
		var is_visible := visible_lookup.has(chunk)
		var terrain_node := _terrain_chunks[chunk] as Node3D
		terrain_node.visible = is_visible
		if _resource_chunks.has(chunk):
			var resource_node := _resource_chunks[chunk] as Node3D
			resource_node.visible = is_visible


func _add_terrain_chunk(chunk: Vector2i, tiles: Array) -> MeshInstance3D:
	var terrain_by_pos := {}
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		terrain_by_pos[Vector2i(tile["x"], tile["y"])] = tile["terrain"]

	var vertices := PackedVector3Array()
	var normals := PackedVector3Array()
	var uvs := PackedVector2Array()
	var colors := PackedColorArray()
	var indices := PackedInt32Array()

	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		if not bool(tile.get("render", true)):
			continue
		_add_blended_tile_geometry(
			vertices,
			normals,
			uvs,
			colors,
			indices,
			Vector2i(tile["x"], tile["y"]),
			terrain_by_pos
		)

	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_TEX_UV] = uvs
	arrays[Mesh.ARRAY_COLOR] = colors
	arrays[Mesh.ARRAY_INDEX] = indices

	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)

	var instance := MeshInstance3D.new()
	instance.name = "Terrain_%d_%d" % [chunk.x, chunk.y]
	instance.mesh = mesh
	instance.material_override = _terrain_blend_material()
	_map_root.add_child(instance)
	return instance


func _add_blended_tile_geometry(
	vertices: PackedVector3Array,
	normals: PackedVector3Array,
	uvs: PackedVector2Array,
	colors: PackedColorArray,
	indices: PackedInt32Array,
	pos: Vector2i,
	terrain_by_pos: Dictionary
) -> void:
	var base_index := vertices.size()
	var offsets := [-0.5, 0.0, 0.5]

	for z_offset: float in offsets:
		for x_offset: float in offsets:
			var world_x := float(pos.x) + x_offset
			var world_z := float(pos.y) + z_offset
			vertices.append(Vector3(world_x, TERRAIN_Y, world_z))
			normals.append(Vector3.UP)
			uvs.append(Vector2(world_x + 0.5, world_z + 0.5))
			colors.append(_terrain_blend_weight(pos, x_offset, z_offset, terrain_by_pos))

	var quads := [
		[0, 1, 4, 3],
		[1, 2, 5, 4],
		[3, 4, 7, 6],
		[4, 5, 8, 7],
	]
	for quad: Array in quads:
		indices.append(base_index + quad[0])
		indices.append(base_index + quad[1])
		indices.append(base_index + quad[2])
		indices.append(base_index + quad[0])
		indices.append(base_index + quad[2])
		indices.append(base_index + quad[3])


func _terrain_blend_weight(pos: Vector2i, x_offset: float, z_offset: float, terrain_by_pos: Dictionary) -> Color:
	var samples: Array[Vector2i] = [pos]
	if x_offset < 0.0:
		samples.append(pos + Vector2i(-1, 0))
	elif x_offset > 0.0:
		samples.append(pos + Vector2i(1, 0))

	if z_offset < 0.0:
		samples.append(pos + Vector2i(0, -1))
	elif z_offset > 0.0:
		samples.append(pos + Vector2i(0, 1))

	if x_offset != 0.0 and z_offset != 0.0:
		samples.append(pos + Vector2i(signi(x_offset), signi(z_offset)))

	var weight := Vector3.ZERO
	for sample_pos: Vector2i in samples:
		weight += _terrain_weight_vector(terrain_by_pos.get(sample_pos, terrain_by_pos.get(pos, "ground")))
	weight /= float(samples.size())
	return Color(weight.x, weight.y, weight.z, 1.0)


func _terrain_weight_vector(terrain_id: String) -> Vector3:
	match terrain_id:
		"stone":
			return Vector3(0.0, 1.0, 0.0)
		"water":
			return Vector3(0.0, 0.0, 1.0)
		_:
			return Vector3(1.0, 0.0, 0.0)


func _terrain_blend_material() -> ShaderMaterial:
	var material := ShaderMaterial.new()
	material.shader = TERRAIN_BLEND_SHADER
	material.set_shader_parameter("ground_texture", load(TERRAIN_TEXTURES["ground"]))
	material.set_shader_parameter("stone_texture", load(TERRAIN_TEXTURES["stone"]))
	material.set_shader_parameter("water_texture", load(TERRAIN_TEXTURES["water"]))
	material.set_shader_parameter("terrain_scale", 0.18)
	material.set_shader_parameter("detail_scale", 0.47)
	material.set_shader_parameter("detail_strength", 0.28)
	return material


func _resource_material(resource_id: String) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.roughness = 0.86
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	var texture: Texture2D
	var texture_path: String = RESOURCE_TEXTURES.get(resource_id, "")
	if not texture_path.is_empty():
		texture = load(texture_path)
	if texture != null:
		material.albedo_color = Color.WHITE
		material.albedo_texture = texture
	else:
		material.albedo_color = RESOURCE_COLORS.get(resource_id, Color.WHITE)
	return material


func _add_resource_chunk(chunk: Vector2i, tiles: Array) -> Node3D:
	var resource_root := Node3D.new()
	resource_root.name = "Resources_%d_%d" % [chunk.x, chunk.y]
	_map_root.add_child(resource_root)

	var positions_by_resource := {}
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		if not bool(tile.get("render", true)):
			continue
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
	return resource_root


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
