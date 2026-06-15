extends RefCounted
class_name DevConsoleController

const ItemCatalogScript := preload("res://game/items/item_catalog.gd")

var dev_console: Node
var sim: Variant
var inventory_window: Node
var inventory_refresh: Callable
var selected_object_snapshot: Callable


func setup(
	console_node: Node,
	simulation: Variant,
	inventory_window_node: Node,
	refresh_inventory: Callable,
	selected_snapshot: Callable
) -> void:
	dev_console = console_node
	sim = simulation
	inventory_window = inventory_window_node
	inventory_refresh = refresh_inventory
	selected_object_snapshot = selected_snapshot
	if dev_console != null:
		dev_console.command_submitted.connect(submit_command)
		dev_console.set_completions(completions())


func submit_command(line: String) -> void:
	var parts := line.split(" ", false)
	if parts.is_empty():
		return

	match str(parts[0]).to_lower():
		"help":
			dev_console.append_lines([
				"commands: help, clear, status, items, give <item> <amount>",
				"F1 toggles console. Up/Down navigate history. Tab completes item ids.",
			])
		"clear":
			dev_console.clear_scrollback()
		"status":
			dev_console.append_output(_status_line())
		"items":
			dev_console.append_output("items: %s" % "  ".join(_item_ids()))
		"give":
			_execute_give_command(parts)
		_:
			dev_console.append_output("Unknown command '%s'. Use 'help' for commands." % str(parts[0]))


func completions() -> Array:
	var values: Array = ["help", "clear", "status", "items", "give"]
	for item_id: String in _item_ids():
		values.append(item_id)
	return values


func _status_line() -> String:
	var selected := _selected_object_status()
	return "tick=%d digest=%d buildings=%d selected=%s" % [
		sim.core_tick(),
		sim.digest(),
		sim.building_count(),
		selected,
	]


func _selected_object_status() -> String:
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


func _execute_give_command(parts: PackedStringArray) -> void:
	if parts.size() < 2:
		dev_console.append_output("Usage: give <item> <amount>")
		return
	var item_id := str(parts[1])
	var amount := 1
	if parts.size() >= 3:
		amount = max(1, int(parts[2]))
	if sim.give_item(item_id, amount):
		dev_console.append_output("Added %s x%d" % [item_id, amount])
		if inventory_window != null and inventory_window.is_open() and inventory_refresh.is_valid():
			inventory_refresh.call()
		return
	dev_console.append_output("Could not add %s x%d" % [item_id, amount])


func _item_ids() -> Array[String]:
	var ids: Array[String] = []
	for raw_definition: Variant in ItemCatalogScript.definitions():
		var definition: Dictionary = raw_definition
		var id := str(definition.get("id", ""))
		if not id.is_empty():
			ids.append(id)
	return ids
