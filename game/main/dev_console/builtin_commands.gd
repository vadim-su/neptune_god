extends RefCounted
class_name DevConsoleBuiltinCommands


func register_dev_console_commands(registry: RefCounted) -> void:
	registry.register_command(
		"help",
		"List registered console commands.",
		"help",
		Callable(self, "_execute_help")
	)
	registry.register_command(
		"clear",
		"Clear console output.",
		"clear",
		Callable(self, "_execute_clear")
	)
	registry.register_command(
		"status",
		"Print simulation status.",
		"status",
		Callable(self, "_execute_status")
	)
	registry.register_command(
		"items",
		"List item ids.",
		"items",
		Callable(self, "_execute_items"),
		Callable(self, "_complete_items")
	)
	registry.register_command(
		"give",
		"Add an item stack to the player inventory.",
		"give <item> <amount>",
		Callable(self, "_execute_give"),
		Callable(self, "_complete_items")
	)
	registry.register_command(
		"teleport",
		"Move the player to world coordinates.",
		"teleport <x> <z>",
		Callable(self, "_execute_teleport"),
		Callable(),
		["tp"]
	)


func _execute_help(context: RefCounted, _parts: PackedStringArray) -> void:
	var lines: Array = ["commands:"]
	for line: String in context.registry.command_summaries():
		lines.append("- %s" % line)
	lines.append("F1 toggles console. Up/Down navigate history. Tab completes commands and item ids.")
	context.append_lines(lines)


func _execute_clear(context: RefCounted, _parts: PackedStringArray) -> void:
	context.clear_scrollback()


func _execute_status(context: RefCounted, _parts: PackedStringArray) -> void:
	context.append_output(context.status_line())


func _execute_items(context: RefCounted, _parts: PackedStringArray) -> void:
	context.append_output("items: %s" % "  ".join(context.item_ids()))


func _execute_give(context: RefCounted, parts: PackedStringArray) -> void:
	if parts.size() < 2:
		context.append_output("Usage: give <item> <amount>")
		return
	var item_id := str(parts[1])
	var amount := 1
	if parts.size() >= 3:
		amount = max(1, int(parts[2]))
	if context.sim.give_item(item_id, amount):
		context.append_output("Added %s x%d" % [item_id, amount])
		context.refresh_inventory_if_open()
		return
	context.append_output("Could not add %s x%d" % [item_id, amount])


func _execute_teleport(context: RefCounted, parts: PackedStringArray) -> void:
	if parts.size() < 3:
		context.append_output("Usage: teleport <x> <z>")
		return
	if not str(parts[1]).is_valid_float() or not str(parts[2]).is_valid_float():
		context.append_output("Usage: teleport <x> <z>")
		return

	var position := Vector3(float(parts[1]), 0.0, float(parts[2]))
	if context.teleport_player(position):
		context.append_output("Teleported player to x=%.2f z=%.2f" % [position.x, position.z])
		return
	context.append_output("Teleport is not available.")


func _complete_items(context: RefCounted) -> Array:
	return context.item_ids()
