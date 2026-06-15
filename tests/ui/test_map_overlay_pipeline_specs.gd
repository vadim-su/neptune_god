extends "res://tests/ui/ui_test_base.gd"

func test_map_overlay_chunk_snapshot_builds_chunk_textures_asynchronously() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_chunk_snapshot([
		{
			"key": "0:0",
			"bounds": Rect2i(Vector2i.ZERO, Vector2i.ONE),
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
	], Rect2i(Vector2i.ZERO, Vector2i.ONE), [], Vector3.ZERO, Rect2i(Vector2i.ZERO, Vector2i.ONE), false)

	assert_eq(overlay.uploaded_map_chunk_texture_count_for_tests(), 0)
	assert_gt(overlay.pending_map_texture_job_count_for_tests(), 0)

	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)

	assert_eq(overlay.uploaded_map_chunk_texture_count_for_tests(), 1)


func test_map_overlay_chunk_snapshot_starts_texture_jobs_with_frame_budget() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = []
	for index in range(6):
		chunks.append({
			"key": "%d:0" % index,
			"bounds": Rect2i(Vector2i(index, 0), Vector2i.ONE),
			"tiles": [
				{"x": index, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		})

	overlay.set_chunk_snapshot(chunks, Rect2i(Vector2i.ZERO, Vector2i(6, 1)), [], Vector3.ZERO, Rect2i(Vector2i.ZERO, Vector2i(6, 1)), false)

	assert_eq(overlay._loading_map_texture_tasks.size(), 0)
	assert_eq(overlay.uploaded_map_chunk_texture_count_for_tests(), 0)
	assert_eq(overlay._pending_map_chunk_syncs.size(), 6)

	overlay.process_map_texture_jobs_for_tests()
	assert_lte(overlay._loading_map_texture_tasks.size(), overlay.MAX_MAP_TEXTURE_JOBS_STARTED_PER_FRAME)
	assert_lte(overlay._loading_map_texture_tasks.size(), overlay.MAX_ACTIVE_MAP_TEXTURE_TASKS)
	assert_gt(overlay._pending_map_chunk_syncs.size(), 0)

	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 6,
		1.0
	)

	assert_eq(overlay.uploaded_map_chunk_texture_count_for_tests(), 6)


func test_map_overlay_pending_chunk_sync_requeue_keeps_cursor_on_blocked_chunk() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = []
	for index in range(3):
		chunks.append({
			"key": "%d:0" % index,
			"bounds": Rect2i(Vector2i(index, 0), Vector2i.ONE),
			"tiles": [
				{"x": index, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		})
	overlay.set_chunk_snapshot(chunks, Rect2i(Vector2i.ZERO, Vector2i(3, 1)), [], Vector3.ZERO, Rect2i(Vector2i.ZERO, Vector2i(3, 1)), false)

	overlay.process_map_texture_jobs_for_tests()

	assert_eq(overlay._pending_map_chunk_sync_read_index, 1)
	assert_eq(str(overlay._pending_map_chunk_syncs[overlay._pending_map_chunk_sync_read_index]["key"]), "1:0")
	assert_true(overlay._pending_map_chunk_sync_lookup.has("1:0"))


func test_map_overlay_ready_texture_results_compact_after_cursor_consumes_backlog() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i.ONE)
	for index in range(40):
		var chunk_key := "%d:0" % index
		overlay._map_chunk_entries[chunk_key] = {
			"bounds": bounds,
			"snapshot_key": "stale",
		}
		overlay._ready_map_texture_results.append({
			"epoch": overlay._map_texture_epoch - 1,
			"chunk_key": chunk_key,
			"bounds": bounds,
			"snapshot_key": "stale",
			"width": 1,
			"height": 1,
			"data": PackedByteArray([0, 0, 0, 0]),
		})

	overlay._apply_ready_map_textures(40)

	assert_eq(overlay._ready_map_texture_results.size(), 0)
	assert_eq(overlay._ready_map_texture_result_read_index, 0)
	assert_eq(overlay.uploaded_map_chunk_texture_count_for_tests(), 0)


func test_map_overlay_chunk_snapshot_prioritizes_current_visible_chunks() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = []
	for index in range(6):
		chunks.append({
			"key": "%d:0" % index,
			"bounds": Rect2i(Vector2i(index, 0), Vector2i.ONE),
			"tiles": [
				{"x": index, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		})

	overlay.set_chunk_snapshot(
		chunks,
		Rect2i(Vector2i.ZERO, Vector2i(6, 1)),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i(4, 0), Vector2i.ONE),
		false
	)

	assert_eq(str(overlay._pending_map_chunk_syncs[0]["key"]), "4:0")


func test_map_overlay_chunk_snapshot_reprioritizes_already_pending_chunks_for_new_view() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = []
	for index in range(6):
		chunks.append({
			"key": "%d:0" % index,
			"bounds": Rect2i(Vector2i(index, 0), Vector2i.ONE),
			"signature": "v1",
			"tiles": [
				{"x": index, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		})

	overlay.set_chunk_snapshot(
		chunks,
		Rect2i(Vector2i.ZERO, Vector2i(6, 1)),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		false
	)
	assert_eq(str(overlay._pending_map_chunk_syncs[0]["key"]), "0:0")

	var changed_chunks: Array[Dictionary] = []
	for raw_chunk: Dictionary in chunks:
		var changed_chunk := raw_chunk.duplicate(true)
		changed_chunk["signature"] = "v2"
		changed_chunks.append(changed_chunk)
	overlay.set_chunk_snapshot(
		changed_chunks,
		Rect2i(Vector2i.ZERO, Vector2i(6, 1)),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i(4, 0), Vector2i.ONE),
		false
	)

	assert_eq(str(overlay._pending_map_chunk_syncs[0]["key"]), "4:0")
	assert_eq(overlay._pending_map_chunk_syncs.size(), 6)


func test_map_overlay_chunk_snapshot_prunes_stale_pending_chunk_syncs() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var first_chunks: Array[Dictionary] = []
	for index in range(6):
		first_chunks.append({
			"key": "%d:0" % index,
			"chunk": Vector2i(index, 0),
			"bounds": Rect2i(Vector2i(index, 0), Vector2i.ONE),
			"tiles": [
				{"x": index, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		})

	overlay.set_chunk_snapshot(
		first_chunks,
		Rect2i(Vector2i.ZERO, Vector2i(6, 1)),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i(6, 1)),
		false
	)
	assert_eq(overlay._pending_map_chunk_syncs.size(), 6)

	var replacement_chunk := {
		"key": "9:0",
		"chunk": Vector2i(9, 0),
		"bounds": Rect2i(Vector2i(9, 0), Vector2i.ONE),
		"tiles": [
			{"x": 9, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
		],
	}
	overlay.set_chunk_snapshot(
		[replacement_chunk],
		Rect2i(Vector2i(9, 0), Vector2i.ONE),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i(9, 0), Vector2i.ONE),
		false
	)

	assert_eq(overlay._pending_map_chunk_syncs.size(), 1)
	assert_eq(str(overlay._pending_map_chunk_syncs[0]["key"]), "9:0")
	assert_false(overlay._target_map_chunk_keys.has("0:0"))
	assert_true(overlay._target_map_chunk_keys.has("9:0"))

	overlay.process_map_texture_jobs_for_tests()
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)

	assert_true(overlay._map_chunk_entries.has("9:0"))
	assert_false(overlay._target_map_chunk_keys.has("0:0"))


func test_map_overlay_retains_ready_chunk_texture_outside_current_target_for_reuse() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var first_chunk := {
		"key": "0:0",
		"chunk": Vector2i.ZERO,
		"bounds": Rect2i(Vector2i.ZERO, Vector2i.ONE),
		"signature": "stable",
		"tiles": [
			{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
		],
	}
	var replacement_chunk := {
		"key": "1:0",
		"chunk": Vector2i(1, 0),
		"bounds": Rect2i(Vector2i(1, 0), Vector2i.ONE),
		"signature": "stable",
		"tiles": [
			{"x": 1, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
		],
	}
	overlay.set_chunk_snapshot(
		[first_chunk],
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		false
	)
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)
	var first_texture: Texture2D = overlay._map_chunk_entries["0:0"]["texture"]

	overlay.set_chunk_snapshot(
		[replacement_chunk],
		Rect2i(Vector2i(1, 0), Vector2i.ONE),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i(1, 0), Vector2i.ONE),
		false
	)

	assert_true(overlay._map_chunk_entries.has("0:0"))
	assert_same(first_texture, overlay._map_chunk_entries["0:0"]["texture"])
	assert_false(overlay._target_map_chunk_keys.has("0:0"))

	overlay.set_chunk_snapshot(
		[first_chunk],
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		false
	)

	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)
	assert_same(first_texture, overlay._map_chunk_entries["0:0"]["texture"])


func test_map_overlay_prunes_retained_texture_cache_without_evicting_current_target() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var texture := ImageTexture.create_from_image(Image.create(1, 1, false, Image.FORMAT_RGBA8))
	overlay._map_chunk_entries = {
		"old:0": {
			"bounds": Rect2i(Vector2i.ZERO, Vector2i.ONE),
			"snapshot_key": "old",
			"texture": texture,
			"last_texture_use": 1,
		},
		"old:1": {
			"bounds": Rect2i(Vector2i.ONE, Vector2i.ONE),
			"snapshot_key": "old",
			"texture": texture,
			"last_texture_use": 2,
		},
		"current:0": {
			"bounds": Rect2i(Vector2i(2, 0), Vector2i.ONE),
			"snapshot_key": "current",
			"texture": texture,
			"last_texture_use": 0,
		},
	}
	overlay._target_map_chunk_keys = {"current:0": true}

	overlay._prune_retained_map_chunk_texture_cache(1)

	assert_false(overlay._map_chunk_entries.has("old:0"))
	assert_true(overlay._map_chunk_entries.has("old:1"))
	assert_true(overlay._map_chunk_entries.has("current:0"))


func test_map_overlay_chunk_texture_jobs_keep_only_buildings_inside_that_chunk() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = [
		{
			"key": "0:0",
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
		{
			"key": "1:0",
			"bounds": Rect2i(Vector2i(4, 0), Vector2i(4, 4)),
			"tiles": [
				{"x": 4, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	var buildings: Array[Dictionary] = [
		{"id": 10, "def_id": "basic_miner", "footprint": [{"x": 1, "y": 1}]},
		{"id": 11, "def_id": "wooden_chest", "footprint": [{"x": 6, "y": 1}]},
	]

	overlay.set_chunk_snapshot(
		chunks,
		Rect2i(Vector2i.ZERO, Vector2i(8, 4)),
		buildings,
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i(8, 4)),
		false
	)
	overlay.process_map_texture_jobs_for_tests()
	await wait_until(
		func() -> bool:
			overlay.process_map_texture_jobs_for_tests()
			return overlay._map_chunk_entries.has("0:0") and overlay._map_chunk_entries.has("1:0"),
		1.0
	)

	assert_eq((overlay._map_chunk_entries["0:0"]["buildings"] as Array).size(), 1)
	assert_eq(int(overlay._map_chunk_entries["0:0"]["buildings"][0]["id"]), 10)
	assert_eq((overlay._map_chunk_entries["1:0"]["buildings"] as Array).size(), 1)
	assert_eq(int(overlay._map_chunk_entries["1:0"]["buildings"][0]["id"]), 11)


func test_map_overlay_chunk_building_index_keeps_buildings_in_overlapping_chunk_bounds() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = [
		{
			"key": "0:0",
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [],
		},
		{
			"key": "1:0",
			"bounds": Rect2i(Vector2i(3, 0), Vector2i(4, 4)),
			"tiles": [],
		},
	]
	var buildings: Array[Dictionary] = [
		{"id": 20, "def_id": "basic_miner", "footprint": [{"x": 3, "y": 1}]},
	]

	var buildings_by_chunk: Dictionary = overlay._buildings_by_chunk_key(chunks, buildings)

	assert_eq((buildings_by_chunk["0:0"] as Array).size(), 1)
	assert_eq((buildings_by_chunk["1:0"] as Array).size(), 1)


func test_map_overlay_chunk_building_index_uses_grid_chunk_coordinates_for_regular_chunks() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [],
		},
		{
			"key": "1:0",
			"chunk": Vector2i(1, 0),
			"bounds": Rect2i(Vector2i(4, 0), Vector2i(4, 4)),
			"tiles": [],
		},
		{
			"key": "0:1",
			"chunk": Vector2i(0, 1),
			"bounds": Rect2i(Vector2i(0, 4), Vector2i(4, 4)),
			"tiles": [],
		},
	]
	var buildings: Array[Dictionary] = [
		{"id": 30, "def_id": "basic_miner", "footprint": [{"x": 4, "y": 0}]},
		{"id": 31, "def_id": "wooden_chest", "footprint": [{"x": 0, "y": 4}]},
	]

	var buildings_by_chunk: Dictionary = overlay._buildings_by_chunk_key(chunks, buildings)

	assert_eq((buildings_by_chunk["0:0"] as Array).size(), 0)
	assert_eq((buildings_by_chunk["1:0"] as Array).size(), 1)
	assert_eq(int(buildings_by_chunk["1:0"][0]["id"]), 30)
	assert_eq((buildings_by_chunk["0:1"] as Array).size(), 1)
	assert_eq(int(buildings_by_chunk["0:1"][0]["id"]), 31)


func test_map_overlay_reuses_building_grid_chunk_index_for_matching_buildings_key() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var first_chunks: Array[Dictionary] = [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [],
		},
		{
			"key": "0:1",
			"chunk": Vector2i(0, 1),
			"bounds": Rect2i(Vector2i(0, 4), Vector2i(4, 4)),
			"tiles": [],
		},
	]
	var second_chunks: Array[Dictionary] = [
		{
			"key": "1:0",
			"chunk": Vector2i(1, 0),
			"bounds": Rect2i(Vector2i(4, 0), Vector2i(4, 4)),
			"tiles": [],
		},
		{
			"key": "1:1",
			"chunk": Vector2i(1, 1),
			"bounds": Rect2i(Vector2i(4, 4), Vector2i(4, 4)),
			"tiles": [],
		},
	]
	var buildings: Array[Dictionary] = [
		{"id": 40, "def_id": "basic_miner", "footprint": [{"x": 4, "y": 0}]},
	]

	overlay._buildings_by_chunk_key(first_chunks, buildings, "same-buildings")
	assert_eq(overlay.building_grid_chunk_cache_rebuild_count_for_tests(), 1)

	var buildings_by_chunk: Dictionary = overlay._buildings_by_chunk_key(second_chunks, buildings, "same-buildings")

	assert_eq(overlay.building_grid_chunk_cache_rebuild_count_for_tests(), 1)
	assert_eq((buildings_by_chunk["1:0"] as Array).size(), 1)
	assert_eq(int(buildings_by_chunk["1:0"][0]["id"]), 40)


func test_map_overlay_visible_chunk_key_cache_filters_to_view_bounds() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	for index in range(8):
		var key := "%d:0" % index
		overlay._map_chunk_entries[key] = {
			"bounds": Rect2i(Vector2i(index * 4, 0), Vector2i(4, 4)),
		}
		overlay._target_map_chunk_keys[key] = true
	overlay._map_chunk_entries_revision += 1

	var keys: Array[String] = overlay.visible_map_chunk_keys_for_tests(Rect2i(Vector2i(5, 0), Vector2i(8, 4)))

	assert_eq(keys.size(), 3)
	assert_false(keys.has("0:0"))
	assert_true(keys.has("1:0"))
	assert_true(keys.has("2:0"))
	assert_true(keys.has("3:0"))
	assert_false(keys.has("4:0"))


func test_map_overlay_visible_chunk_key_lookup_uses_chunk_grid_index_for_chunk_snapshots() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks: Array[Dictionary] = []
	for chunk_y in range(4):
		for chunk_x in range(4):
			var key := "%d:%d" % [chunk_x, chunk_y]
			var bounds := Rect2i(Vector2i(chunk_x * 32, chunk_y * 32), Vector2i(32, 32))
			chunks.append({
				"key": key,
				"chunk": Vector2i(chunk_x, chunk_y),
				"bounds": bounds,
				"tiles": [],
			})
			overlay._map_chunk_entries[key] = {
				"chunk": Vector2i(chunk_x, chunk_y),
				"bounds": bounds,
			}
	overlay._map_chunk_entries_revision += 1

	overlay._sync_map_chunk_entries(chunks, [])
	var keys: Array[String] = overlay.visible_map_chunk_keys_for_tests(Rect2i(Vector2i(48, 48), Vector2i(48, 48)))

	assert_true(overlay._map_chunk_grid_complete)
	assert_eq(keys.size(), 4)
	assert_true(keys.has("1:1"))
	assert_true(keys.has("2:1"))
	assert_true(keys.has("1:2"))
	assert_true(keys.has("2:2"))
	assert_false(keys.has("0:0"))
	assert_false(keys.has("3:3"))


func test_map_overlay_query_tiles_cache_filters_to_query_rect() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay._map_chunk_entries = {
		"0:0": {
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
				{"x": 3, "y": 3, "terrain": "stone", "resource": "", "amount": 0, "render": true},
			],
		},
		"1:0": {
			"bounds": Rect2i(Vector2i(4, 0), Vector2i(4, 4)),
			"tiles": [
				{"x": 4, "y": 1, "terrain": "ground", "resource": "iron_ore", "amount": 20, "render": true},
			],
		},
	}
	overlay._target_map_chunk_keys = {"0:0": true, "1:0": true}
	overlay._map_chunk_entries_revision += 1

	var first_query: Array = overlay._tiles_for_query_rect(Rect2i(Vector2i(0, 0), Vector2i(2, 2)))

	assert_eq(first_query.size(), 1)
	assert_eq(Vector2i(int(first_query[0]["x"]), int(first_query[0]["y"])), Vector2i.ZERO)
	assert_eq(overlay._query_tiles_cache_revision, overlay._map_chunk_entries_revision)
	assert_eq(overlay._query_tiles_cache_rect, Rect2i(Vector2i(0, 0), Vector2i(2, 2)))

	overlay._map_chunk_entries["2:0"] = {
		"bounds": Rect2i(Vector2i(8, 0), Vector2i(4, 4)),
		"tiles": [
			{"x": 8, "y": 1, "terrain": "ground", "resource": "coal", "amount": 10, "render": true},
		],
	}
	overlay._target_map_chunk_keys["2:0"] = true
	overlay._map_chunk_entries_revision += 1

	var second_query: Array = overlay._tiles_for_query_rect(Rect2i(Vector2i(8, 0), Vector2i(2, 2)))

	assert_eq(second_query.size(), 1)
	assert_eq(Vector2i(int(second_query[0]["x"]), int(second_query[0]["y"])), Vector2i(8, 1))


func test_map_overlay_hovered_resource_vein_reuses_cache_until_chunk_revision_changes() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay._current_visible_rect = Rect2i(Vector2i.ZERO, Vector2i(4, 4))
	overlay._map_chunk_entries = {
		"0:0": {
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [
				{"x": 1, "y": 1, "terrain": "ground", "resource": "iron_ore", "amount": 10, "render": true},
				{"x": 2, "y": 1, "terrain": "ground", "resource": "iron_ore", "amount": 15, "render": true},
			],
		},
	}
	overlay._target_map_chunk_keys = {"0:0": true}
	overlay._map_chunk_entries_revision += 1

	var first_vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(1, 1))
	overlay._map_chunk_entries["0:0"]["tiles"] = [
		{"x": 1, "y": 1, "terrain": "ground", "resource": "", "amount": 0, "render": true},
	]
	var cached_vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(1, 1))

	assert_eq(int(first_vein["amount"]), 25)
	assert_eq(int(cached_vein["amount"]), 25)

	overlay._map_chunk_entries_revision += 1
	var refreshed_vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(1, 1))

	assert_true(refreshed_vein.is_empty())


func test_map_overlay_hovered_resource_lookup_is_reused_across_cursor_tiles() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay._current_visible_rect = Rect2i(Vector2i.ZERO, Vector2i(6, 6))
	overlay._map_chunk_entries = {
		"0:0": {
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(6, 6)),
			"tiles": [
				{"x": 1, "y": 1, "terrain": "ground", "resource": "iron_ore", "amount": 10, "render": true},
				{"x": 2, "y": 1, "terrain": "ground", "resource": "iron_ore", "amount": 15, "render": true},
				{"x": 4, "y": 4, "terrain": "ground", "resource": "copper_ore", "amount": 20, "render": true},
			],
		},
	}
	overlay._target_map_chunk_keys = {"0:0": true}
	overlay._map_chunk_entries_revision += 1

	var first_vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(1, 1))
	var second_vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(4, 4))
	overlay.refresh_resource_selection()
	var same_tile_after_mouse_motion: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(4, 4))

	assert_eq(int(first_vein["amount"]), 25)
	assert_eq(int(second_vein["amount"]), 20)
	assert_eq(int(same_tile_after_mouse_motion["amount"]), 20)
	assert_eq(overlay.query_tiles_cache_rebuild_count_for_tests(), 1)
	assert_eq(overlay.visible_resource_lookup_cache_rebuild_count_for_tests(), 1)


func test_map_overlay_hovered_resource_outside_visible_rect_skips_query_tile_cache() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay._current_visible_rect = Rect2i(Vector2i.ZERO, Vector2i(2, 2))
	overlay._map_chunk_entries = {
		"0:0": {
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [
				{"x": 3, "y": 3, "terrain": "ground", "resource": "iron_ore", "amount": 10, "render": true},
			],
		},
	}
	overlay._target_map_chunk_keys = {"0:0": true}
	overlay._map_chunk_entries_revision += 1

	var vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(3, 3))

	assert_true(vein.is_empty())
	assert_eq(overlay.query_tiles_cache_rebuild_count_for_tests(), 0)


func test_map_overlay_drag_suspends_hovered_resource_vein_lookup_until_pointer_settles() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_fullscreen_open(true)
	overlay._current_visible_rect = Rect2i(Vector2i.ZERO, Vector2i(4, 4))
	overlay._map_chunk_entries = {
		"0:0": {
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"tiles": [
				{"x": 1, "y": 1, "terrain": "ground", "resource": "iron_ore", "amount": 10, "render": true},
			],
		},
	}
	overlay._target_map_chunk_keys = {"0:0": true}
	overlay._map_chunk_entries_revision += 1
	var drag_event := InputEventMouseMotion.new()
	drag_event.relative = Vector2(12.0, 0.0)
	drag_event.button_mask = MOUSE_BUTTON_MASK_LEFT

	overlay.handle_fullscreen_mouse_motion(drag_event)
	var suspended_vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(1, 1))

	assert_true(suspended_vein.is_empty())
	assert_true(overlay.resource_hover_suspended_for_tests())
	assert_eq(overlay.query_tiles_cache_rebuild_count_for_tests(), 0)

	var release_event := InputEventMouseButton.new()
	release_event.button_index = MOUSE_BUTTON_LEFT
	release_event.pressed = false
	overlay._gui_input(release_event)
	var settled_vein: Dictionary = overlay.hovered_resource_vein_for_tests(Vector2i(1, 1))

	assert_false(overlay.resource_hover_suspended_for_tests())
	assert_eq(int(settled_vein["amount"]), 10)
	assert_eq(overlay.query_tiles_cache_rebuild_count_for_tests(), 1)


func test_map_overlay_zoom_and_pan_reuse_chunk_textures_until_snapshot_changes() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i(4, 4))
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": bounds,
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
				{"x": 3, "y": 3, "terrain": "stone", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, bounds, false)
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)

	var first_texture: Texture2D = overlay._map_chunk_entries["0:0"]["texture"]
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)

	overlay.set_player_position(Vector3(4.0, 0.0, 3.0))
	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3(4.0, 0.0, 3.0), bounds, false)
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)
	assert_same(first_texture, overlay._map_chunk_entries["0:0"]["texture"])

	overlay.set_fullscreen_open(true)
	overlay.drag_by(Vector2(8.0, 4.0))
	overlay.zoom_by(1.25)
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)
	assert_same(first_texture, overlay._map_chunk_entries["0:0"]["texture"])

	var changed_chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": bounds,
			"tiles": [
				{"x": 0, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(changed_chunks, bounds, [], Vector3.ZERO, bounds, false)

	assert_gt(overlay.pending_map_texture_job_count_for_tests(), 0)


func test_map_overlay_chunk_texture_rebuilds_when_tile_content_changes_without_size_change() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i(2, 1))
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": bounds,
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
				{"x": 1, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, bounds, false)
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)

	var changed_chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": bounds,
			"tiles": [
				{"x": 0, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
				{"x": 1, "y": 0, "terrain": "stone", "resource": "iron_ore", "amount": 1, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(changed_chunks, bounds, [], Vector3.ZERO, bounds, false)

	assert_gt(overlay.pending_map_texture_job_count_for_tests(), 0)


func test_map_overlay_current_visible_rect_change_does_not_resync_chunk_textures() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i(4, 1))
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": bounds,
			"signature": "same-content",
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
				{"x": 3, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, Rect2i(Vector2i.ZERO, Vector2i.ONE), false)
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)
	var first_texture: Texture2D = overlay._map_chunk_entries["0:0"]["texture"]
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)

	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, Rect2i(Vector2i(3, 0), Vector2i.ONE), false)

	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)
	assert_same(first_texture, overlay._map_chunk_entries["0:0"]["texture"])


func test_map_overlay_visible_rect_change_does_not_resync_matching_chunks() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunk_bounds := Rect2i(Vector2i.ZERO, Vector2i(4, 1))
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": chunk_bounds,
			"signature": "stable",
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
				{"x": 3, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(chunks, chunk_bounds, [], Vector3.ZERO, chunk_bounds, false)
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)
	var revision: int = overlay._map_chunk_entries_revision
	var first_texture: Texture2D = overlay._map_chunk_entries["0:0"]["texture"]
	overlay._pending_map_chunk_syncs.clear()
	overlay._pending_map_chunk_sync_lookup.clear()

	overlay.set_chunk_snapshot(
		chunks,
		Rect2i(Vector2i(-2, 0), Vector2i(8, 1)),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i(1, 0), Vector2i(2, 1)),
		false
	)

	assert_eq(overlay._map_chunk_entries_revision, revision)
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)
	assert_same(first_texture, overlay._map_chunk_entries["0:0"]["texture"])


func test_map_overlay_visible_rect_change_reuses_flattened_tile_cache_for_matching_chunks() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunk_bounds := Rect2i(Vector2i.ZERO, Vector2i(2, 1))
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": chunk_bounds,
			"signature": "stable",
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
				{"x": 1, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(chunks, chunk_bounds, [], Vector3.ZERO, chunk_bounds, true)
	var cached_tiles: Array = overlay._tiles
	cached_tiles.append({"x": 99, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true})

	overlay.set_chunk_snapshot(
		chunks,
		Rect2i(Vector2i(-2, 0), Vector2i(6, 1)),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		true
	)

	assert_same(cached_tiles, overlay._tiles)
	assert_eq(overlay._tiles.size(), 3)


func test_map_overlay_repeated_matching_chunk_snapshot_does_not_enqueue_pending_syncs() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i.ONE)
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": bounds,
			"signature": "stable",
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, bounds, false)
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() == 1,
		1.0
	)
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)
	overlay._pending_map_chunk_syncs.clear()
	overlay._pending_map_chunk_sync_lookup.clear()

	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, bounds, false)

	assert_eq(overlay._pending_map_chunk_syncs.size(), 0)
	assert_eq(overlay.pending_map_texture_job_count_for_tests(), 0)


func test_map_overlay_repeated_matching_chunk_snapshot_does_not_request_redraw() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i.ONE)
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": bounds,
			"signature": "stable",
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
	]

	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, bounds, false, "", "stable-snapshot")
	var redraw_count: int = overlay.redraw_request_count_for_tests()

	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, bounds, false, "", "stable-snapshot")

	assert_eq(overlay.redraw_request_count_for_tests(), redraw_count)


