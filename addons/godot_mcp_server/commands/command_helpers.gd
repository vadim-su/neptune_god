## command_helpers.gd — Shared utility functions for editor command modules.
## C-05: Extracted from 7 files to eliminate ~120 lines of duplication.

class_name CommandHelpers


## Get the root node of the currently edited scene.
## Tries EditorInterface first (editor mode), falls back to SceneTree root child (headless).
static func get_edited_scene_root(plugin: EditorPlugin = null) -> Node:
	if plugin != null:
		var ei: EditorInterface = plugin.get_editor_interface()
		if ei != null:
			var edited: Node = ei.get_edited_scene_root()
			if edited != null:
				return edited
	var ml: MainLoop = Engine.get_main_loop()
	if ml == null or not (ml is SceneTree):
		return null
	var st: SceneTree = ml as SceneTree
	if st == null or st.root == null:
		return null
	if st.root.get_child_count() > 0:
		return st.root.get_child(0)
	return null


## Find a node by path relative to root.
## Strips leading "root/" prefix and leading slashes.
static func find_node(root: Node, path: String) -> Node:
	if path == "" or path == "root":
		return root
	var p: String = path
	while p.begins_with("/"):
		p = p.substr(1)
	if p.begins_with("root/"):
		p = p.substr(5)
	if p.begins_with(root.name + "/"):
		p = p.substr(root.name.length() + 1)
	elif p == root.name:
		return root
	if p == "":
		return root
	return root.get_node_or_null(p)


## Check that a property exists on an object and its value type is compatible.
## Replaces duplicated copies in scene_commands.gd and ui_commands.gd.
## C-03: Removed string wildcard pass-through — strings are only allowed when
## the target property is also a string, or when str_to_var can parse them into
## the correct type (e.g., "Vector2(1, 2)" for a Vector2 property).
static func property_exists_and_type_ok(obj: Object, prop_name: String, val) -> bool:
	var found: bool = false
	for p: Dictionary in obj.get_property_list():
		if p["name"] == prop_name:
			found = true
			break
	if not found:
		return false
	var current: Variant = obj.get(prop_name)
	if current == null:
		return val == null
	var current_type: int = typeof(current)
	var val_type: int = typeof(val)
	if current_type == val_type:
		return true
	# Allow float/int interchange
	if (current_type == TYPE_FLOAT and val_type == TYPE_INT) or (current_type == TYPE_INT and val_type == TYPE_FLOAT):
		return true
	# C-03: String values only allowed for string properties, or when str_to_var
	# can convert them to the expected type (e.g. "Vector2(1,2)" → TYPE_VECTOR2)
	if val_type == TYPE_STRING:
		if current_type == TYPE_STRING:
			return true
		# Try str_to_var conversion and check the result type matches
		var parsed: Variant = str_to_var(val)
		if parsed != null:
			return typeof(parsed) == current_type
		return false
	return false  # type mismatch — reject
