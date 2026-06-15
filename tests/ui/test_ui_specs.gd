extends "res://tests/ui/ui_test_base.gd"

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
	var slot = add_child_autoqfree(InventorySlotScene.instantiate())
	var source_ref := {"kind": "player", "slot": 1}
	slot.configure(source_ref, "", 0, null)

	assert_null(slot._get_drag_data(Vector2.ZERO))
	assert_false(slot._can_drop_data(Vector2.ZERO, {
		"type": "inventory_slot",
		"source": source_ref,
		"item": "iron_ore",
		"amount": 10,
	}))


func test_inventory_slot_scene_updates_icon_amount_and_tooltip_from_configure() -> void:
	var slot = add_child_autoqfree(InventorySlotScene.instantiate())
	var image := Image.create_empty(1, 1, false, Image.FORMAT_RGBA8)
	image.fill(Color.WHITE)
	var texture := ImageTexture.create_from_image(image)

	slot.configure({"kind": "player", "slot": 1}, "iron_ore", 12, texture)

	var icon := slot.get_node("Icon") as TextureRect
	var amount_label := slot.get_node("AmountLabel") as Label
	assert_eq(icon.texture, texture)
	assert_true(icon.visible)
	assert_eq(icon.tooltip_text, "Iron ore")
	assert_eq(amount_label.text, "12")
	assert_true(amount_label.visible)

	slot.configure({"kind": "player", "slot": 1}, "", 0, null)

	assert_null(icon.texture)
	assert_false(icon.visible)
	assert_eq(amount_label.text, "")
	assert_false(amount_label.visible)


func test_inventory_slot_emits_transfer_for_valid_drop() -> void:
	var target = add_child_autoqfree(InventorySlotScene.instantiate())
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
	var slot = add_child_autoqfree(InventorySlotScene.instantiate())
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
