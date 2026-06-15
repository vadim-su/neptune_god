extends "res://addons/gut/test.gd"

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const CatalogSelectorScript := preload("res://game/ui/catalog_selector.gd")
const HotbarScript := preload("res://game/ui/hotbar.gd")
const InventorySlotScript := preload("res://game/ui/inventory_slot.gd")
const MapOverlayScript := preload("res://game/ui/map_overlay.gd")
const DevConsoleControllerScript := preload("res://game/main/dev_console_controller.gd")
const InventoryControllerScript := preload("res://game/main/inventory_controller.gd")
const MapOverlayControllerScript := preload("res://game/main/map_overlay_controller.gd")
const MainScript := preload("res://game/main/main.gd")
const FpsCounterScript := preload("res://game/ui/fps_counter.gd")
const FpsCounterScene := preload("res://game/ui/fps_counter.tscn")
const PlayerZoneOverlayScene := preload("res://game/main/player_zone_overlay.tscn")
const MapOverlayControllerScene := preload("res://game/main/map_overlay_controller.tscn")
const BuildGhostScene := preload("res://game/main/build_ghost.tscn")

const GROUP_CATEGORY := 0
const GROUP_USAGE := 1
const GROUP_NAME := 2


class SnapshotCountingEnvironment:
	extends Node

	var visible_chunk_snapshot_calls := 0
	var explored_chunk_snapshot_calls := 0
	var explored_chunk_snapshot_for_rect_calls := 0
	var last_explored_chunk_snapshot_rect := Rect2i()
	var chunk_grid_size := Vector2i.ONE

	func visible_chunk_snapshot() -> Dictionary:
		visible_chunk_snapshot_calls += 1
		return {
			"chunks": [
				{
					"key": "0:0",
					"bounds": Rect2i(Vector2i.ZERO, Vector2i.ONE),
					"tiles": [
						{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
					],
				},
			],
			"rect": Rect2i(Vector2i.ZERO, Vector2i.ONE),
		}

	func explored_chunk_snapshot() -> Dictionary:
		explored_chunk_snapshot_calls += 1
		return {
			"chunks": [
				{
					"key": "0:0",
					"bounds": Rect2i(Vector2i.ZERO, Vector2i.ONE),
					"tiles": [
						{"x": 0, "y": 0, "terrain": "ground", "resource": "", "amount": 0, "render": true},
					],
				},
			],
			"rect": Rect2i(Vector2i.ZERO, Vector2i.ONE),
		}

	func explored_chunk_snapshot_for_rect(tile_rect: Rect2i) -> Dictionary:
		explored_chunk_snapshot_for_rect_calls += 1
		last_explored_chunk_snapshot_rect = tile_rect
		return {
			"chunks": [
				{
					"key": "scoped",
					"bounds": tile_rect,
					"tiles": [
						{
							"x": tile_rect.position.x,
							"y": tile_rect.position.y,
							"terrain": "ground",
							"resource": "",
							"amount": 0,
							"render": true,
						},
					],
				},
			],
			"rect": tile_rect,
		}

	func chunk_snapshot_grid_size() -> Vector2i:
		return chunk_grid_size


class ToggleVisibilityProvider:
	var visible := true

	func should_show() -> bool:
		return visible


class FakeDevConsole:
	extends Node

	signal command_submitted(line: String)

	var outputs: Array[String] = []
	var completions: Array = []
	var clear_count := 0

	func append_output(line: String) -> void:
		outputs.append(line)

	func append_lines(lines: Array) -> void:
		for line: Variant in lines:
			outputs.append(str(line))

	func clear_scrollback() -> void:
		clear_count += 1
		outputs.clear()

	func set_completions(values: Array) -> void:
		completions = values.duplicate()


class FakeDevConsoleSim:
	extends RefCounted

	var give_calls: Array[Dictionary] = []

	func core_tick() -> int:
		return 12

	func digest() -> int:
		return 34

	func building_count() -> int:
		return 2

	func give_item(item_id: String, amount: int) -> bool:
		give_calls.append({"item": item_id, "amount": amount})
		return item_id == "iron_ore"


class FakeInventoryWindow:
	extends Node

	signal slot_transfer_requested(from_ref: Dictionary, to_ref: Dictionary, amount: int)
	signal slot_action_requested(slot_ref: Dictionary, action: String)

	var open := false
	var hide_count := 0
	var show_calls: Array[Dictionary] = []
	var update_calls: Array[Dictionary] = []

	func is_open() -> bool:
		return open

	func hide_window() -> void:
		open = false
		hide_count += 1

	func show_inventory(inventory_snapshot: Dictionary, selected_object: Dictionary, object_ui_snapshot: Dictionary) -> void:
		open = true
		show_calls.append({
			"inventory": inventory_snapshot,
			"selected": selected_object,
			"object_ui": object_ui_snapshot,
		})

	func update_inventory(inventory_snapshot: Dictionary, selected_object: Dictionary, object_ui_snapshot: Dictionary) -> void:
		update_calls.append({
			"inventory": inventory_snapshot,
			"selected": selected_object,
			"object_ui": object_ui_snapshot,
		})


class FakeMachineWindow:
	extends Node

	var hide_count := 0

	func hide_window() -> void:
		hide_count += 1


class FakeInventorySim:
	extends RefCounted

	var transfer_calls: Array[Dictionary] = []
	var action_calls: Array[Dictionary] = []

	func inventory_snapshot() -> Dictionary:
		return {"player_slots": [{"item": "iron_ore", "amount": 5}]}

	func building_ui_snapshot(building_id: int) -> Dictionary:
		return {"building_id": building_id}

	func transfer_inventory_slot(from_ref: Dictionary, to_ref: Dictionary, amount: int) -> bool:
		transfer_calls.append({"from": from_ref, "to": to_ref, "amount": amount})
		return amount > 0

	func click_inventory_slot(slot_ref: Dictionary, action: String) -> bool:
		action_calls.append({"slot": slot_ref, "action": action})
		return action != "reject"


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


func set_chunk_snapshot_and_wait_for_image(
	overlay: Control,
	chunks: Array,
	bounds: Rect2i,
	current_visible_rect := Rect2i()
) -> Image:
	overlay.set_chunk_snapshot(chunks, bounds, [], Vector3.ZERO, current_visible_rect)
	await wait_until(
		func() -> bool:
			return overlay.uploaded_map_chunk_texture_count_for_tests() > 0,
		1.0
	)
	return overlay.uploaded_map_chunk_image_for_tests(str(chunks[0]["key"]))


func test_dev_console_controller_handles_status_items_and_unknown_commands() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var refresh_calls: Array = []
	var selected_snapshot := {
		"id": 42,
		"object": {"def_id": "basic_miner"},
	}
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			refresh_calls.append(true),
		func() -> Dictionary:
			return selected_snapshot
	)

	controller.submit_command("status")
	controller.submit_command("items")
	controller.submit_command("missing")

	assert_eq(console.outputs[0], "tick=12 digest=34 buildings=2 selected=basic_miner #42")
	assert_true((console.outputs[1] as String).contains("iron_ore"))
	assert_true((console.outputs[1] as String).contains("tin_ore"))
	assert_eq(console.outputs[2], "Unknown command 'missing'. Use 'help' for commands.")
	assert_true(console.completions.has("give"))
	assert_true(console.completions.has("iron_ore"))
	assert_eq(refresh_calls.size(), 0)


func test_dev_console_controller_give_command_refreshes_open_inventory() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	inventory.open = true
	var refresh_calls: Array = []
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			refresh_calls.append(true),
		func() -> Dictionary:
			return {}
	)

	controller.submit_command("give iron_ore 3")
	controller.submit_command("give missing 2")

	assert_eq(sim.give_calls.size(), 2)
	assert_eq(sim.give_calls[0], {"item": "iron_ore", "amount": 3})
	assert_eq(console.outputs[0], "Added iron_ore x3")
	assert_eq(console.outputs[1], "Could not add missing x2")
	assert_eq(refresh_calls.size(), 1)


func test_inventory_controller_toggle_shows_inventory_with_selected_object_snapshot() -> void:
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var machine = add_child_autoqfree(FakeMachineWindow.new())
	var sim := FakeInventorySim.new()
	var controller = autofree(InventoryControllerScript.new())
	controller.setup(
		inventory,
		machine,
		sim,
		func() -> Dictionary:
			return {"id": 42, "object": {"id": 42, "def_id": "basic_miner"}}
	)

	controller.toggle()

	assert_true(inventory.is_open())
	assert_eq(machine.hide_count, 1)
	assert_eq(inventory.update_calls.size(), 1)
	assert_eq(inventory.show_calls.size(), 1)
	assert_eq(inventory.show_calls[0]["selected"], {"id": 42, "def_id": "basic_miner"})
	assert_eq(inventory.show_calls[0]["object_ui"], {"building_id": 42})

	controller.toggle()

	assert_false(inventory.is_open())
	assert_eq(inventory.hide_count, 1)


func test_inventory_controller_updates_after_successful_slot_transfer_and_action() -> void:
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var machine = add_child_autoqfree(FakeMachineWindow.new())
	var sim := FakeInventorySim.new()
	var controller = autofree(InventoryControllerScript.new())
	controller.setup(
		inventory,
		machine,
		sim,
		func() -> Dictionary:
			return {}
	)

	inventory.slot_transfer_requested.emit({"kind": "player"}, {"kind": "building"}, 4)
	inventory.slot_transfer_requested.emit({"kind": "player"}, {"kind": "building"}, 0)
	inventory.slot_action_requested.emit({"kind": "player"}, "stack")
	inventory.slot_action_requested.emit({"kind": "player"}, "reject")

	assert_eq(sim.transfer_calls.size(), 2)
	assert_eq(sim.action_calls.size(), 2)
	assert_eq(inventory.update_calls.size(), 2)


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

	assert_eq(minimap_overlay._current_bounds(), Rect2i(Vector2i(1, 0), Vector2i.ONE))
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


func test_main_keeps_hotbar_above_fullscreen_map_overlay() -> void:
	var main = autofree(MainScript.new())
	var hotbar_control = autofree(Control.new())
	var fullscreen_overlay = autofree(MapOverlayScript.new())
	fullscreen_overlay.configure_fullscreen()
	main.hotbar = hotbar_control
	main.map_overlay = fullscreen_overlay

	main._keep_hotbar_above_map_overlay()

	assert_gt(hotbar_control.z_index, fullscreen_overlay.z_index)


func test_fps_counter_keeps_itself_above_fullscreen_map_overlay_without_blocking_input() -> void:
	var fps_counter = autofree(FpsCounterScript.new())
	var fullscreen_overlay = autofree(MapOverlayScript.new())
	fullscreen_overlay.configure_fullscreen()

	fps_counter.keep_above_control(fullscreen_overlay)

	assert_gt(fps_counter.z_index, fullscreen_overlay.z_index)


func test_main_scene_has_top_right_fps_label() -> void:
	var main_scene = load("res://game/main/main.tscn").instantiate()
	var fps_counter := main_scene.get_node("Hud/FpsCounter") as Control
	var scene_fps_label := main_scene.get_node("Hud/FpsCounter/FpsLabel") as Label

	assert_not_null(fps_counter)
	assert_not_null(scene_fps_label)
	assert_eq(scene_fps_label.text, "FPS: 0")
	assert_eq(fps_counter.anchor_left, 1.0)
	assert_eq(fps_counter.anchor_right, 1.0)
	assert_eq(fps_counter.mouse_filter, Control.MOUSE_FILTER_IGNORE)
	assert_eq(scene_fps_label.mouse_filter, Control.MOUSE_FILTER_IGNORE)

	main_scene.free()


func test_fps_counter_scene_updates_fps_label_text() -> void:
	var fps_counter = autofree(FpsCounterScene.instantiate())
	add_child(fps_counter)

	fps_counter.update_value(59.6)

	assert_eq(fps_counter.label.text, "FPS: 60")
	assert_eq(FpsCounterScript.format_text(12.2), "FPS: 12")


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


func test_player_zone_overlay_hides_while_fullscreen_map_is_open() -> void:
	var main = autofree(MainScript.new())
	var overlay = autofree(MapOverlayScript.new())
	overlay.configure_fullscreen()
	main.map_overlay = overlay

	assert_true(main._should_show_player_zone_overlay())
	overlay.set_fullscreen_open(true)
	assert_false(main._should_show_player_zone_overlay())
	overlay.set_fullscreen_open(false)
	assert_true(main._should_show_player_zone_overlay())


func test_player_zone_overlay_scene_follows_player_and_uses_visibility_provider() -> void:
	var player = add_child_autoqfree(Node3D.new())
	var visibility := ToggleVisibilityProvider.new()
	var zone_overlay = add_child_autoqfree(PlayerZoneOverlayScene.instantiate())
	player.global_position = Vector3(4.0, 0.0, -7.0)

	zone_overlay.setup(player, Callable(visibility, "should_show"))
	zone_overlay.update()

	var zone_mesh := zone_overlay.get_node("ZoneMesh") as MeshInstance3D
	assert_not_null(zone_mesh.mesh)
	assert_true(zone_mesh.visible)
	assert_eq(zone_mesh.global_position, Vector3(4.0, zone_overlay.PLAYER_ZONE_Y, -7.0))

	visibility.visible = false
	zone_overlay.update()

	assert_false(zone_mesh.visible)


func test_build_ghost_scene_renders_and_hides_preview_tiles() -> void:
	var build_ghost = add_child_autoqfree(BuildGhostScene.instantiate())
	var footprint: Array = [
		{"x": 2, "y": -1},
		{"x": 3, "y": -1},
	]

	build_ghost.show_footprint(footprint, true)

	assert_true(build_ghost.visible)
	assert_eq(build_ghost.get_child_count(), 2)
	var first_tile := build_ghost.get_child(0) as MeshInstance3D
	assert_eq(first_tile.position, Vector3(2.0, build_ghost.BUILD_GHOST_Y, -1.0))
	assert_eq((first_tile.material_override as StandardMaterial3D).albedo_color, build_ghost.GHOST_VALID_COLOR)

	build_ghost.show_footprint([{"x": 4, "y": 5}], false)

	assert_eq(build_ghost.get_child_count(), 1)
	var blocked_tile := build_ghost.get_child(0) as MeshInstance3D
	assert_eq(blocked_tile.position, Vector3(4.0, build_ghost.BUILD_GHOST_Y, 5.0))
	assert_eq((blocked_tile.material_override as StandardMaterial3D).albedo_color, build_ghost.GHOST_BLOCKED_COLOR)

	build_ghost.hide_preview()

	assert_false(build_ghost.visible)
	assert_eq(build_ghost.get_child_count(), 0)


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


func _color_luminance(color: Color) -> float:
	return color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722
