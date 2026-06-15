extends "res://addons/gut/test.gd"

const MainScript := preload("res://game/main/main.gd")
const EnvironmentScript := preload("res://game/world/environment.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const MapOverlayScript := preload("res://game/ui/map_overlay.gd")


class FakeTileProvider:
	var requested_chunks: Array[Vector3i] = []
	var next_job_id := 1
	var jobs := {}
	var mutex := Mutex.new()

	func _build_chunk_tiles(chunk_x: int, chunk_y: int, margin: int) -> Array:
		mutex.lock()
		requested_chunks.append(Vector3i(chunk_x, chunk_y, margin))
		mutex.unlock()
		return [
			{
				"x": chunk_x,
				"y": chunk_y,
				"terrain": "ground",
				"resource": "",
				"render": true,
			},
		]

	func start_chunk_tiles_job(chunk_x: int, chunk_y: int, margin: int) -> int:
		var job_id := next_job_id
		next_job_id += 1
		jobs[job_id] = _build_chunk_tiles(chunk_x, chunk_y, margin)
		return job_id

	func is_chunk_tiles_job_ready(job_id: int) -> bool:
		return jobs.has(job_id)

	func take_chunk_tiles_job(job_id: int) -> Array:
		var result: Array = jobs.get(job_id, [])
		jobs.erase(job_id)
		return result

	func discard_chunk_tiles_job(job_id: int) -> void:
		jobs.erase(job_id)


class FakeWorldStreamingController:
	var sync_calls := 0

	func sync_around_position(_player_position: Vector3, _force := false) -> bool:
		sync_calls += 1
		return true


func before_each() -> void:
	ItemCatalogScript.load_from_rows([
		{"id": "tin_ore", "display_name": "Tin ore", "color": "#8A8F91"},
	])


func sync_chunks_and_wait(environment: Node, tile_provider: Variant, visible_chunks: Array, preload_chunks: Array = []) -> void:
	environment.sync_chunks(tile_provider, visible_chunks, preload_chunks)
	var expected_chunks := {}
	for raw_chunk: Variant in visible_chunks:
		expected_chunks[raw_chunk] = true
	for raw_chunk: Variant in preload_chunks:
		expected_chunks[raw_chunk] = true
	await wait_until(
		func() -> bool:
			for chunk: Vector2i in expected_chunks.keys():
				if not environment._terrain_chunks.has(chunk):
					return false
				if environment._is_chunk_loading(chunk):
					return false
			return true,
		1.0
	)


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


func test_streaming_preload_chunk_rect_extends_visible_rect_by_one_chunk_ring() -> void:
	var main = autofree(MainScript.new())

	var visible_rect := Rect2i(Vector2i(-2, -2), Vector2i(5, 5))
	var preload_rect: Rect2i = main._preload_chunk_rect_for(visible_rect)

	assert_eq(preload_rect, Rect2i(Vector2i(-3, -3), Vector2i(7, 7)))


func test_streaming_center_stays_on_player_while_fullscreen_map_moves() -> void:
	var main = autofree(MainScript.new())
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_fullscreen_open(true)
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + overlay.DETAILED_WORLD_TRANSITION_PIXELS)
	overlay.set_player_position(Vector3(64.0, 0.0, -32.0))
	main.map_overlay = overlay

	assert_eq(main._streaming_center_position(Vector3.ZERO), Vector3.ZERO)


func test_fullscreen_map_view_change_does_not_sync_world_chunks() -> void:
	var main = autofree(MainScript.new())
	var streaming := FakeWorldStreamingController.new()
	main.world_streaming_controller = streaming

	main._on_map_overlay_view_changed()

	assert_eq(streaming.sync_calls, 0)


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

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])

	assert_eq(tile_provider.requested_chunks.size(), 2)
	assert_true(tile_provider.requested_chunks.has(Vector3i(0, 0, environment.CHUNK_BLEND_MARGIN)))
	assert_true(tile_provider.requested_chunks.has(Vector3i(1, 0, environment.CHUNK_BLEND_MARGIN)))
	assert_true(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_true(environment._terrain_chunks.has(Vector2i(1, 0)))
	assert_true(environment._resource_chunks.has(Vector2i(0, 0)))
	assert_true(environment._resource_chunks.has(Vector2i(1, 0)))
	assert_true(environment._terrain_chunks[Vector2i(0, 0)].visible)
	assert_true(environment._resource_chunks[Vector2i(1, 0)].visible)


func test_environment_sync_hides_chunks_that_leave_visible_grid_without_regenerating_cached_chunks() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])
	await sync_chunks_and_wait(environment, tile_provider, [
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


func test_environment_visibility_update_touches_only_chunks_that_changed_visibility() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	environment.clear_generated_map()
	for index in range(100):
		var chunk := Vector2i(index, 0)
		var terrain_node := Node3D.new()
		var resource_node := Node3D.new()
		environment._map_root.add_child(terrain_node)
		environment._map_root.add_child(resource_node)
		environment._terrain_chunks[chunk] = terrain_node
		environment._resource_chunks[chunk] = resource_node
	environment._visible_chunks = {Vector2i(0, 0): true}
	environment._chunk_visibility_update_count = 0

	environment._set_visible_chunks({Vector2i(1, 0): true})

	assert_eq(environment._chunk_visibility_update_count, 2)
	assert_false(environment._terrain_chunks[Vector2i(0, 0)].visible)
	assert_true(environment._terrain_chunks[Vector2i(1, 0)].visible)
	assert_true(environment._terrain_chunks[Vector2i(50, 0)].visible)


func test_environment_visible_chunk_snapshot_returns_chunks_and_bounds_for_current_visible_chunks() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])

	var snapshot: Dictionary = environment.visible_chunk_snapshot()

	assert_eq(snapshot["chunks"].size(), 2)
	assert_eq(snapshot["rect"], Rect2i(Vector2i(0, 0), Vector2i(2, 1)))

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(1, 0),
	])
	snapshot = environment.visible_chunk_snapshot()

	assert_eq(snapshot["chunks"].size(), 1)
	assert_eq(snapshot["rect"], Rect2i(Vector2i(1, 0), Vector2i.ONE))


func test_environment_chunk_snapshots_reuse_cached_objects_until_revision_changes() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
	])

	var visible_snapshot: Dictionary = environment.visible_chunk_snapshot()
	var repeated_visible_snapshot: Dictionary = environment.visible_chunk_snapshot()
	var explored_rect := Rect2i(Vector2i.ZERO, Vector2i.ONE)
	var scoped_snapshot: Dictionary = environment.explored_chunk_snapshot_for_rect(explored_rect)
	var repeated_scoped_snapshot: Dictionary = environment.explored_chunk_snapshot_for_rect(explored_rect)

	assert_same(visible_snapshot, repeated_visible_snapshot)
	assert_same(scoped_snapshot, repeated_scoped_snapshot)

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(1, 0),
	])

	assert_not_same(visible_snapshot, environment.visible_chunk_snapshot())
	assert_not_same(scoped_snapshot, environment.explored_chunk_snapshot_for_rect(explored_rect))


func test_environment_chunk_snapshot_returns_chunk_entries_without_flattening_tiles() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])
	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(1, 0),
	])

	var visible_snapshot: Dictionary = environment.visible_chunk_snapshot()
	var explored_snapshot: Dictionary = environment.explored_chunk_snapshot()

	assert_eq(visible_snapshot["chunks"].size(), 1)
	assert_eq(visible_snapshot["chunks"][0]["key"], "1:0")
	assert_eq(visible_snapshot["chunks"][0]["bounds"], Rect2i(Vector2i(1, 0), Vector2i.ONE))
	assert_false(str(visible_snapshot["chunks"][0].get("signature", "")).is_empty())
	assert_false(str(visible_snapshot.get("key", "")).is_empty())
	assert_eq(visible_snapshot["rect"], Rect2i(Vector2i(1, 0), Vector2i.ONE))
	assert_eq(explored_snapshot["chunks"].size(), 2)
	assert_false(str(explored_snapshot.get("key", "")).is_empty())
	assert_eq(explored_snapshot["rect"], Rect2i(Vector2i(0, 0), Vector2i(2, 1)))


func test_environment_chunk_render_task_signature_changes_with_tile_content() -> void:
	var environment = autofree(EnvironmentScript.new())
	var first_tiles := [
		{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
	]
	var changed_tiles := [
		{"x": 0, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
	]

	var first_task = environment._chunk_render_task(Vector2i.ZERO, first_tiles, 1)
	first_task.run()
	var changed_task = environment._chunk_render_task(Vector2i.ZERO, changed_tiles, 1)
	changed_task.run()

	assert_ne(first_task.result()["tile_signature"], changed_task.result()["tile_signature"])


func test_environment_chunk_render_task_prepares_visible_tile_bounds_off_thread() -> void:
	var environment = autofree(EnvironmentScript.new())
	var tiles := [
		{"x": -2, "y": 3, "terrain": "ground", "resource": "", "amount": 0, "render": true},
		{"x": 4, "y": 7, "terrain": "stone", "resource": "", "amount": 0, "render": true},
		{"x": 99, "y": 99, "terrain": "water", "resource": "", "amount": 0, "render": false},
	]

	var task = environment._chunk_render_task(Vector2i.ZERO, tiles, 1)
	task.run()

	assert_eq(task.result()["tile_bounds"], Rect2i(Vector2i(-2, 3), Vector2i(7, 5)))


func test_environment_explored_chunk_snapshot_for_rect_returns_only_intersecting_chunks() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
		Vector2i(2, 0),
	])

	var snapshot: Dictionary = environment.explored_chunk_snapshot_for_rect(Rect2i(Vector2i(1, 0), Vector2i.ONE))

	assert_eq(snapshot["chunks"].size(), 1)
	assert_eq(snapshot["chunks"][0]["key"], "1:0")
	assert_eq(snapshot["rect"], Rect2i(Vector2i(1, 0), Vector2i.ONE))


func test_environment_scoped_chunk_snapshot_preserves_requested_rect_when_empty() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()
	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
	])

	var requested_rect := Rect2i(Vector2i(20, 20), Vector2i(4, 4))
	var snapshot: Dictionary = environment.explored_chunk_snapshot_for_rect(requested_rect)

	assert_eq(snapshot["chunks"].size(), 0)
	assert_eq(snapshot["rect"], requested_rect)


func test_environment_chunk_snapshot_grid_size_is_cached_after_first_discovery() -> void:
	var environment = autofree(EnvironmentScript.new())
	environment._tile_bounds_by_chunk = {
		Vector2i.ZERO: Rect2i(Vector2i.ZERO, Vector2i(32, 32)),
		Vector2i(99, 99): Rect2i(Vector2i(3168, 3168), Vector2i(32, 32)),
	}

	assert_eq(environment.chunk_snapshot_grid_size(), Vector2i(32, 32))

	environment._tile_bounds_by_chunk.clear()

	assert_eq(environment.chunk_snapshot_grid_size(), Vector2i(32, 32))


func test_environment_explored_chunk_snapshot_keeps_chunks_after_they_leave_visible_grid() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])
	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(1, 0),
	])

	var visible_snapshot: Dictionary = environment.visible_chunk_snapshot()
	var explored_snapshot: Dictionary = environment.explored_chunk_snapshot()

	assert_eq(visible_snapshot["chunks"].size(), 1)
	assert_eq(visible_snapshot["rect"], Rect2i(Vector2i(1, 0), Vector2i.ONE))
	assert_eq(explored_snapshot["chunks"].size(), 2)
	assert_eq(explored_snapshot["rect"], Rect2i(Vector2i(0, 0), Vector2i(2, 1)))


func test_environment_async_chunk_sync_queues_missing_chunks_and_applies_ready_chunks_with_frame_budget() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()
	watch_signals(environment)

	environment.sync_chunks(tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])

	assert_eq(environment._pending_chunks.slice(environment._pending_chunk_read_index), [
		Vector2i(1, 0),
	])
	assert_false(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_false(environment._terrain_chunks.has(Vector2i(1, 0)))

	await wait_until(
		func() -> bool:
			return (
				environment._terrain_chunks.has(Vector2i(0, 0))
				and environment._terrain_chunks.has(Vector2i(1, 0))
				and environment._resource_chunks.has(Vector2i(0, 0))
				and environment._resource_chunks.has(Vector2i(1, 0))
				and not environment._is_chunk_loading(Vector2i(0, 0))
				and not environment._is_chunk_loading(Vector2i(1, 0))
			),
		1.0
	)

	assert_true(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_true(environment._terrain_chunks.has(Vector2i(1, 0)))
	assert_signal_emitted(environment, "chunks_changed")
	assert_eq(tile_provider.requested_chunks.size(), 2)
	assert_true(tile_provider.requested_chunks.has(Vector3i(0, 0, environment.CHUNK_BLEND_MARGIN)))
	assert_true(tile_provider.requested_chunks.has(Vector3i(1, 0, environment.CHUNK_BLEND_MARGIN)))


func test_environment_async_chunk_sync_refreshes_missing_chunk_queue_once_per_sync() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	environment.sync_chunks(tile_provider, [
		Vector2i(0, 0),
		Vector2i(1, 0),
		Vector2i(2, 0),
		Vector2i(3, 0),
	])

	assert_eq(environment._pending_chunk_refresh_count, 1)
	assert_eq(environment._visible_chunks.keys().size(), 4)


func test_environment_pending_chunk_queue_consumes_with_cursor_without_front_shifts() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()
	environment.clear_generated_map()
	for index in range(40):
		var chunk := Vector2i(index, 0)
		environment._pending_chunks.append(chunk)
		environment._pending_chunk_lookup[chunk] = true

	environment._start_background_chunk_jobs(tile_provider)

	assert_eq(environment._pending_chunk_read_index, 1)
	assert_eq(environment._pending_chunks[0], Vector2i(0, 0))
	assert_eq(environment._pending_chunks[environment._pending_chunk_read_index], Vector2i(1, 0))
	assert_false(environment._pending_chunk_lookup.has(Vector2i(0, 0)))


func test_environment_pending_render_task_counts_as_chunk_loading() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	environment.clear_generated_map()
	var chunk := Vector2i(2, 3)

	environment._enqueue_chunk_render_task(chunk, [], "test pending render")

	assert_true(environment._is_chunk_loading(chunk))

	environment.clear_generated_map()

	assert_false(environment._is_chunk_loading(chunk))


func test_async_map_and_chunk_pipelines_do_not_run_tasks_synchronously_on_main_thread() -> void:
	var environment_source := FileAccess.get_file_as_string("res://game/world/environment.gd")
	var map_overlay_source := FileAccess.get_file_as_string("res://game/ui/map_overlay.gd")

	assert_false(environment_source.contains("\n\t\ttask.run()"))
	assert_false(map_overlay_source.contains("\n\t\ttask.run()"))


func test_environment_chunk_apply_does_not_runtime_load_builtin_textures() -> void:
	var environment_source := FileAccess.get_file_as_string("res://game/world/environment.gd")

	assert_false(environment_source.contains("load(TERRAIN_TEXTURES"))
	assert_false(environment_source.contains("load(texture_path"))


func test_environment_ready_chunk_apply_is_split_across_frame_stages() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	watch_signals(environment)
	environment.clear_generated_map()
	environment._visible_chunks = {Vector2i(0, 0): true}
	var tiles := [
		{"x": 0, "y": 0, "terrain": "ground", "resource": "tin_ore", "render": true},
	]
	environment._ready_chunk_results.append({
		"epoch": environment._chunk_loading_epoch,
		"chunk": Vector2i(0, 0),
		"tiles": tiles,
		"terrain": environment._terrain_chunk_render_data(tiles),
		"resources": environment._resource_chunk_render_data(tiles),
	})

	environment._apply_ready_chunk_results(1)
	assert_true(environment._is_chunk_loading(Vector2i(0, 0)))
	assert_true(environment._tiles_by_chunk.has(Vector2i(0, 0)))
	assert_false(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_false(environment._resource_chunks.has(Vector2i(0, 0)))
	assert_signal_not_emitted(environment, "chunks_changed")

	environment._apply_pending_chunk_stages(1)
	assert_true(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_eq((environment._terrain_chunks.get(Vector2i(0, 0)) as Node3D).get_child_count(), 0)
	assert_false(environment._resource_chunks.has(Vector2i(0, 0)))
	assert_signal_not_emitted(environment, "chunks_changed")

	environment._apply_pending_chunk_stages(1)
	assert_gt((environment._terrain_chunks.get(Vector2i(0, 0)) as Node3D).get_child_count(), 0)
	assert_false(environment._resource_chunks.has(Vector2i(0, 0)))
	assert_signal_not_emitted(environment, "chunks_changed")

	environment._apply_pending_chunk_stages(1)
	assert_true(environment._resource_chunks.has(Vector2i(0, 0)))
	assert_eq((environment._resource_chunks.get(Vector2i(0, 0)) as Node3D).get_child_count(), 0)
	assert_signal_not_emitted(environment, "chunks_changed")

	environment._apply_pending_chunk_stages(1)
	assert_eq((environment._resource_chunks.get(Vector2i(0, 0)) as Node3D).get_child_count(), 1)
	assert_signal_not_emitted(environment, "chunks_changed")

	environment._apply_pending_chunk_stages(1)
	assert_signal_emitted(environment, "chunks_changed")
	assert_false(environment._is_chunk_loading(Vector2i(0, 0)))


func test_environment_hidden_preload_apply_does_not_dirty_map_snapshots() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	watch_signals(environment)
	environment.clear_generated_map()
	var revision_before: int = environment._chunk_snapshot_revision
	var hidden_chunk := Vector2i(1, 0)
	var tiles := [
		{"x": 32, "y": 0, "terrain": "ground", "resource": "tin_ore", "render": true},
	]
	environment._ready_chunk_results.append({
		"epoch": environment._chunk_loading_epoch,
		"chunk": hidden_chunk,
		"tiles": tiles,
		"tile_bounds": Rect2i(Vector2i(32, 0), Vector2i.ONE),
		"tile_signature": "hidden",
		"terrain": environment._terrain_chunk_render_data(tiles),
		"resources": environment._resource_chunk_render_data(tiles),
	})

	for _index in range(8):
		environment._apply_ready_chunk_results(1)

	assert_true(environment._terrain_chunks.has(hidden_chunk))
	assert_false((environment._terrain_chunks[hidden_chunk] as Node3D).visible)
	assert_true(environment._tiles_by_chunk.has(hidden_chunk))
	assert_eq(environment._chunk_snapshot_revision, revision_before)
	assert_signal_not_emitted(environment, "chunks_changed")


func test_environment_ready_and_apply_queues_compact_after_cursor_consumes_backlog() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	environment.clear_generated_map()
	for index in range(40):
		environment._ready_chunk_results.append({
			"epoch": environment._chunk_loading_epoch - 1,
			"chunk": Vector2i(index, 0),
			"tiles": [],
			"terrain": {"batches": []},
			"resources": {"batches": []},
		})

	environment._queue_ready_chunk_applies(40)

	assert_eq(environment._ready_chunk_results.size(), 0)
	assert_eq(environment._ready_chunk_result_read_index, 0)
	assert_eq(environment._pending_chunk_applies.size(), 40)

	environment._apply_pending_chunk_stages(40)

	assert_eq(environment._pending_chunk_applies.size(), 0)
	assert_eq(environment._pending_chunk_apply_read_index, 0)


func test_environment_terrain_render_data_splits_large_chunks_into_mesh_batches() -> void:
	var environment = autofree(EnvironmentScript.new())
	var tiles := [
		{"x": 0, "y": 0, "terrain": "ground", "resource": "", "render": true},
		{
			"x": environment.TERRAIN_RENDER_BATCH_SIZE_TILES,
			"y": 0,
			"terrain": "stone",
			"resource": "",
			"render": true,
		},
	]

	var render_data: Dictionary = environment._terrain_chunk_render_data(tiles)

	assert_true(render_data.has("batches"))
	assert_eq(render_data["batches"].size(), 2)
	assert_gt(render_data["batches"][0]["vertices"].size(), 0)
	assert_gt(render_data["batches"][1]["vertices"].size(), 0)


func test_environment_resource_render_data_splits_large_deposits_into_instance_batches() -> void:
	var environment = autofree(EnvironmentScript.new())
	var tiles := []
	for index in range(environment.RESOURCE_RENDER_BATCH_SIZE_INSTANCES + 1):
		tiles.append({
			"x": index,
			"y": 0,
			"terrain": "ground",
			"resource": "tin_ore",
			"amount": 1,
			"render": true,
		})

	var render_data: Dictionary = environment._resource_chunk_render_data(tiles)

	assert_true(render_data.has("batches"))
	assert_eq(render_data["batches"].size(), 2)
	assert_eq(render_data["batches"][0]["positions"].size(), environment.RESOURCE_RENDER_BATCH_SIZE_INSTANCES)
	assert_eq(render_data["batches"][1]["positions"].size(), 1)


func test_environment_chunk_render_task_prepares_render_data_without_environment_callable() -> void:
	var environment = autofree(EnvironmentScript.new())
	var tiles := [
		{"x": 2, "y": 3, "terrain": "ground", "resource": "", "amount": 0, "render": true},
		{"x": 3, "y": 3, "terrain": "stone", "resource": "tin_ore", "amount": 5, "render": true},
	]

	var task = environment._chunk_render_task(Vector2i(2, 3), tiles, 77)
	task.run()
	var result: Dictionary = task.result()

	assert_eq(result["epoch"], 77)
	assert_eq(result["chunk"], Vector2i(2, 3))
	assert_eq(result["tiles"].size(), 2)
	assert_true(result["terrain"].has("batches"))
	assert_gt(result["terrain"]["batches"][0]["vertices"].size(), 0)
	assert_true(result["resources"].has("batches"))
	assert_eq(result["resources"]["batches"][0]["resource"], "tin_ore")


func test_environment_async_chunk_sync_prewarms_hidden_chunks_before_they_become_visible() -> void:
	var environment = add_child_autoqfree(EnvironmentScript.new())
	var tile_provider := FakeTileProvider.new()

	environment.sync_chunks(tile_provider, [
		Vector2i(0, 0),
	], [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])

	await wait_until(
		func() -> bool:
			return environment._terrain_chunks.has(Vector2i(0, 0)) and environment._terrain_chunks.has(Vector2i(1, 0)),
		1.0
	)

	assert_true(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_true(environment._terrain_chunks.has(Vector2i(1, 0)))
	assert_true(environment._terrain_chunks[Vector2i(0, 0)].visible)
	assert_false(environment._terrain_chunks[Vector2i(1, 0)].visible)
	assert_eq(environment.explored_chunk_snapshot()["rect"], Rect2i(Vector2i(0, 0), Vector2i.ONE))

	await sync_chunks_and_wait(environment, tile_provider, [
		Vector2i(1, 0),
	], [
		Vector2i(0, 0),
		Vector2i(1, 0),
	])

	assert_false(environment._terrain_chunks[Vector2i(0, 0)].visible)
	assert_true(environment._terrain_chunks[Vector2i(1, 0)].visible)
	assert_eq(environment.explored_chunk_snapshot()["rect"], Rect2i(Vector2i(0, 0), Vector2i(2, 1)))
	assert_eq(tile_provider.requested_chunks.size(), 2)
	assert_true(tile_provider.requested_chunks.has(Vector3i(0, 0, environment.CHUNK_BLEND_MARGIN)))
	assert_true(tile_provider.requested_chunks.has(Vector3i(1, 0, environment.CHUNK_BLEND_MARGIN)))


func test_render_chunk_tile_provider_loads_visible_terrain_through_internal_background_job() -> void:
	var tile_provider := NeptuneChunkTileProvider.new()
	assert_true(tile_provider.configure_worldgen([], []))
	var environment = add_child_autoqfree(EnvironmentScript.new())

	environment.sync_chunks(tile_provider, [
		Vector2i(0, 0),
	])

	await wait_until(
		func() -> bool:
			return environment._terrain_chunks.has(Vector2i(0, 0)),
		1.0
	)

	assert_true(environment._terrain_chunks.has(Vector2i(0, 0)))
	assert_true(environment._terrain_chunks[Vector2i(0, 0)].visible)
	assert_gt(environment.visible_chunk_snapshot()["chunks"].size(), 0)


func test_environment_resource_material_uses_item_catalog_color_for_modded_resource() -> void:
	var environment = autofree(EnvironmentScript.new())

	var material: StandardMaterial3D = environment._resource_material("tin_ore")

	assert_eq(material.albedo_color, Color.from_string("#8A8F91", Color.BLACK))
	assert_null(material.albedo_texture)
