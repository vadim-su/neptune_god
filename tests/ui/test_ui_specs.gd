extends "res://addons/gut/test.gd"

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const CatalogSelectorScript := preload("res://game/ui/catalog_selector.gd")
const HotbarScript := preload("res://game/ui/hotbar.gd")
const InventorySlotScript := preload("res://game/ui/inventory_slot.gd")
const MapOverlayScript := preload("res://game/ui/map_overlay.gd")

const GROUP_CATEGORY := 0
const GROUP_USAGE := 1
const GROUP_NAME := 2


func before_each() -> void:
	ItemCatalogScript.load_from_rows([
		{"id": "iron_ore", "display_name": "Iron ore", "color": "#94785C"},
		{"id": "tin_ore", "display_name": "Tin ore", "color": "#8A8F91"},
	])
	BuildingCatalogScript.load_from_rows([
		{"id": "basic_miner", "display_name": "Basic Miner", "ui_type": "Machine"},
		{"id": "basic_belt", "display_name": "Basic Belt", "ui_type": "Transport", "walkable": true},
		{"id": "wooden_chest", "display_name": "Wooden Chest", "ui_type": "Storage"},
	])


func test_catalog_selector_normalizes_buildable_entries_for_display_and_search() -> void:
	var selector = autofree(CatalogSelectorScript.new())
	var entry: Dictionary = selector._normalized_entry({"id": "basic_miner"})

	assert_eq(entry["id"], "basic_miner")
	assert_eq(entry["label"], "Basic Miner")
	assert_eq(entry["kind"], "building")
	assert_eq(entry["category"], "Machine")
	assert_eq(entry["usage_tags"], ["buildable"])
	assert_true(selector._entry_matches(entry, "miner"))
	assert_true(selector._entry_matches(entry, "machine"))
	assert_true(selector._entry_matches(entry, "buildable"))
	assert_false(selector._entry_matches(entry, "furnace"))


func test_catalog_selector_grouping_keeps_other_last_and_name_symbols_under_hash() -> void:
	var selector = autofree(CatalogSelectorScript.new())
	var entries: Array[Dictionary] = [
		{"id": "raw", "label": "Raw", "category": "", "usage_tags": []},
		{"id": "belt", "label": "Basic Belt", "category": "Transport", "usage_tags": ["transport"]},
		{"id": "assembler", "label": "Assembler", "category": "Machine", "usage_tags": ["crafting"]},
	]

	selector.grouping_mode = GROUP_CATEGORY
	var category_groups: Array[Dictionary] = selector._group_entries(entries)
	assert_eq(_group_ids(category_groups), ["machine", "transport", "other"])

	selector.grouping_mode = GROUP_USAGE
	var usage_groups: Array[Dictionary] = selector._group_entries(entries)
	assert_eq(_group_ids(usage_groups), ["crafting", "transport", "other"])

	selector.grouping_mode = GROUP_NAME
	var name_entries: Array[Dictionary] = [
		{"id": "numeric", "label": "3D Printer", "category": "Machine", "usage_tags": []},
		{"id": "assembler", "label": "Assembler", "category": "Machine", "usage_tags": []},
	]
	var name_groups: Array[Dictionary] = selector._group_entries(name_entries)
	assert_eq(_group_ids(name_groups), ["#", "a"])


func test_inventory_slot_rejects_empty_drag_data_and_same_target_drop() -> void:
	var slot = autofree(InventorySlotScript.new())
	var source_ref := {"kind": "player", "slot": 1}
	slot.configure(source_ref, "", 0, null)

	assert_null(slot._get_drag_data(Vector2.ZERO))
	assert_false(slot._can_drop_data(Vector2.ZERO, {
		"type": "inventory_slot",
		"source": source_ref,
		"item": "iron_ore",
		"amount": 10,
	}))


func test_inventory_slot_emits_transfer_for_valid_drop() -> void:
	var target = autofree(InventorySlotScript.new())
	var target_ref := {"kind": "building", "building_id": 42, "role": "Input", "slot": 0}
	var source_ref := {"kind": "player", "slot": 3}
	var captured: Array = []
	target.configure(target_ref, "", 0, null)
	target.transfer_requested.connect(func(from_ref: Dictionary, to_ref: Dictionary, amount: int) -> void:
		captured.append({"from": from_ref, "to": to_ref, "amount": amount})
	)

	target._drop_data(Vector2.ZERO, {
		"type": "inventory_slot",
		"source": source_ref,
		"item": "iron_ore",
		"amount": 12,
	})

	assert_eq(captured.size(), 1)
	assert_eq(captured[0]["from"], source_ref)
	assert_eq(captured[0]["to"], target_ref)
	assert_eq(captured[0]["amount"], 12)


func test_inventory_slot_mouse_actions_express_stack_split_and_single_item_contract() -> void:
	var slot = autofree(InventorySlotScript.new())
	var slot_ref := {"kind": "character", "container": "backpack", "slot": 2}
	var captured: Array = []
	slot.configure(slot_ref, "iron_ore", 10, null)
	slot.action_requested.connect(func(ref: Dictionary, action: String) -> void:
		captured.append({"ref": ref, "action": action})
	)

	var left_click := InputEventMouseButton.new()
	left_click.button_index = MOUSE_BUTTON_LEFT
	left_click.pressed = false
	slot._gui_input(left_click)

	var right_click := InputEventMouseButton.new()
	right_click.button_index = MOUSE_BUTTON_RIGHT
	right_click.pressed = false
	slot._gui_input(right_click)

	assert_eq(captured.size(), 2)
	assert_eq(captured[0], {"ref": slot_ref, "action": "stack"})
	assert_eq(captured[1], {"ref": slot_ref, "action": "one"})


func test_hotbar_number_keys_select_slots_using_factorio_style_zero_for_tenth_slot() -> void:
	var hotbar = autofree(HotbarScript.new())

	assert_eq(hotbar._hotbar_index_for_key(KEY_1), 0)
	assert_eq(hotbar._hotbar_index_for_key(KEY_5), 4)
	assert_eq(hotbar._hotbar_index_for_key(KEY_9), 8)
	assert_eq(hotbar._hotbar_index_for_key(KEY_0), 9)
	assert_eq(hotbar._hotbar_index_for_key(KEY_A), -1)


func test_hotbar_assignment_updates_valid_slots_and_notifies_when_selected_slot_changes() -> void:
	var hotbar = autofree(HotbarScript.new())
	var selected_ids: Array[String] = []
	for index in range(10):
		hotbar.slots.append({})
	hotbar.selected_slot = 2
	hotbar.selected.connect(func(entry_id: String) -> void:
		selected_ids.append(entry_id)
	)

	hotbar.assign_slot(20, {"id": "basic_belt"})
	hotbar.assign_slot(1, {"id": ""})
	assert_eq(hotbar.slots[1], {})

	hotbar.assign_slot(2, {"id": "basic_miner"})
	assert_eq(hotbar.slots[2]["id"], "basic_miner")
	assert_eq(hotbar.slots[2]["label"], "Basic Miner")
	assert_eq(selected_ids, ["basic_miner"])


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


func test_map_overlay_fades_chart_layer_across_detailed_top_down_transition() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_fullscreen_open(true)
	var half_transition: float = overlay.DETAILED_WORLD_TRANSITION_PIXELS * 0.5

	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE - half_transition)
	assert_eq(overlay.chart_layer_alpha(), 1.0)
	assert_true(overlay.should_draw_chart_layer())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE)
	assert_gt(overlay.chart_layer_alpha(), 0.0)
	assert_lt(overlay.chart_layer_alpha(), 1.0)
	assert_true(overlay.should_draw_chart_layer())
	overlay.set_pixels_per_tile(overlay.DETAILED_WORLD_PIXELS_PER_TILE + half_transition)
	assert_eq(overlay.chart_layer_alpha(), 0.0)
	assert_false(overlay.should_draw_chart_layer())


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
	var fullscreen_overlay = autofree(MapOverlayScript.new())
	fullscreen_overlay.configure_fullscreen()
	fullscreen_overlay.set_world_snapshot(tiles, [], bounds, Vector3.ZERO)
	var minimap_overlay = autofree(MapOverlayScript.new())
	minimap_overlay.configure_minimap()
	minimap_overlay.set_world_snapshot(tiles, [], bounds, Vector3.ZERO)

	var fullscreen_image: Image = fullscreen_overlay.schematic_image_for_tests(bounds)
	var minimap_image: Image = minimap_overlay.schematic_image_for_tests(bounds)

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


func test_map_overlay_schematic_texture_has_opaque_background() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_world_snapshot([], [], Rect2i(Vector2i(-1, -1), Vector2i(3, 3)), Vector3.ZERO)

	var image: Image = overlay.schematic_image_for_tests(Rect2i(Vector2i(-1, -1), Vector2i(3, 3)))
	var background := image.get_pixel(0, 0)

	assert_gt(background.a, 0.99)
	assert_ne(background, Color.WHITE)


func test_map_overlay_retains_schematic_texture_after_rebuild() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	overlay.set_world_snapshot([
		{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
	], [], Rect2i(Vector2i.ZERO, Vector2i.ONE), Vector3.ZERO)

	var texture: ImageTexture = overlay.schematic_texture_for_tests(Rect2i(Vector2i.ZERO, Vector2i.ONE))

	assert_not_null(texture)
	assert_same(texture, overlay._schematic_texture)
	assert_eq(texture.get_width(), 1)
	assert_eq(texture.get_height(), 1)


func test_map_overlay_reuses_schematic_texture_until_bounds_or_snapshot_change() -> void:
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	var bounds := Rect2i(Vector2i.ZERO, Vector2i.ONE)
	overlay.set_world_snapshot([
		{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
	], [], bounds, Vector3.ZERO)

	var first_texture: ImageTexture = overlay.schematic_texture_for_tests(bounds)
	var same_texture: ImageTexture = overlay.schematic_texture_for_tests(bounds)
	overlay.set_player_position(Vector3(4.0, 0.0, 3.0))
	var moved_player_texture: ImageTexture = overlay.schematic_texture_for_tests(bounds)

	assert_same(first_texture, same_texture)
	assert_same(first_texture, moved_player_texture)

	var shifted_bounds := Rect2i(Vector2i(1, 0), Vector2i.ONE)
	var shifted_texture: ImageTexture = overlay.schematic_texture_for_tests(shifted_bounds)
	assert_not_same(first_texture, shifted_texture)

	overlay.set_world_snapshot([
		{"x": 0, "y": 0, "terrain": "stone", "resource": "", "amount": 0, "render": true},
	], [], bounds, Vector3.ZERO)
	var changed_snapshot_texture: ImageTexture = overlay.schematic_texture_for_tests(bounds)
	assert_not_same(first_texture, changed_snapshot_texture)


func _group_ids(groups: Array[Dictionary]) -> Array:
	var ids: Array = []
	for group: Dictionary in groups:
		ids.append(str(group.get("id", "")))
	return ids


func _sorted_vec2i(values: Array) -> Array:
	var sorted := values.duplicate()
	sorted.sort_custom(func(left: Vector2i, right: Vector2i) -> bool:
		if left.y == right.y:
			return left.x < right.x
		return left.y < right.y
	)
	return sorted


func _color_almost_eq(left: Color, right: Color, tolerance: float) -> bool:
	return (
		absf(left.r - right.r) <= tolerance
		and absf(left.g - right.g) <= tolerance
		and absf(left.b - right.b) <= tolerance
		and absf(left.a - right.a) <= tolerance
	)
