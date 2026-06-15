extends RefCounted
class_name DevConsoleController

const DevConsoleCommandContextScript := preload("res://game/main/dev_console/command_context.gd")
const DevConsoleCommandRegistryScript := preload("res://game/main/dev_console/command_registry.gd")
const DevConsoleBuiltinCommandsScript := preload("res://game/main/dev_console/builtin_commands.gd")

var dev_console: Node
var context: RefCounted
var registry: RefCounted
var _builtin_commands: RefCounted
var _mod_command_providers: Array = []


func setup(
	console_node: Node,
	simulation: Variant,
	inventory_window_node: Node,
	refresh_inventory: Callable,
	selected_snapshot: Callable,
	teleport_player: Callable = Callable()
) -> void:
	dev_console = console_node
	registry = DevConsoleCommandRegistryScript.new()
	context = DevConsoleCommandContextScript.new()
	context.setup(
		console_node,
		simulation,
		inventory_window_node,
		refresh_inventory,
		selected_snapshot,
		registry,
		teleport_player
	)
	_builtin_commands = DevConsoleBuiltinCommandsScript.new()
	registry.register_provider(_builtin_commands)
	_register_manifest_command_providers()
	if dev_console != null:
		dev_console.command_submitted.connect(submit_command)
		dev_console.set_completions(completions())


func submit_command(line: String) -> void:
	registry.execute_line(context, line)


func completions() -> Array:
	return registry.completion_values(context)


func register_command(spec: Dictionary) -> bool:
	var result: bool = registry.register_command_spec(spec)
	if result and dev_console != null:
		dev_console.set_completions(completions())
	return result


func register_command_provider(provider: Variant) -> bool:
	var result: bool = registry.register_provider(provider)
	if result:
		_mod_command_providers.append(provider)
		if dev_console != null:
			dev_console.set_completions(completions())
	return result


func _register_manifest_command_providers() -> void:
	if dev_console == null or not dev_console.is_inside_tree():
		return

	var root := dev_console.get_tree().root
	if not root.has_meta("mod_registry"):
		return

	var mod_registry: Variant = root.get_meta("mod_registry")
	if mod_registry == null or not mod_registry.has_method("dev_console_command_paths"):
		return

	for path: String in mod_registry.dev_console_command_paths():
		_register_command_provider_script(path)


func _register_command_provider_script(path: String) -> void:
	if path.is_empty() or not ResourceLoader.exists(path):
		push_warning("Dev console command provider does not exist: %s" % path)
		return
	var script: Variant = load(path)
	if script == null:
		push_warning("Dev console command provider failed to load: %s" % path)
		return
	var provider: Variant = script.new()
	if not register_command_provider(provider):
		push_warning("Dev console command provider has no register_dev_console_commands(): %s" % path)
