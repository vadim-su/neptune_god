extends RefCounted
class_name DevConsoleCommandContext

const ItemCatalogScript := preload("res://game/items/item_catalog.gd")

var dev_console: Node
var sim: Variant
var inventory_window: Node
var inventory_refresh: Callable
var selected_object_snapshot: Callable
var player_teleport: Callable
var registry: RefCounted


func setup(
	console_node: Node,
	simulation: Variant,
	inventory_window_node: Node,
	refresh_inventory: Callable,
	selected_snapshot: Callable,
	command_registry: RefCounted,
	teleport_player: Callable = Callable()
) -> void:
	dev_console = console_node
	sim = simulation
	inventory_window = inventory_window_node
	inventory_refresh = refresh_inventory
	selected_object_snapshot = selected_snapshot
	registry = command_registry
	player_teleport = teleport_player


func append_output(line: String) -> void:
	if dev_console != null:
		dev_console.append_output(line)


func append_lines(lines: Array) -> void:
	if dev_console != null:
		dev_console.append_lines(lines)


func clear_scrollback() -> void:
	if dev_console != null:
		dev_console.clear_scrollback()


func refresh_inventory_if_open() -> void:
	if inventory_window != null and inventory_window.is_open() and inventory_refresh.is_valid():
		inventory_refresh.call()


func teleport_player(position: Vector3) -> bool:
	if not player_teleport.is_valid():
		return false
	player_teleport.call(position)
	return true


func status_line() -> String:
	return "tick=%d digest=%d buildings=%d selected=%s" % [
		sim.core_tick(),
		sim.digest(),
		sim.building_count(),
		selected_object_status(),
	]


func selected_object_status() -> String:
	if not selected_object_snapshot.is_valid():
		return "none"
	var snapshot: Dictionary = selected_object_snapshot.call()
	var selected_object: Dictionary = snapshot.get("object", {})
	if selected_object.is_empty():
		return "none"
	return "%s #%d" % [
		str(selected_object.get("def_id", "")),
		int(snapshot.get("id", -1)),
	]


func item_ids() -> Array[String]:
	var ids: Array[String] = []
	for raw_definition: Variant in ItemCatalogScript.definitions():
		var definition: Dictionary = raw_definition
		var id := str(definition.get("id", ""))
		if not id.is_empty():
			ids.append(id)
	return ids
