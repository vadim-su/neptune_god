extends "res://addons/gut/test.gd"

const MainScript := preload("res://game/main/main.gd")
const EnvironmentScript := preload("res://game/world/environment.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")


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


func before_each() -> void:
	ItemCatalogScript.load_from_rows([
		{"id": "tin_ore", "display_name": "Tin ore", "color": "#8A8F91"},
	])


func test_visible_chunk_rect_expands_camera_footprint_and_includes_player_tile() -> void:
	var main = autofree(MainScript.new())
	main.chunk_size = 8

	var camera_tiles := Rect2i(Vector2i(10, 10), Vector2i(4, 4))
	var chunk_rect: Rect2i = main._visible_chunk_rect_for(camera_tiles, Vector3(100.0, 0.0, -20.0))

	assert_eq(chunk_rect.position, Vector2i(4, -3))
	assert_eq(chunk_rect.size, Vector2i(9, 7))


func test_visible_chunk_rect_caps_extreme_low_angle_camera_footprints() -> void:
	var main = autofree(MainScript.new())
	main.chunk_size = 32

	var camera_tiles := Rect2i(Vector2i(-1000, -1000), Vector2i(2001, 2001))
	var chunk_rect: Rect2i = main._visible_chunk_rect_for(camera_tiles, Vector3.ZERO)

	assert_eq(chunk_rect.position, Vector2i(-2, -2))
	assert_eq(chunk_rect.size, Vector2i(5, 5))


func test_streaming_chunk_rect_depends_on_player_position_not_camera_rotation_footprint() -> void:
	var main = autofree(MainScript.new())
	main.chunk_size = 32

	var center_rect: Rect2i = main._streaming_chunk_rect_for(Vector3.ZERO)
	var same_player_rect: Rect2i = main._streaming_chunk_rect_for(Vector3(31.0, 0.0, 31.0))
	var moved_player_rect: Rect2i = main._streaming_chunk_rect_for(Vector3(65.0, 0.0, 0.0))

	assert_eq(center_rect, Rect2i(Vector2i(-2, -2), Vector2i(5, 5)))
	assert_eq(same_player_rect, center_rect)
	assert_ne(moved_player_rect, center_rect)
	assert_eq(moved_player_rect, Rect2i(Vector2i(0, -2), Vector2i(5, 5)))


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


func test_detailed_map_camera_uses_same_screen_up_as_schematic_tile_y() -> void:
	var main = autofree(MainScript.new())

	assert_eq(main._map_camera_up_vector(0.0), Vector3.UP)
	assert_eq(main._map_camera_up_vector(1.0), Vector3.BACK)


func test_detailed_map_camera_height_matches_requested_pixels_per_tile() -> void:
	var main = autofree(MainScript.new())
	var viewport_height := 900.0
	var pixels_per_tile := 18.0
	var fov := 48.0

	var height: float = main._detailed_map_camera_height(pixels_per_tile, viewport_height, fov)
	var visible_world_height := 2.0 * height * tan(deg_to_rad(fov) * 0.5)

	assert_almost_eq(viewport_height / visible_world_height, pixels_per_tile, 0.001)


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


func test_environment_visible_tile_snapshot_returns_tiles_and_bounds_for_current_visible_chunks() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	environment._sync_chunks_with_tile_provider(tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])

	var snapshot: Dictionary = environment.visible_tile_snapshot()

	assert_eq(snapshot["tiles"].size(), 2)
	assert_eq(snapshot["rect"], Rect2i(Vector2i(0, 0), Vector2i(2, 1)))

	environment._sync_chunks_with_tile_provider(tile_provider, [
		Vector2i(1, 0),
	])
	snapshot = environment.visible_tile_snapshot()

	assert_eq(snapshot["tiles"].size(), 1)
	assert_eq(snapshot["rect"], Rect2i(Vector2i(1, 0), Vector2i.ONE))


func test_environment_explored_tile_snapshot_keeps_chunks_after_they_leave_visible_grid() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	environment._sync_chunks_with_tile_provider(tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])
	environment._sync_chunks_with_tile_provider(tile_provider, [
		Vector2i(1, 0),
	])

	var visible_snapshot: Dictionary = environment.visible_tile_snapshot()
	var explored_snapshot: Dictionary = environment.explored_tile_snapshot()

	assert_eq(visible_snapshot["tiles"].size(), 1)
	assert_eq(visible_snapshot["rect"], Rect2i(Vector2i(1, 0), Vector2i.ONE))
	assert_eq(explored_snapshot["tiles"].size(), 2)
	assert_eq(explored_snapshot["rect"], Rect2i(Vector2i(0, 0), Vector2i(2, 1)))


func test_environment_resource_material_uses_item_catalog_color_for_modded_resource() -> void:
	var environment = autofree(EnvironmentScript.new())

	var material: StandardMaterial3D = environment._resource_material("tin_ore")

	assert_eq(material.albedo_color, Color.from_string("#8A8F91", Color.BLACK))
	assert_null(material.albedo_texture)
