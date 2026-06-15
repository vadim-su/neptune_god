extends RefCounted
class_name DevConsoleBuiltinCommands

class HelpCommand:
	extends RefCounted

	func command_name() -> String:
		return "help"

	func command_description() -> String:
		return "List registered console commands."

	func command_usage() -> String:
		return "help"

	func execute(context: RefCounted, _parts: PackedStringArray) -> void:
		var lines: Array = ["commands:"]
		for line: String in context.registry.command_summaries():
			lines.append("- %s" % line)
		lines.append("F1 toggles console. Up/Down navigate history. Tab completes commands and item ids.")
		context.append_lines(lines)


class ClearCommand:
	extends RefCounted

	func command_name() -> String:
		return "clear"

	func command_description() -> String:
		return "Clear console output."

	func command_usage() -> String:
		return "clear"

	func execute(context: RefCounted, _parts: PackedStringArray) -> void:
		context.clear_scrollback()


class StatusCommand:
	extends RefCounted

	func command_name() -> String:
		return "status"

	func command_description() -> String:
		return "Print simulation status."

	func command_usage() -> String:
		return "status"

	func execute(context: RefCounted, _parts: PackedStringArray) -> void:
		context.append_output(context.status_line())


class ItemsCommand:
	extends RefCounted

	func command_name() -> String:
		return "items"

	func command_description() -> String:
		return "List item ids."

	func command_usage() -> String:
		return "items"

	func execute(context: RefCounted, _parts: PackedStringArray) -> void:
		context.append_output("items: %s" % "  ".join(context.item_ids()))

	func complete(context: RefCounted) -> Array:
		return context.item_ids()


class GiveCommand:
	extends RefCounted

	func command_name() -> String:
		return "give"

	func command_description() -> String:
		return "Add an item stack to the player inventory."

	func command_usage() -> String:
		return "give <item> <amount>"

	func execute(context: RefCounted, parts: PackedStringArray) -> void:
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

	func complete(context: RefCounted) -> Array:
		return context.item_ids()


class TeleportCommand:
	extends RefCounted

	func command_name() -> String:
		return "teleport"

	func command_description() -> String:
		return "Move the player to world coordinates."

	func command_usage() -> String:
		return "teleport <x> <z>"

	func command_aliases() -> Array:
		return ["tp"]

	func execute(context: RefCounted, parts: PackedStringArray) -> void:
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


func register_dev_console_commands(registry: RefCounted) -> void:
	registry.register_command_object(HelpCommand.new())
	registry.register_command_object(ClearCommand.new())
	registry.register_command_object(StatusCommand.new())
	registry.register_command_object(ItemsCommand.new())
	registry.register_command_object(GiveCommand.new())
	registry.register_command_object(TeleportCommand.new())
