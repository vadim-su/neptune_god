extends RefCounted
class_name DevConsoleCommandRegistry

class CallableCommand:
	extends RefCounted

	var _name := ""
	var _description := ""
	var _usage := ""
	var _execute := Callable()
	var _complete := Callable()
	var _aliases: Array = []

	func setup(
		name: String,
		description: String,
		usage: String,
		execute: Callable,
		complete: Callable = Callable(),
		aliases: Array = []
	) -> void:
		_name = name
		_description = description
		_usage = usage
		_execute = execute
		_complete = complete
		_aliases = aliases.duplicate()

	func command_name() -> String:
		return _name

	func command_description() -> String:
		return _description

	func command_usage() -> String:
		return _usage

	func command_aliases() -> Array:
		return _aliases.duplicate()

	func execute(context: RefCounted, parts: PackedStringArray) -> void:
		_execute.call(context, parts)

	func complete(context: RefCounted) -> Array:
		if not _complete.is_valid():
			return []
		var values: Variant = _complete.call(context)
		if values is Array:
			return values
		return []


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
	if name.strip_edges().is_empty() or not execute.is_valid():
		return false
	var command := CallableCommand.new()
	command.setup(name, description, usage, execute, completions, aliases)
	return register_command_object(command)


func register_command_object(command: Variant) -> bool:
	if not _is_valid_command(command):
		return false

	var command_name := _command_name(command)
	if command_name.is_empty():
		return false

	_commands[command_name] = command
	for raw_alias: Variant in _command_aliases(command):
		var alias := str(raw_alias).strip_edges().to_lower()
		if not alias.is_empty():
			_aliases[alias] = command_name

	return true


func _is_valid_command(command: Variant) -> bool:
	return (
		command != null
		and command.has_method("command_name")
		and command.has_method("execute")
	)


func _command_name(command: Variant) -> String:
	return str(command.command_name()).strip_edges().to_lower()


func _command_description(command: Variant) -> String:
	if command.has_method("command_description"):
		return str(command.command_description())
	return ""


func _command_usage(command: Variant) -> String:
	if command.has_method("command_usage"):
		var usage := str(command.command_usage())
		if not usage.is_empty():
			return usage
	return _command_name(command)


func _command_aliases(command: Variant) -> Array:
	if not command.has_method("command_aliases"):
		return []
	var aliases: Variant = command.command_aliases()
	if aliases is Array:
		return aliases
	return []


func _command_complete(command: Variant, context: RefCounted) -> Array:
	if not command.has_method("complete"):
		return []
	var values: Variant = command.complete(context)
	if values is Array:
		return values
	return []


func register_command_spec(spec: Dictionary) -> bool:
	if spec.has("command"):
		return register_command_object(spec.get("command"))
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

	var command: Variant = _commands[command_name]
	command.execute(context, parts)


func completion_values(context: RefCounted) -> Array:
	var values: Array = command_names()
	for alias: String in _aliases.keys():
		if not values.has(alias):
			values.append(alias)

	for command_name: String in command_names():
		var command: Variant = _commands[command_name]
		for raw_value: Variant in _command_complete(command, context):
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
		var command: Variant = _commands[command_name]
		var usage := _command_usage(command)
		var description := _command_description(command)
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
