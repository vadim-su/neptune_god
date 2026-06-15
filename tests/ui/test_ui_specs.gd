extends "res://tests/ui/ui_test_base.gd"

const CatalogRegistryScript := preload("res://bootstrap/catalog_registry.gd")
const BuildingRendererScript := preload("res://game/buildings/building_renderer.gd")


class AssetCatalogPaths:
	const PATHS := {
		"items": ["res://assets/catalog/items.json"],
		"recipes": ["res://assets/catalog/recipes.json"],
		"buildings": ["res://assets/catalog/buildings.json"],
		"terrain": ["res://assets/catalog/terrain.json"],
		"player": ["res://assets/catalog/player.json"],
		"resources": ["res://assets/catalog/resources.json"],
		"worldgen": ["res://assets/catalog/worldgen.json"],
	}

	func catalog_paths(catalog_kind: String) -> Array[String]:
		var paths: Array[String] = []
		for raw_path: Variant in PATHS.get(catalog_kind, []):
			paths.append(str(raw_path))
		return paths


func _load_asset_catalog_registry() -> RefCounted:
	var registry = CatalogRegistryScript.new()
	assert_true(registry.load_from_mod_registry(AssetCatalogPaths.new()))
	return registry


func _configure_asset_sim(sim: NeptuneSim, registry: RefCounted) -> void:
	assert_true(sim.configure_catalogs(
		registry.rows("items"),
		registry.rows("recipes"),
		registry.rows("buildings"),
		registry.rows("terrain"),
		registry.rows("player"),
		registry.rows("resources"),
		registry.rows("worldgen")
	))


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


func test_dev_console_controller_ignores_blank_commands() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	controller.submit_command("")
	controller.submit_command("   ")

	assert_eq(console.outputs, [])
	assert_eq(sim.give_calls.size(), 0)


func test_dev_console_help_lists_commands_as_vertical_bullets() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	controller.submit_command("help")

	assert_eq(console.outputs[0], "commands:")
	assert_true(console.outputs.has("- give <item> <amount> - Add an item stack to the player inventory."))
	assert_true(console.outputs.has("- teleport <x> <z> - Move the player to world coordinates."))
	assert_false((console.outputs[1] as String).begins_with("  "))


func test_dev_console_clear_command_removes_scrollback() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	controller.submit_command("items")
	assert_gt(console.outputs.size(), 0)

	controller.submit_command("clear")

	assert_eq(console.outputs, [])
	assert_eq(console.clear_count, 1)


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


func test_dev_console_give_command_is_case_insensitive_and_clamps_invalid_amount() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	controller.submit_command("GiVe iron_ore nope")
	controller.submit_command("GIVE iron_ore -20")

	assert_eq(sim.give_calls, [
		{"item": "iron_ore", "amount": 1},
		{"item": "iron_ore", "amount": 1},
	])
	assert_eq(console.outputs, [
		"Added iron_ore x1",
		"Added iron_ore x1",
	])


func test_dev_console_controller_teleports_player() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var teleports: Array[Vector3] = []
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {},
		func(position: Vector3) -> void:
			teleports.append(position)
	)

	controller.submit_command("teleport 12.5 -4")
	controller.submit_command("tp nope 8")

	assert_eq(teleports, [Vector3(12.5, 0.0, -4.0)])
	assert_eq(console.outputs[0], "Teleported player to x=12.50 z=-4.00")
	assert_eq(console.outputs[1], "Usage: teleport <x> <z>")
	assert_true(console.completions.has("teleport"))
	assert_true(console.completions.has("tp"))


func test_dev_console_teleport_reports_when_player_callback_is_missing() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	controller.submit_command("TP 2 3")

	assert_eq(console.outputs, ["Teleport is not available."])


func test_dev_console_controller_accepts_registered_command_providers() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	assert_true(controller.register_command_provider(FakeDevConsoleCommandProvider.new()))

	controller.submit_command("ping alpha")
	controller.submit_command("pong beta")

	assert_eq(console.outputs[0], "provider:ping alpha")
	assert_eq(console.outputs[1], "provider:pong beta")
	assert_true(console.completions.has("ping"))
	assert_true(console.completions.has("pong"))
	assert_true(console.completions.has("provider_value"))


func test_dev_console_commands_provide_their_own_completions() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	assert_true(controller.register_command_provider(FakeDevConsoleObjectCommandProvider.new()))

	controller.submit_command("op alpha")

	assert_eq(console.outputs, ["object:op alpha"])
	assert_true(console.completions.has("object_ping"))
	assert_true(console.completions.has("op"))
	assert_true(console.completions.has("object_value"))


func test_dev_console_completions_are_unique_and_ignore_invalid_provider_values() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)

	assert_true(controller.register_command_provider(FakeDevConsoleOddCompletionProvider.new()))

	assert_eq(console.completions.count("shared"), 1)
	assert_true(console.completions.has("unique"))
	assert_false(console.completions.has(""))
	assert_false(console.completions.has("not-an-array"))


func test_dev_console_register_command_rejects_invalid_specs() -> void:
	var console = add_child_autoqfree(FakeDevConsole.new())
	var sim := FakeDevConsoleSim.new()
	var inventory = add_child_autoqfree(FakeInventoryWindow.new())
	var controller = autofree(DevConsoleControllerScript.new())
	controller.setup(
		console,
		sim,
		inventory,
		func() -> void:
			pass,
		func() -> Dictionary:
			return {}
	)
	var before_completions: Array = console.completions.duplicate()

	assert_false(controller.register_command({}))
	assert_false(controller.register_command({"name": "broken"}))

	controller.submit_command("broken")

	assert_eq(console.completions, before_completions)
	assert_eq(console.outputs, ["Unknown command 'broken'. Use 'help' for commands."])


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


func test_main_scene_has_simulation_speed_controls() -> void:
	var main_scene = load("res://game/main/main.tscn").instantiate()
	var panel := main_scene.get_node("Hud/SimulationSpeedPanel") as PanelContainer
	var down_button := main_scene.get_node("Hud/SimulationSpeedPanel/SimulationSpeedControls/SimulationSpeedDownButton") as Button
	var speed_label := main_scene.get_node("Hud/SimulationSpeedPanel/SimulationSpeedControls/SimulationSpeedLabel") as Label
	var up_button := main_scene.get_node("Hud/SimulationSpeedPanel/SimulationSpeedControls/SimulationSpeedUpButton") as Button

	assert_not_null(panel)
	assert_not_null(down_button)
	assert_not_null(speed_label)
	assert_not_null(up_button)
	assert_eq(down_button.text, "-")
	assert_eq(speed_label.text, "TPS: 60")
	assert_eq(up_button.text, "+")

	main_scene.free()


func test_main_scene_has_day_night_lighting_nodes() -> void:
	var main_scene = load("res://game/main/main.tscn").instantiate()
	var sun := main_scene.get_node("Sun") as DirectionalLight3D
	var sky_fill := main_scene.get_node("SkyFill") as DirectionalLight3D
	var world_environment := main_scene.get_node("WorldEnvironment") as WorldEnvironment

	assert_not_null(sun)
	assert_not_null(sky_fill)
	assert_not_null(world_environment)
	assert_not_null(world_environment.environment)

	main_scene.free()


func test_main_advances_simulation_at_default_sixty_ticks_per_second() -> void:
	var main = autofree(MainScript.new())
	var sim := FakeSimulationClockSim.new()
	main.sim = sim

	main._advance_simulation(1.0 / 120.0)
	assert_eq(sim.tick_many_calls, [])

	main._advance_simulation(1.0 / 120.0)
	assert_eq(sim.tick_many_calls, [1])

	main._advance_simulation(0.5)
	assert_eq(sim.tick_many_calls, [1, 30])


func test_main_simulation_speed_buttons_clamp_and_pause_ticks() -> void:
	var main = autofree(MainScript.new())
	var sim := FakeSimulationClockSim.new()
	var label = autofree(Label.new())
	main.sim = sim
	main.simulation_speed_label = label

	for _index in range(4):
		main._decrease_simulation_speed()

	assert_eq(main.simulation_ticks_per_second, 0)
	assert_eq(label.text, "TPS: 0")
	main._advance_simulation(1.0)
	assert_eq(sim.tick_many_calls, [])

	main._increase_simulation_speed()
	main._advance_simulation(1.0)

	assert_eq(main.simulation_ticks_per_second, 15)
	assert_eq(label.text, "TPS: 15")
	assert_eq(sim.tick_many_calls, [15])


func test_main_day_night_lighting_matches_core_time_and_solar_factor() -> void:
	var main = autofree(MainScript.new())
	var render_environment := Environment.new()
	main.sun = autofree(DirectionalLight3D.new())
	main.sky_fill = autofree(DirectionalLight3D.new())
	main.world_environment = autofree(WorldEnvironment.new())
	main.world_environment.environment = render_environment

	main._apply_celestial_lighting(0.5, 1.0)

	assert_gt(main.sun.position.y, 40.0)
	assert_gt(main.sun.light_energy, 2.0)
	assert_gt(render_environment.ambient_light_energy, 0.3)
	assert_gt(render_environment.fog_light_energy, 0.2)

	main._apply_celestial_lighting(0.0, 0.0)

	assert_lt(main.sun.position.y, -40.0)
	assert_lt(main.sun.light_energy, 0.1)
	assert_lt(render_environment.ambient_light_energy, 0.2)
	assert_lt(render_environment.fog_light_energy, 0.1)


func test_main_simulation_tick_refreshes_celestial_lighting_from_sim() -> void:
	var main = autofree(MainScript.new())
	var sim := FakeSimulationClockSim.new()
	main.sim = sim
	main.sun = autofree(DirectionalLight3D.new())
	main.sky_fill = autofree(DirectionalLight3D.new())
	main.world_environment = autofree(WorldEnvironment.new())
	main.world_environment.environment = Environment.new()
	sim.time_of_day = 0.0
	sim.solar_factor_value = 0.0

	main._advance_simulation(1.0 / 60.0)

	assert_eq(sim.tick_many_calls, [1])
	assert_lt(main.sun.position.y, -40.0)
	assert_lt(main.sun.light_energy, 0.1)


func test_fps_counter_scene_updates_fps_label_text() -> void:
	var fps_counter = autofree(FpsCounterScene.instantiate())
	add_child(fps_counter)

	fps_counter.update_value(59.6)

	assert_eq(fps_counter.label.text, "FPS: 60")
	assert_eq(FpsCounterScript.format_text(12.2), "FPS: 12")


func test_asset_catalog_miner_can_place_on_generated_resource_tile() -> void:
	var registry := _load_asset_catalog_registry()
	var sim := NeptuneSim.new()
	_configure_asset_sim(sim, registry)
	sim.generate_starting_map(48)

	var resource_tiles: Array = []
	for raw_tile: Variant in sim.map_tiles():
		var tile: Dictionary = raw_tile
		if not str(tile.get("resource", "")).is_empty() and int(tile.get("amount", 0)) > 0:
			resource_tiles.append(tile)

	assert_gt(resource_tiles.size(), 0)
	var valid_origin := Vector2i(999999, 999999)
	for tile: Dictionary in resource_tiles:
		var origin := Vector2i(int(tile["x"]), int(tile["y"]))
		if sim.can_place_building("basic_miner", origin.x, origin.y, 0):
			valid_origin = origin
			break

	assert_ne(valid_origin, Vector2i(999999, 999999))


func test_building_renderer_draws_asset_catalog_fallback_building_footprint() -> void:
	var registry := _load_asset_catalog_registry()
	BuildingCatalogScript.load_from_rows(registry.rows("buildings"))
	var sim := NeptuneSim.new()
	_configure_asset_sim(sim, registry)
	assert_true(sim.place_building("wooden_chest", 0, 0, 0))

	var buildings_root := add_child_autoqfree(Node3D.new()) as Node3D
	var tile_index := {}
	var blocked_tiles := {}

	BuildingRendererScript.render_from_sim(sim, buildings_root, tile_index, blocked_tiles)

	assert_eq(buildings_root.get_child_count(), 1)
	var building_node := buildings_root.get_child(0) as Node3D
	assert_gt(building_node.get_child_count(), 0)
	assert_true(building_node.get_child(0) is MeshInstance3D)
	assert_true(tile_index.has(Vector2i(0, 0)))
	assert_true(blocked_tiles.has(Vector2i(0, 0)))


func test_main_render_buildings_uses_godot_buildings_bridge() -> void:
	var registry := _load_asset_catalog_registry()
	BuildingCatalogScript.load_from_rows(registry.rows("buildings"))
	var sim := NeptuneSim.new()
	_configure_asset_sim(sim, registry)
	assert_true(sim.place_building("wooden_chest", 0, 0, 0))

	var main = autofree(MainScript.new())
	main.sim = sim
	main.buildings_root = add_child_autoqfree(Node3D.new()) as Node3D
	main.building_tile_index = {}
	main.blocked_building_tiles = {}

	main._render_buildings_from_sim()

	assert_eq(main.buildings_root.get_child_count(), 1)
	assert_true(main.building_tile_index.has(Vector2i(0, 0)))
	assert_true(main.blocked_building_tiles.has(Vector2i(0, 0)))


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
		{"x": 2, "y": -1, "surface_z": 2},
		{"x": 3, "y": -1},
	]

	build_ghost.show_footprint(footprint, true)

	assert_true(build_ghost.visible)
	assert_eq(build_ghost.get_child_count(), 2)
	var first_tile := build_ghost.get_child(0) as MeshInstance3D
	assert_eq(
		first_tile.position,
		Vector3(
			2.0,
			build_ghost.BUILD_GHOST_Y + 2.0 * build_ghost.SURFACE_LEVEL_HEIGHT,
			-1.0
		)
	)
	assert_eq((first_tile.material_override as StandardMaterial3D).albedo_color, build_ghost.GHOST_VALID_COLOR)
	var base_tile := build_ghost.get_child(1) as MeshInstance3D
	assert_eq(base_tile.position, Vector3(3.0, build_ghost.BUILD_GHOST_Y, -1.0))

	build_ghost.show_footprint([{"x": 4, "y": 5}], false)

	assert_eq(build_ghost.get_child_count(), 1)
	var blocked_tile := build_ghost.get_child(0) as MeshInstance3D
	assert_eq(blocked_tile.position, Vector3(4.0, build_ghost.BUILD_GHOST_Y, 5.0))
	assert_eq((blocked_tile.material_override as StandardMaterial3D).albedo_color, build_ghost.GHOST_BLOCKED_COLOR)

	build_ghost.hide_preview()

	assert_false(build_ghost.visible)
	assert_eq(build_ghost.get_child_count(), 0)
