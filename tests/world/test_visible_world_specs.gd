extends "res://addons/gut/test.gd"

const MainScript := preload("res://game/main/main.gd")
const EnvironmentScript := preload("res://game/world/environment.gd")


class FakeTileProvider:
	var requested_chunks: Array[Vector3i] = []

	func chunk_tiles_with_margin(chunk_x: int, chunk_y: int, margin: int) -> Array:
		requested_chunks.append(Vector3i(chunk_x, chunk_y, margin))
		return [
			{
				"x": chunk_x,
				"y": chunk_y,
				"terrain": "ground",
				"resource": "",
				"render": true,
			},
		]


func test_visible_chunk_rect_expands_camera_footprint_and_includes_player_tile() -> void:
	var main = autofree(MainScript.new())
	main.chunk_size = 8

	var camera_tiles := Rect2i(Vector2i(10, 10), Vector2i(4, 4))
	var chunk_rect: Rect2i = main._visible_chunk_rect_for(camera_tiles, Vector3(100.0, 0.0, -20.0))

	assert_eq(chunk_rect.position, Vector2i(-1, -3))
	assert_eq(chunk_rect.size, Vector2i(14, 7))


func test_visible_chunk_math_uses_floor_chunks_for_negative_tiles() -> void:
	var main = autofree(MainScript.new())
	main.chunk_size = 8

	assert_eq(main._tile_to_chunk(Vector2i(0, 0)), Vector2i(0, 0))
	assert_eq(main._tile_to_chunk(Vector2i(7, 7)), Vector2i(0, 0))
	assert_eq(main._tile_to_chunk(Vector2i(8, 8)), Vector2i(1, 1))
	assert_eq(main._tile_to_chunk(Vector2i(-1, -1)), Vector2i(-1, -1))
	assert_eq(main._tile_to_chunk(Vector2i(-8, -8)), Vector2i(-1, -1))
	assert_eq(main._tile_to_chunk(Vector2i(-9, -9)), Vector2i(-2, -2))


func test_chunks_in_rect_enumerates_every_visible_chunk_row_major() -> void:
	var main = autofree(MainScript.new())

	assert_eq(main._chunks_in_rect(Vector2i(-1, 2), Vector2i(1, 3)), [
		Vector2i(-1, 2),
		Vector2i(0, 2),
		Vector2i(1, 2),
		Vector2i(-1, 3),
		Vector2i(0, 3),
		Vector2i(1, 3),
	])


func test_environment_sync_creates_visible_chunks_from_tile_provider() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	environment._sync_chunks_with_tile_provider(tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])

	assert_eq(tile_provider.requested_chunks, [
		Vector3i(0, 0, environment.CHUNK_BLEND_MARGIN),
		Vector3i(1, 0, environment.CHUNK_BLEND_MARGIN),
	])
	assert_true(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_true(environment._terrain_chunks.has(Vector2i(1, 0)))
	assert_true(environment._resource_chunks.has(Vector2i(0, 0)))
	assert_true(environment._resource_chunks.has(Vector2i(1, 0)))
	assert_true(environment._terrain_chunks[Vector2i(0, 0)].visible)
	assert_true(environment._resource_chunks[Vector2i(1, 0)].visible)


func test_environment_sync_hides_chunks_that_leave_visible_grid_without_regenerating_cached_chunks() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	environment._sync_chunks_with_tile_provider(tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])
	environment._sync_chunks_with_tile_provider(tile_provider, [
		Vector2i(1, 0),
		Vector2i(2, 0),
	])

	assert_eq(tile_provider.requested_chunks, [
		Vector3i(0, 0, environment.CHUNK_BLEND_MARGIN),
		Vector3i(1, 0, environment.CHUNK_BLEND_MARGIN),
		Vector3i(2, 0, environment.CHUNK_BLEND_MARGIN),
	])
	assert_false(environment._terrain_chunks[Vector2i(0, 0)].visible)
	assert_false(environment._resource_chunks[Vector2i(0, 0)].visible)
	assert_true(environment._terrain_chunks[Vector2i(1, 0)].visible)
	assert_true(environment._resource_chunks[Vector2i(1, 0)].visible)
	assert_true(environment._terrain_chunks[Vector2i(2, 0)].visible)
	assert_true(environment._resource_chunks[Vector2i(2, 0)].visible)
