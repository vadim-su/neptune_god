extends "res://tests/ui/ui_test_base.gd"

func test_map_overlay_resource_vein_groups_same_resource_tiles_within_two_tile_gap() -> void:
	var vein: Dictionary = MapOverlayScript.collect_resource_vein(Vector2i(0, 0), [
		{"x": 0, "y": 0, "resource": "iron_ore", "amount": 10, "render": true},
		{"x": 1, "y": 0, "resource": "iron_ore", "amount": 20, "render": true},
		{"x": 3, "y": 1, "resource": "iron_ore", "amount": 30, "render": true},
		{"x": 6, "y": 1, "resource": "iron_ore", "amount": 40, "render": true},
		{"x": 1, "y": 1, "resource": "copper_ore", "amount": 50, "render": true},
		{"x": 2, "y": 0, "resource": "iron_ore", "amount": 999, "render": false},
		{"x": 2, "y": 1, "resource": "iron_ore", "amount": 0, "render": true},
	], Rect2i(Vector2i(-1, -1), Vector2i(8, 4)))

	assert_eq(vein["resource"], "iron_ore")
	assert_eq(vein["amount"], 60)
	assert_eq(_sorted_vec2i(vein["tiles"]), [
		Vector2i(0, 0),
		Vector2i(1, 0),
		Vector2i(3, 1),
	])


func test_map_overlay_resource_vein_counts_only_visible_part_of_patch() -> void:
	var tiles := [
		{"x": 31, "y": 0, "resource": "iron_ore", "amount": 25, "render": true},
		{"x": 33, "y": 0, "resource": "iron_ore", "amount": 35, "render": true},
	]

	var partial: Dictionary = MapOverlayScript.collect_resource_vein(
		Vector2i(31, 0),
		tiles,
		Rect2i(Vector2i(0, -1), Vector2i(32, 3))
	)
	var full: Dictionary = MapOverlayScript.collect_resource_vein(
		Vector2i(31, 0),
		tiles,
		Rect2i(Vector2i(0, -1), Vector2i(64, 3))
	)

	assert_eq(partial["amount"], 25)
	assert_eq(partial["tiles"], [Vector2i(31, 0)])
	assert_eq(full["amount"], 60)
	assert_eq(_sorted_vec2i(full["tiles"]), [Vector2i(31, 0), Vector2i(33, 0)])


func test_map_overlay_fullscreen_contract_has_player_dot_label_and_detailed_zoom_transition() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_player_position(Vector3(12.0, 0.0, -4.0))

	assert_false(overlay.is_fullscreen_open())
	overlay.set_fullscreen_open(true)

	assert_true(overlay.is_fullscreen_open())
	assert_true(overlay.visible)
	assert_eq(overlay.player_marker_snapshot()["tile"], Vector2i(12, -4))
	assert_eq(overlay.player_marker_snapshot()["label"], "player")

	var half_transition: float = overlay.DETAILED_WORLD_TRANSITION_PIXELS * 0.5
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE - half_transition - 0.1)
	assert_eq(overlay.detailed_world_blend(), 0.0)
	assert_false(overlay.detailed_world_visible())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE)
	assert_gt(overlay.detailed_world_blend(), 0.0)
	assert_lt(overlay.detailed_world_blend(), 1.0)
	assert_false(overlay.detailed_world_visible())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + half_transition)
	assert_eq(overlay.detailed_world_blend(), 1.0)
	assert_true(overlay.detailed_world_visible())


func test_map_overlay_player_marker_keeps_screen_radius_across_zoom_levels() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()

	var zoomed_out_radius: float = overlay.player_marker_radius_for_tests(overlay.MIN_PIXELS_PER_TILE)
	var normal_radius: float = overlay.player_marker_radius_for_tests(10.0)
	var transition_radius: float = overlay.player_marker_radius_for_tests(overlay.DETAILED_WORLD_PIXELS_PER_TILE)
	var zoomed_in_radius: float = overlay.player_marker_radius_for_tests(overlay.MAX_PIXELS_PER_TILE)

	assert_eq(zoomed_out_radius, normal_radius)
	assert_eq(normal_radius, transition_radius)
	assert_eq(transition_radius, zoomed_in_radius)
	assert_true(zoomed_out_radius >= 5.0)


func test_minimap_scene_is_framed_rect_with_embedded_map_overlay() -> void:
	var minimap = autofree(MinimapScene.instantiate())
	minimap.configure_minimap()
	var map := minimap.map_overlay as Control

	assert_true(minimap is PanelContainer)
	assert_eq(minimap.anchor_left, 1.0)
	assert_eq(minimap.anchor_top, 1.0)
	assert_eq(minimap.anchor_right, 1.0)
	assert_eq(minimap.anchor_bottom, 1.0)
	assert_eq(minimap.size, minimap.MINIMAP_SIZE)
	assert_true(minimap.clip_contents)
	assert_eq(minimap.mouse_filter, Control.MOUSE_FILTER_IGNORE)
	assert_not_null(minimap.get_theme_stylebox("panel"))
	assert_eq(map.get_parent(), minimap)
	assert_eq(map.anchor_left, 0.0)
	assert_eq(map.anchor_top, 0.0)
	assert_eq(map.anchor_right, 1.0)
	assert_eq(map.anchor_bottom, 1.0)
	assert_eq(map.mouse_filter, Control.MOUSE_FILTER_IGNORE)


func test_minimap_view_stays_centered_on_player_between_chunk_snapshot_updates() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_minimap()
	overlay.set_chunk_snapshot(
		[],
		Rect2i(Vector2i(-128, -128), Vector2i(256, 256)),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i(-128, -128), Vector2i(256, 256))
	)
	overlay.set_player_position(Vector3(10.25, 0.0, -5.75))
	var first_bounds: Rect2i = overlay.current_tile_bounds()
	var first_center: Vector2 = overlay.map_center_for_tests()

	overlay.set_player_position(Vector3(10.75, 0.0, -5.25))
	var moved_bounds: Rect2i = overlay.current_tile_bounds()
	var moved_center: Vector2 = overlay.map_center_for_tests()

	assert_eq(first_center, Vector2(10.25, -5.75))
	assert_eq(moved_center, Vector2(10.75, -5.25))
	assert_eq(first_bounds.size, Vector2i(overlay.MINIMAP_VIEW_RADIUS_TILES * 2, overlay.MINIMAP_VIEW_RADIUS_TILES * 2))
	assert_eq(moved_bounds.size, first_bounds.size)
	assert_true(first_bounds.has_point(Vector2i(10, -6)))
	assert_true(moved_bounds.has_point(Vector2i(11, -5)))


func test_map_overlay_repeated_fullscreen_open_does_not_request_redraw() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_player_position(Vector3.ZERO)
	overlay.set_fullscreen_open(true)
	var redraw_count: int = overlay.redraw_request_count_for_tests()

	overlay.set_fullscreen_open(true)

	assert_eq(overlay.redraw_request_count_for_tests(), redraw_count)


func test_map_overlay_keeps_opaque_map_background_through_detailed_view() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_fullscreen_open(true)
	var half_transition: float = overlay.DETAILED_WORLD_TRANSITION_PIXELS * 0.5

	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE - half_transition)
	assert_eq(overlay.map_background_alpha(), 1.0)
	assert_eq(overlay.chart_layer_alpha(), 1.0)
	assert_true(overlay.should_draw_chart_layer())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE)
	assert_eq(overlay.map_background_alpha(), 1.0)
	assert_eq(overlay.chart_layer_alpha(), 1.0)
	assert_true(overlay.should_draw_chart_layer())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + half_transition)
	assert_eq(overlay.map_background_alpha(), 1.0)
	assert_eq(overlay.chart_layer_alpha(), 1.0)
	assert_true(overlay.should_draw_chart_layer())


func test_map_overlay_does_not_draw_rectangular_background_frame_in_detailed_view() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.anchor_left = 0.0
	overlay.anchor_top = 0.0
	overlay.anchor_right = 0.0
	overlay.anchor_bottom = 0.0
	overlay.size = Vector2(100.0, 100.0)
	overlay.set_fullscreen_open(true)
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + overlay.DETAILED_WORLD_TRANSITION_PIXELS)

	assert_true(overlay.detailed_world_visible())
	assert_eq(overlay.background_regions_for_tests(overlay._current_bounds()).size(), 0)


func test_map_overlay_hides_map_markers_in_detailed_view() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_fullscreen_open(true)
	var half_transition: float = overlay.DETAILED_WORLD_TRANSITION_PIXELS * 0.5

	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE)
	assert_true(overlay.should_draw_map_markers())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + half_transition)
	assert_true(overlay.detailed_world_visible())
	assert_false(overlay.should_draw_map_markers())


func test_map_overlay_resource_selection_refreshes_on_mouse_motion_contract() -> void:
	var main = autofree(MainScript.new())
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_fullscreen_open(true)
	main.map_overlay = overlay

	var before: int = overlay.resource_selection_revision_for_tests()
	main._refresh_map_overlay_resource_selection()

	assert_eq(overlay.resource_selection_revision_for_tests(), before + 1)


func test_map_overlay_uses_precomputed_buildings_key_without_rehashing_buildings() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var buildings: Array = [
		{
			"id": 1,
			"def_id": "basic_mining_drill",
			"footprint": [{"x": 0, "y": 0}],
		},
	]

	overlay.set_chunk_snapshot(
		[],
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		buildings,
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		true,
		"precomputed-buildings"
	)

	assert_eq(overlay.buildings_key_compute_count_for_tests(), 0)


func test_map_overlay_uses_precomputed_chunk_snapshot_key_without_rehashing_chunks() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": Rect2i(Vector2i.ZERO, Vector2i.ONE),
			"signature": "stable",
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
	]

	overlay.set_chunk_snapshot(
		chunks,
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		[],
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i.ONE),
		true,
		"buildings",
		"precomputed-chunks"
	)

	assert_eq(overlay.chunk_sync_key_compute_count_for_tests(), 0)


func test_map_overlay_regular_grid_snapshot_uses_cached_chunk_building_keys() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var chunks := [
		{
			"key": "0:0",
			"chunk": Vector2i.ZERO,
			"bounds": Rect2i(Vector2i.ZERO, Vector2i(4, 4)),
			"signature": "stable-a",
			"tiles": [
				{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
		{
			"key": "1:0",
			"chunk": Vector2i(1, 0),
			"bounds": Rect2i(Vector2i(4, 0), Vector2i(4, 4)),
			"signature": "stable-b",
			"tiles": [
				{"x": 4, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
			],
		},
	]
	var buildings: Array = [
		{"id": 41, "def_id": "basic_miner", "footprint": [{"x": 4, "y": 0}]},
	]

	overlay.set_chunk_snapshot(
		chunks,
		Rect2i(Vector2i.ZERO, Vector2i(8, 4)),
		buildings,
		Vector3.ZERO,
		Rect2i(Vector2i.ZERO, Vector2i(8, 4)),
		false,
		"same-buildings",
		"same-chunks"
	)

	assert_eq(overlay.buildings_key_compute_count_for_tests(), 0)
	assert_eq(overlay.building_grid_chunk_cache_rebuild_count_for_tests(), 1)


func test_map_overlay_controller_sends_only_current_visible_snapshot_to_minimap() -> void:
	var controller = autofree(MapOverlayControllerScript.new())
	var minimap_overlay = add_child_autoqfree(MapOverlayScript.new())
	minimap_overlay.configure_minimap()
	var fullscreen_overlay = add_child_autoqfree(MapOverlayScript.new())
	fullscreen_overlay.configure_fullscreen()
	controller.minimap = minimap_overlay
	controller.map_overlay = fullscreen_overlay
	var explored_snapshot := {
		"chunks": [
			{
				"key": "0:0",
				"bounds": Rect2i(Vector2i.ZERO, Vector2i(2, 1)),
				"tiles": [
					{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
					{"x": 1, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
				],
			},
		],
		"rect": Rect2i(Vector2i.ZERO, Vector2i(2, 1)),
	}
	var visible_snapshot := {
		"chunks": [
			{
				"key": "1:0",
				"bounds": Rect2i(Vector2i(1, 0), Vector2i.ONE),
				"tiles": [
					{"x": 1, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
				],
			},
		],
		"rect": Rect2i(Vector2i(1, 0), Vector2i.ONE),
	}

	controller.apply_chunk_snapshots(explored_snapshot, visible_snapshot, [], Vector3.ZERO)

	assert_eq(minimap_overlay._visible_rect, Rect2i(Vector2i(1, 0), Vector2i.ONE))
	assert_eq(
		minimap_overlay._current_bounds(),
		Rect2i(
			Vector2i(-minimap_overlay.MINIMAP_VIEW_RADIUS_TILES, -minimap_overlay.MINIMAP_VIEW_RADIUS_TILES),
			Vector2i(minimap_overlay.MINIMAP_VIEW_RADIUS_TILES * 2, minimap_overlay.MINIMAP_VIEW_RADIUS_TILES * 2)
		)
	)
	assert_eq(fullscreen_overlay.pending_map_texture_job_count_for_tests(), 0)

	fullscreen_overlay.set_fullscreen_open(true)
	controller.apply_chunk_snapshots(explored_snapshot, visible_snapshot, [], Vector3.ZERO)
	assert_gt(fullscreen_overlay.pending_map_texture_job_count_for_tests(), 0)


func test_map_overlay_controller_skips_explored_snapshot_while_fullscreen_map_is_closed() -> void:
	var hud = add_child_autoqfree(Control.new())
	var environment = add_child_autoqfree(SnapshotCountingEnvironment.new())
	var controller = add_child_autoqfree(MapOverlayControllerScript.new())
	var player = add_child_autoqfree(Node3D.new())
	var hotbar_control = autofree(Control.new())
	controller.setup(hud, environment, NeptuneSim.new(), player, hotbar_control)
	controller.buildings_dirty = false

	controller.update_snapshots(true)

	assert_eq(environment.visible_chunk_snapshot_calls, 1)
	assert_eq(environment.explored_chunk_snapshot_calls, 0)
	assert_eq(environment.explored_chunk_snapshot_for_rect_calls, 0)

	controller.map_overlay.set_fullscreen_open(true)
	controller.map_overlay.anchor_left = 0.0
	controller.map_overlay.anchor_top = 0.0
	controller.map_overlay.anchor_right = 0.0
	controller.map_overlay.anchor_bottom = 0.0
	controller.map_overlay.size = Vector2(64.0, 64.0)
	controller.map_overlay.set_pixels_per_tile(16.0)
	controller.mark_dirty()
	controller.update_snapshots(true)

	assert_eq(environment.visible_chunk_snapshot_calls, 1)
	assert_eq(environment.explored_chunk_snapshot_calls, 0)
	assert_eq(environment.explored_chunk_snapshot_for_rect_calls, 1)
	assert_eq(environment.last_explored_chunk_snapshot_rect, Rect2i(Vector2i(-3, -3), Vector2i(6, 6)))


func test_map_overlay_controller_scene_owns_minimap_and_fullscreen_overlay_nodes() -> void:
	var hud = add_child_autoqfree(Control.new())
	var environment = add_child_autoqfree(SnapshotCountingEnvironment.new())
	var controller = add_child_autoqfree(MapOverlayControllerScene.instantiate())
	var player = add_child_autoqfree(Node3D.new())
	var hotbar_control = autofree(Control.new())

	controller.setup(hud, environment, NeptuneSim.new(), player, hotbar_control)

	assert_eq(controller.minimap, controller.get_node("Minimap"))
	assert_eq(controller.map_overlay, controller.get_node("MapOverlay"))
	assert_eq(controller.minimap.get_parent(), controller)
	assert_eq(controller.map_overlay.get_parent(), controller)
	assert_false(controller.map_overlay.is_fullscreen_open())
	assert_false(controller.map_overlay.visible)
	assert_true(controller.minimap.visible)


func test_map_overlay_controller_does_not_snapshot_when_fullscreen_view_changes() -> void:
	var hud = add_child_autoqfree(Control.new())
	var environment = add_child_autoqfree(SnapshotCountingEnvironment.new())
	var controller = add_child_autoqfree(MapOverlayControllerScript.new())
	var player = add_child_autoqfree(Node3D.new())
	var hotbar_control = autofree(Control.new())
	controller.setup(hud, environment, NeptuneSim.new(), player, hotbar_control)
	controller.buildings_dirty = false
	controller.update_snapshots(true)
	controller.map_overlay.set_fullscreen_open(true)
	controller.map_snapshot_dirty = false
	controller.map_snapshot_update_cooldown = 0.0
	environment.visible_chunk_snapshot_calls = 0
	environment.explored_chunk_snapshot_for_rect_calls = 0

	controller.map_overlay.drag_by(Vector2(8.0, 0.0))

	assert_false(controller.map_snapshot_dirty)
	assert_eq(controller.map_snapshot_update_cooldown, 0.0)
	controller.process(controller.MAP_SNAPSHOT_UPDATE_INTERVAL_SEC * 0.5)
	assert_eq(environment.visible_chunk_snapshot_calls, 0)
	assert_eq(environment.explored_chunk_snapshot_for_rect_calls, 0)

	controller.map_overlay.zoom_by(1.25)
	controller.process(controller.MAP_SNAPSHOT_UPDATE_INTERVAL_SEC)

	assert_eq(environment.visible_chunk_snapshot_calls, 0)
	assert_eq(environment.explored_chunk_snapshot_for_rect_calls, 0)
	assert_false(controller.map_snapshot_dirty)


func test_map_overlay_controller_does_not_resnapshot_fullscreen_map_inside_same_chunk_rect() -> void:
	var hud = add_child_autoqfree(Control.new())
	var environment = add_child_autoqfree(SnapshotCountingEnvironment.new())
	environment.chunk_grid_size = Vector2i(32, 32)
	var controller = add_child_autoqfree(MapOverlayControllerScript.new())
	var player = add_child_autoqfree(Node3D.new())
	var hotbar_control = autofree(Control.new())
	controller.setup(hud, environment, NeptuneSim.new(), player, hotbar_control)
	controller.buildings_dirty = false
	controller.map_overlay.set_fullscreen_open(true)
	controller.map_overlay.anchor_left = 0.0
	controller.map_overlay.anchor_top = 0.0
	controller.map_overlay.anchor_right = 0.0
	controller.map_overlay.anchor_bottom = 0.0
	controller.map_overlay.size = Vector2(64.0, 64.0)
	controller.map_overlay.set_pixels_per_tile(16.0)
	controller.update_snapshots(true)
	controller.map_snapshot_dirty = false
	controller.map_snapshot_update_cooldown = 0.0
	environment.visible_chunk_snapshot_calls = 0
	environment.explored_chunk_snapshot_for_rect_calls = 0

	controller.map_overlay.drag_by(Vector2(16.0, 0.0))
	controller.process(controller.MAP_SNAPSHOT_UPDATE_INTERVAL_SEC)

	assert_eq(environment.visible_chunk_snapshot_calls, 0)
	assert_eq(environment.explored_chunk_snapshot_for_rect_calls, 0)
	assert_false(controller.map_snapshot_dirty)


func test_map_overlay_controller_does_not_resnapshot_fullscreen_map_after_leaving_buffered_chunk_rect() -> void:
	var hud = add_child_autoqfree(Control.new())
	var environment = add_child_autoqfree(SnapshotCountingEnvironment.new())
	environment.chunk_grid_size = Vector2i(32, 32)
	var controller = add_child_autoqfree(MapOverlayControllerScript.new())
	var player = add_child_autoqfree(Node3D.new())
	var hotbar_control = autofree(Control.new())
	controller.setup(hud, environment, NeptuneSim.new(), player, hotbar_control)
	controller.buildings_dirty = false
	controller.map_overlay.set_fullscreen_open(true)
	controller.map_overlay.anchor_left = 0.0
	controller.map_overlay.anchor_top = 0.0
	controller.map_overlay.anchor_right = 0.0
	controller.map_overlay.anchor_bottom = 0.0
	controller.map_overlay.size = Vector2(64.0, 64.0)
	controller.map_overlay.set_pixels_per_tile(16.0)
	controller.update_snapshots(true)
	controller.map_snapshot_dirty = false
	controller.map_snapshot_update_cooldown = 0.0
	environment.visible_chunk_snapshot_calls = 0
	environment.explored_chunk_snapshot_for_rect_calls = 0

	controller.map_overlay.drag_by(Vector2(1024.0, 0.0))
	controller.process(controller.MAP_SNAPSHOT_UPDATE_INTERVAL_SEC)

	assert_eq(environment.visible_chunk_snapshot_calls, 0)
	assert_eq(environment.explored_chunk_snapshot_for_rect_calls, 0)
	assert_false(controller.map_snapshot_dirty)


func test_fullscreen_map_left_mouse_drag_pans_without_following_player() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.anchor_left = 0.0
	overlay.anchor_top = 0.0
	overlay.anchor_right = 0.0
	overlay.anchor_bottom = 0.0
	overlay.size = Vector2(100.0, 100.0)
	overlay.set_pixels_per_tile(10.0)
	overlay.set_player_position(Vector3(10.0, 0.0, 20.0))
	overlay.set_fullscreen_open(true)

	overlay.drag_by(Vector2(20.0, 10.0))
	var dragged_center: Vector2 = overlay.map_center_for_tests()
	overlay.set_player_position(Vector3(30.0, 0.0, 40.0))

	assert_eq(dragged_center, Vector2(12.0, 21.0))
	assert_eq(overlay.map_center_for_tests(), dragged_center)
	overlay.center_on_player()
	assert_eq(overlay.map_center_for_tests(), Vector2(30.0, 40.0))


func test_fullscreen_map_drag_and_zoom_emit_view_change_for_immediate_camera_sync() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_pixels_per_tile(10.0)
	overlay.set_player_position(Vector3.ZERO)
	overlay.set_fullscreen_open(true)
	watch_signals(overlay)

	overlay.drag_by(Vector2(5.0, 0.0))
	overlay.zoom_by(1.25)

	assert_signal_emit_count(overlay, "view_changed", 2)


func test_fullscreen_map_follow_player_emits_view_change_only_when_bounds_change() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.anchor_left = 0.0
	overlay.anchor_top = 0.0
	overlay.anchor_right = 0.0
	overlay.anchor_bottom = 0.0
	overlay.size = Vector2(100.0, 100.0)
	overlay.set_pixels_per_tile(10.0)
	overlay.set_player_position(Vector3.ZERO)
	overlay.set_fullscreen_open(true)
	watch_signals(overlay)
	var initial_bounds: Rect2i = overlay.current_tile_bounds()

	overlay.set_player_position(Vector3(0.1, 0.0, 0.1))

	assert_eq(overlay.current_tile_bounds(), initial_bounds)
	assert_signal_emit_count(overlay, "view_changed", 0)

	overlay.set_player_position(Vector3(1.0, 0.0, 0.0))

	assert_ne(overlay.current_tile_bounds(), initial_bounds)
	assert_signal_emit_count(overlay, "view_changed", 1)


func test_fullscreen_map_mouse_motion_is_handled_by_overlay_not_main_input() -> void:
	var main = autofree(MainScript.new())
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_pixels_per_tile(10.0)
	overlay.set_player_position(Vector3.ZERO)
	overlay.set_fullscreen_open(true)
	main.map_overlay = overlay
	watch_signals(overlay)
	var drag_event := InputEventMouseMotion.new()
	drag_event.relative = Vector2(10.0, 0.0)
	drag_event.button_mask = MOUSE_BUTTON_MASK_LEFT

	main._input(drag_event)
	assert_eq(overlay.map_center_for_tests(), Vector2.ZERO)
	assert_signal_emit_count(overlay, "view_changed", 0)

	overlay._gui_input(drag_event)
	assert_eq(overlay.map_center_for_tests(), Vector2(1.0, 0.0))
	assert_signal_emit_count(overlay, "view_changed", 1)

	var zoom_event := InputEventMouseButton.new()
	zoom_event.button_index = MOUSE_BUTTON_WHEEL_UP
	zoom_event.pressed = true
	main._input(zoom_event)
	assert_eq(overlay.pixels_per_tile(), 10.0)
	assert_signal_emit_count(overlay, "view_changed", 1)

	overlay._gui_input(zoom_event)
	assert_eq(overlay.pixels_per_tile(), 12.5)
	assert_signal_emit_count(overlay, "view_changed", 2)


func test_fullscreen_map_drag_applies_fractional_texture_offset_before_bounds_change() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.anchor_left = 0.0
	overlay.anchor_top = 0.0
	overlay.anchor_right = 0.0
	overlay.anchor_bottom = 0.0
	overlay.size = Vector2(100.0, 100.0)
	overlay.set_pixels_per_tile(10.0)
	overlay.set_player_position(Vector3.ZERO)
	overlay.set_fullscreen_open(true)
	var bounds: Rect2i = overlay._current_bounds()
	var before_rect: Rect2 = overlay.tile_region_local_rect_for_tests(bounds, bounds, 10.0)

	overlay.drag_by(Vector2(4.0, 0.0))
	var after_bounds: Rect2i = overlay._current_bounds()
	var after_rect: Rect2 = overlay.tile_region_local_rect_for_tests(bounds, after_bounds, 10.0)

	assert_eq(after_bounds, bounds)
	assert_eq(after_rect.position.x, before_rect.position.x + 4.0)
	assert_eq(after_rect.position.y, before_rect.position.y)


func test_main_disables_3d_rendering_while_fullscreen_map_is_schematic() -> void:
	var main = autofree(MainScript.new())
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	main.map_overlay = overlay

	assert_true(main._should_render_3d_world())
	overlay.set_fullscreen_open(true)
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE)
	assert_false(main._should_render_3d_world())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + overlay.DETAILED_WORLD_TRANSITION_PIXELS)
	assert_true(main._should_render_3d_world())
	overlay.set_fullscreen_open(false)
	assert_true(main._should_render_3d_world())


func test_fullscreen_map_tracks_player_center_for_detailed_camera_alignment() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.anchor_left = 0.0
	overlay.anchor_top = 0.0
	overlay.anchor_right = 0.0
	overlay.anchor_bottom = 0.0
	overlay.size = Vector2(100.0, 100.0)
	overlay.set_pixels_per_tile(10.0)
	overlay.set_fullscreen_open(true)

	overlay.set_player_position(Vector3(10.0, 0.0, 20.0))
	var first_bounds: Rect2i = overlay._current_bounds()
	overlay.set_player_position(Vector3(14.0, 0.0, 24.0))
	var moved_bounds: Rect2i = overlay._current_bounds()

	assert_eq(first_bounds.position, Vector2i(5, 15))
	assert_eq(moved_bounds.position, Vector2i(9, 19))


func test_map_overlay_uses_nearest_texture_filter_for_crisp_schematic_pixels() -> void:
	var fullscreen_overlay = autofree(MapOverlayScript.new())
	fullscreen_overlay.configure_fullscreen()
	var minimap_overlay = autofree(MapOverlayScript.new())
	minimap_overlay.configure_minimap()

	assert_eq(fullscreen_overlay.texture_filter, CanvasItem.TEXTURE_FILTER_NEAREST)
	assert_eq(minimap_overlay.texture_filter, CanvasItem.TEXTURE_FILTER_NEAREST)


func test_map_overlay_uses_item_catalog_color_for_modded_resource_schematic() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()

	var color: Color = overlay.resource_color_for_tests("tin_ore", Color.GREEN)

	assert_eq(color, Color.from_string("#8A8F91", Color.BLACK))


func test_fullscreen_map_schematic_mirrors_x_to_match_top_down_camera() -> void:
	var bounds := Rect2i(Vector2i(-1, 0), Vector2i(3, 1))
	var tiles := [
		{"x": -1, "y": 0, "terrain": "ground", "resource": "copper_ore", "amount": 1, "render": true},
		{"x": 1, "y": 0, "terrain": "ground", "resource": "iron_ore", "amount": 1, "render": true},
	]
	var chunks := [{"key": "0:0", "bounds": bounds, "tiles": tiles}]
	var fullscreen_overlay = add_child_autoqfree(MapOverlayScript.new())
	fullscreen_overlay.configure_fullscreen()
	var fullscreen_image: Image = await set_chunk_snapshot_and_wait_for_image(fullscreen_overlay, chunks, bounds)
	var minimap_overlay = add_child_autoqfree(MapOverlayScript.new())
	minimap_overlay.configure_minimap()
	var minimap_image: Image = await set_chunk_snapshot_and_wait_for_image(minimap_overlay, chunks, bounds)

	assert_eq(fullscreen_overlay._tile_to_local(Vector2i(1, 0), bounds, 10.0), Vector2.ZERO)
	assert_eq(fullscreen_overlay._local_to_tile(Vector2(5.0, 5.0), bounds, 10.0), Vector2i(1, 0))
	assert_eq(minimap_overlay._tile_to_local(Vector2i(-1, 0), bounds, 10.0), Vector2.ZERO)
	assert_eq(minimap_overlay._local_to_tile(Vector2(5.0, 5.0), bounds, 10.0), Vector2i(-1, 0))
	assert_true(_color_almost_eq(
		fullscreen_image.get_pixel(0, 0),
		fullscreen_overlay.resource_color_for_tests("iron_ore", Color.BLACK).lerp(Color.WHITE, 0.08),
		0.004
	))
	assert_true(_color_almost_eq(
		minimap_image.get_pixel(0, 0),
		minimap_overlay.resource_color_for_tests("copper_ore", Color.BLACK).lerp(Color.WHITE, 0.08),
		0.004
	))


func test_minimap_schematic_does_not_flip_vertical_axis() -> void:
	var bounds := Rect2i(Vector2i(0, -1), Vector2i(1, 3))
	var tiles := [
		{"x": 0, "y": -1, "terrain": "ground", "resource": "copper_ore", "amount": 1, "render": true},
		{"x": 0, "y": 1, "terrain": "ground", "resource": "iron_ore", "amount": 1, "render": true},
	]
	var chunks := [{"key": "0:0", "bounds": bounds, "tiles": tiles}]
	var fullscreen_overlay = add_child_autoqfree(MapOverlayScript.new())
	fullscreen_overlay.configure_fullscreen()
	var fullscreen_image: Image = await set_chunk_snapshot_and_wait_for_image(fullscreen_overlay, chunks, bounds)
	var minimap_overlay = add_child_autoqfree(MapOverlayScript.new())
	minimap_overlay.configure_minimap()
	var minimap_image: Image = await set_chunk_snapshot_and_wait_for_image(minimap_overlay, chunks, bounds)

	assert_eq(fullscreen_overlay._tile_to_local(Vector2i(0, 1), bounds, 10.0), Vector2.ZERO)
	assert_eq(fullscreen_overlay._local_to_tile(Vector2(5.0, 5.0), bounds, 10.0), Vector2i(0, 1))
	assert_eq(minimap_overlay._tile_to_local(Vector2i(0, -1), bounds, 10.0), Vector2.ZERO)
	assert_eq(minimap_overlay._local_to_tile(Vector2(5.0, 5.0), bounds, 10.0), Vector2i(0, -1))
	assert_true(_color_almost_eq(
		fullscreen_image.get_pixel(0, 0),
		fullscreen_overlay.resource_color_for_tests("iron_ore", Color.BLACK).lerp(Color.WHITE, 0.08),
		0.004
	))
	assert_true(_color_almost_eq(
		minimap_image.get_pixel(0, 0),
		minimap_overlay.resource_color_for_tests("copper_ore", Color.BLACK).lerp(Color.WHITE, 0.08),
		0.004
	))


func test_map_overlay_schematic_texture_leaves_unexplored_tiles_transparent() -> void:
	var overlay = add_child_autoqfree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i(2, 1))
	var chunks := [{
		"key": "0:0",
		"bounds": bounds,
		"tiles": [
			{"x": 1, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
		],
	}]

	var image: Image = await set_chunk_snapshot_and_wait_for_image(overlay, chunks, bounds)
	var explored := image.get_pixel(0, 0)
	var unexplored := image.get_pixel(1, 0)

	assert_eq(unexplored.a, 0.0)
	assert_gt(explored.a, 0.99)


func test_map_overlay_fades_explored_tiles_outside_current_visible_rect() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var visible_rect := Rect2i(Vector2i.ZERO, Vector2i.ONE)
	overlay.set_chunk_snapshot([], Rect2i(Vector2i.ZERO, Vector2i(2, 1)), [], Vector3.ZERO, visible_rect)
	overlay.set_fullscreen_open(true)
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + overlay.DETAILED_WORLD_TRANSITION_PIXELS)

	assert_true(overlay.tile_uses_detailed_world_for_tests(Vector2i(0, 0)))
	assert_false(overlay.tile_uses_detailed_world_for_tests(Vector2i(1, 0)))


func test_map_overlay_detailed_view_tracks_current_visible_tiles_without_texture_rebuild() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var visible_rect := Rect2i(Vector2i.ZERO, Vector2i.ONE)
	overlay.set_chunk_snapshot([], Rect2i(Vector2i.ZERO, Vector2i(2, 1)), [], Vector3.ZERO, visible_rect)
	overlay.set_fullscreen_open(true)
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + overlay.DETAILED_WORLD_TRANSITION_PIXELS)

	assert_true(overlay.tile_uses_detailed_world_for_tests(Vector2i(0, 0)))
	assert_false(overlay.tile_uses_detailed_world_for_tests(Vector2i(1, 0)))


