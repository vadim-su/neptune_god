extends RefCounted
class_name WorldStreamingController

var environment: Node
var tile_provider: Variant
var chunk_size := 32
var max_visible_tile_radius := 64
var preload_chunk_ring := 1
var visible_chunk_rect_valid := false
var visible_chunk_rect := Rect2i()


func setup(
	environment_node: Node,
	chunk_tile_provider: Variant,
	next_chunk_size: int,
	next_max_visible_tile_radius: int,
	next_preload_chunk_ring: int
) -> void:
	environment = environment_node
	tile_provider = chunk_tile_provider
	chunk_size = next_chunk_size
	max_visible_tile_radius = next_max_visible_tile_radius
	preload_chunk_ring = next_preload_chunk_ring


func sync_around_position(player_position: Vector3, force := false) -> Variant:
	if environment == null or tile_provider == null:
		return false

	var chunk_rect := streaming_chunk_rect_for(player_position, chunk_size, max_visible_tile_radius)
	if not force and visible_chunk_rect_valid and visible_chunk_rect == chunk_rect:
		return false

	var min_chunk := chunk_rect.position
	var max_chunk := chunk_rect.position + chunk_rect.size - Vector2i.ONE
	var preload_rect := preload_chunk_rect_for(chunk_rect, preload_chunk_ring)
	var preload_min_chunk := preload_rect.position
	var preload_max_chunk := preload_rect.position + preload_rect.size - Vector2i.ONE
	environment.sync_chunks(
		tile_provider,
		chunks_in_rect(min_chunk, max_chunk),
		chunks_in_rect(preload_min_chunk, preload_max_chunk)
	)
	visible_chunk_rect = chunk_rect
	visible_chunk_rect_valid = true
	return chunk_rect


static func visible_chunk_rect_for(
	tile_rect: Rect2i,
	player_position: Vector3,
	chunk_size: int,
	generation_margin_tiles: int,
	max_visible_tile_radius: int
) -> Rect2i:
	var min_tile := tile_rect.position - Vector2i(generation_margin_tiles, generation_margin_tiles)
	var max_tile := tile_rect.position + tile_rect.size - Vector2i.ONE + Vector2i(generation_margin_tiles, generation_margin_tiles)
	var player_tile := world_to_tile(player_position)
	min_tile = Vector2i(mini(min_tile.x, player_tile.x), mini(min_tile.y, player_tile.y))
	max_tile = Vector2i(maxi(max_tile.x, player_tile.x), maxi(max_tile.y, player_tile.y))
	min_tile = clamp_tile_to_visible_radius(min_tile, player_tile, max_visible_tile_radius)
	max_tile = clamp_tile_to_visible_radius(max_tile, player_tile, max_visible_tile_radius)

	var min_chunk := tile_to_chunk(min_tile, chunk_size)
	var max_chunk := tile_to_chunk(max_tile, chunk_size)
	return Rect2i(min_chunk, max_chunk - min_chunk + Vector2i.ONE)


static func streaming_chunk_rect_for(player_position: Vector3, chunk_size: int, max_visible_tile_radius: int) -> Rect2i:
	var player_chunk := tile_to_chunk(world_to_tile(player_position), chunk_size)
	var chunk_radius := int(ceil(float(max_visible_tile_radius) / float(chunk_size)))
	var min_chunk := player_chunk - Vector2i(chunk_radius, chunk_radius)
	var max_chunk := player_chunk + Vector2i(chunk_radius, chunk_radius)
	return Rect2i(min_chunk, max_chunk - min_chunk + Vector2i.ONE)


static func preload_chunk_rect_for(visible_rect: Rect2i, preload_chunk_ring: int) -> Rect2i:
	var preload_margin := Vector2i(preload_chunk_ring, preload_chunk_ring)
	return Rect2i(
		visible_rect.position - preload_margin,
		visible_rect.size + preload_margin * 2
	)


static func world_to_tile(position: Vector3) -> Vector2i:
	return Vector2i(int(round(position.x)), int(round(position.z)))


static func tile_to_chunk(tile: Vector2i, chunk_size: int) -> Vector2i:
	return Vector2i(
		int(floor(float(tile.x) / float(chunk_size))),
		int(floor(float(tile.y) / float(chunk_size)))
	)


static func chunks_in_rect(min_chunk: Vector2i, max_chunk: Vector2i) -> Array[Vector2i]:
	var chunks: Array[Vector2i] = []
	for chunk_y in range(min_chunk.y, max_chunk.y + 1):
		for chunk_x in range(min_chunk.x, max_chunk.x + 1):
			chunks.append(Vector2i(chunk_x, chunk_y))
	return chunks


static func clamp_tile_to_visible_radius(tile: Vector2i, center: Vector2i, max_visible_tile_radius: int) -> Vector2i:
	return Vector2i(
		clampi(tile.x, center.x - max_visible_tile_radius, center.x + max_visible_tile_radius),
		clampi(tile.y, center.y - max_visible_tile_radius, center.y + max_visible_tile_radius)
	)
