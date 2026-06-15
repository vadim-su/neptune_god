extends "res://addons/gut/test.gd"

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const CatalogSelectorScript := preload("res://game/ui/catalog_selector.gd")
const HotbarScript := preload("res://game/ui/hotbar.gd")
const InventorySlotScene := preload("res://game/ui/inventory_slot.tscn")
const MapOverlayScript := preload("res://game/ui/map_overlay.gd")
const DevConsoleControllerScript := preload("res://game/main/dev_console_controller.gd")
const DevConsoleCommandContextScript := preload("res://game/main/dev_console/command_context.gd")
const DevConsoleCommandRegistryScript := preload("res://game/main/dev_console/command_registry.gd")
const InventoryControllerScript := preload("res://game/main/inventory_controller.gd")
const MapOverlayControllerScript := preload("res://game/main/map_overlay_controller.gd")
const MainScript := preload("res://game/main/main.gd")
const FpsCounterScript := preload("res://game/ui/fps_counter.gd")
const FpsCounterScene := preload("res://game/ui/fps_counter.tscn")
const MinimapScene := preload("res://game/ui/minimap.tscn")
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


class FakeDevConsoleCommandProvider:
	extends RefCounted

	func register_dev_console_commands(registry: RefCounted) -> void:
		registry.register_command(
			"ping",
			"Test command provider hook.",
			"ping",
			Callable(self, "_execute_ping"),
			Callable(self, "_complete_ping"),
			["pong"]
		)

	func _execute_ping(context: RefCounted, parts: PackedStringArray) -> void:
		context.append_output("provider:%s" % " ".join(parts))

	func _complete_ping(_context: RefCounted) -> Array:
		return ["provider_value"]


class FakeDevConsoleObjectCommand:
	extends RefCounted

	func command_name() -> String:
		return "object_ping"

	func command_description() -> String:
		return "Object command provider hook."

	func command_usage() -> String:
		return "object_ping"

	func command_aliases() -> Array:
		return ["op"]

	func execute(context: RefCounted, parts: PackedStringArray) -> void:
		context.append_output("object:%s" % " ".join(parts))

	func complete(_context: RefCounted) -> Array:
		return ["object_value"]


class FakeDevConsoleObjectCommandProvider:
	extends RefCounted

	func register_dev_console_commands(registry: RefCounted) -> void:
		registry.register_command_object(FakeDevConsoleObjectCommand.new())


class FakeDevConsoleOddCompletionProvider:
	extends RefCounted

	func register_dev_console_commands(registry: RefCounted) -> void:
		registry.register_command(
			"alpha",
			"Alpha command.",
			"alpha",
			Callable(self, "_execute_alpha"),
			Callable(self, "_complete_alpha")
		)
		registry.register_command(
			"beta",
			"Beta command.",
			"beta",
			Callable(self, "_execute_beta"),
			Callable(self, "_complete_beta")
		)

	func _execute_alpha(context: RefCounted, _parts: PackedStringArray) -> void:
		context.append_output("alpha")

	func _execute_beta(context: RefCounted, _parts: PackedStringArray) -> void:
		context.append_output("beta")

	func _complete_alpha(_context: RefCounted) -> Array:
		return ["shared", "shared", "", "unique"]

	func _complete_beta(_context: RefCounted) -> String:
		return "not-an-array"


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
