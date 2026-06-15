extends RefCounted
class_name DevConsoleCommandRegistry

var _commands := {}
var _aliases := {}
var _providers: Array = []


func register_command(
	name: String,
	description: String,
	usage: String,
	execute: Callable,
	completions: Callable = Callable(),
	aliases: Array = []
) -> bool:
	var command_name := name.strip_edges().to_lower()
	if command_name.is_empty() or not execute.is_valid():
		return false

	_commands[command_name] = {
		"name": command_name,
		"description": description,
		"usage": usage,
		"execute": execute,
		"completions": completions,
		"aliases": aliases.duplicate(),
	}

	for raw_alias: Variant in aliases:
		var alias := str(raw_alias).strip_edges().to_lower()
		if not alias.is_empty():
			_aliases[alias] = command_name

	return true


func register_command_spec(spec: Dictionary) -> bool:
	return register_command(
		str(spec.get("name", "")),
		str(spec.get("description", "")),
		str(spec.get("usage", "")),
		spec.get("execute", Callable()),
		spec.get("completions", spec.get("complete", Callable())),
		spec.get("aliases", [])
	)


func register_provider(provider: Variant) -> bool:
	if provider == null or not provider.has_method("register_dev_console_commands"):
		return false
	_providers.append(provider)
	provider.register_dev_console_commands(self)
	return true


func execute_line(context: RefCounted, line: String) -> void:
	var parts := line.split(" ", false)
	if parts.is_empty():
		return

	var requested_name := str(parts[0]).to_lower()
	var command_name := _resolve_command_name(requested_name)
	if command_name.is_empty():
		context.append_output("Unknown command '%s'. Use 'help' for commands." % str(parts[0]))
		return

	var command: Dictionary = _commands[command_name]
	var execute: Callable = command["execute"]
	execute.call(context, parts)


func completion_values(context: RefCounted) -> Array:
	var values: Array = command_names()
	for alias: String in _aliases.keys():
		if not values.has(alias):
			values.append(alias)

	for command_name: String in command_names():
		var command: Dictionary = _commands[command_name]
		var completions: Callable = command.get("completions", Callable())
		if not completions.is_valid():
			continue
		var raw_values: Variant = completions.call(context)
		if not raw_values is Array:
			continue
		for raw_value: Variant in raw_values:
			var value := str(raw_value)
			if not value.is_empty() and not values.has(value):
				values.append(value)
	return values


func command_names() -> Array:
	var names := _commands.keys()
	names.sort()
	return names


func command_summaries() -> Array[String]:
	var lines: Array[String] = []
	for command_name: String in command_names():
		var command: Dictionary = _commands[command_name]
		var usage := str(command.get("usage", command_name))
		var description := str(command.get("description", ""))
		if description.is_empty():
			lines.append(usage)
		else:
			lines.append("%s - %s" % [usage, description])
	return lines


func has_command(name: String) -> bool:
	return not _resolve_command_name(name).is_empty()


func _resolve_command_name(name: String) -> String:
	var command_name := name.strip_edges().to_lower()
	if _commands.has(command_name):
		return command_name
	return str(_aliases.get(command_name, ""))
