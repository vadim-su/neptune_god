@tool
extends EditorPlugin

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const PREVIEW_HEIGHT := 260

var dock: VBoxContainer
var item_list: ItemList
var title_label: Label
var detail_label: RichTextLabel
var viewport: SubViewport
var preview_root: Node3D
var preview_camera: Camera3D
var definitions: Array = []


func _enter_tree() -> void:
	dock = _build_dock()
	add_control_to_dock(DOCK_SLOT_RIGHT_UL, dock)
	_reload_catalog()


func _exit_tree() -> void:
	if dock != null:
		remove_control_from_docks(dock)
		dock.queue_free()
		dock = null


func _build_dock() -> VBoxContainer:
	var root := VBoxContainer.new()
	root.name = "ResourceCatalogDock"
	root.custom_minimum_size = Vector2(360.0, 520.0)
	root.add_theme_constant_override("separation", 8)

	var toolbar := HBoxContainer.new()
	toolbar.add_theme_constant_override("separation", 6)
	root.add_child(toolbar)

	var heading := Label.new()
	heading.text = "Resource Catalog"
	heading.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	heading.add_theme_font_size_override("font_size", 16)
	toolbar.add_child(heading)

	var reload_button := Button.new()
	reload_button.text = "Reload"
	reload_button.tooltip_text = "Reload assets/catalog/buildings.json"
	reload_button.pressed.connect(_reload_catalog)
	toolbar.add_child(reload_button)

	item_list = ItemList.new()
	item_list.custom_minimum_size = Vector2(0.0, 180.0)
	item_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	item_list.item_selected.connect(_on_item_selected)
	root.add_child(item_list)

	title_label = Label.new()
	title_label.text = "No building selected"
	title_label.add_theme_font_size_override("font_size", 15)
	root.add_child(title_label)

	detail_label = RichTextLabel.new()
	detail_label.custom_minimum_size = Vector2(0.0, 132.0)
	detail_label.fit_content = true
	detail_label.scroll_active = false
	detail_label.bbcode_enabled = true
	root.add_child(detail_label)

	var viewport_container := SubViewportContainer.new()
	viewport_container.custom_minimum_size = Vector2(0.0, PREVIEW_HEIGHT)
	viewport_container.stretch = true
	root.add_child(viewport_container)

	viewport = SubViewport.new()
	viewport.size = Vector2i(520, 360)
	viewport.own_world_3d = true
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	viewport_container.add_child(viewport)

	var world := Node3D.new()
	world.name = "PreviewWorld"
	viewport.add_child(world)

	preview_root = Node3D.new()
	preview_root.name = "PreviewRoot"
	world.add_child(preview_root)

	var sun := DirectionalLight3D.new()
	sun.name = "PreviewKeyLight"
	sun.rotation_degrees = Vector3(-48.0, -35.0, 0.0)
	sun.light_energy = 2.2
	world.add_child(sun)

	var fill := OmniLight3D.new()
	fill.name = "PreviewFillLight"
	fill.position = Vector3(-2.0, 2.5, 3.0)
	fill.light_energy = 1.2
	world.add_child(fill)

	preview_camera = Camera3D.new()
	preview_camera.name = "PreviewCamera"
	preview_camera.look_at_from_position(Vector3(3.2, 2.4, 4.2), Vector3(0.0, 0.45, 0.0), Vector3.UP)
	preview_camera.current = true
	world.add_child(preview_camera)

	return root


func _reload_catalog() -> void:
	BuildingCatalogScript.reload()
	definitions = BuildingCatalogScript.definitions()
	item_list.clear()

	for definition: Dictionary in definitions:
		var id := str(definition.get("id", ""))
		var label := "%s  (%s)" % [BuildingCatalogScript.display_name(id), id]
		item_list.add_item(label)

	if definitions.is_empty():
		title_label.text = "No buildings in catalog"
		detail_label.text = "[color=orange]No definitions found.[/color]"
		_clear_preview()
		return

	item_list.select(0)
	_show_definition(0)


func _on_item_selected(index: int) -> void:
	_show_definition(index)


func _show_definition(index: int) -> void:
	if index < 0 or index >= definitions.size():
		return

	var definition: Dictionary = definitions[index]
	var id := str(definition.get("id", ""))
	var model_path := BuildingCatalogScript.model_path(id)
	title_label.text = BuildingCatalogScript.display_name(id)
	detail_label.text = _definition_text(id, model_path)
	_preview_model(id, model_path)


func _definition_text(id: String, model_path: String) -> String:
	var model_state := "[color=gray]fallback geometry[/color]"
	if not model_path.is_empty():
		model_state = model_path
		if not ResourceLoader.exists(model_path):
			model_state = "[color=orange]%s[/color]" % model_path

	var rows := [
		"[b]id[/b]: %s" % id,
		"[b]type[/b]: %s" % BuildingCatalogScript.ui_type(id),
		"[b]state[/b]: %s" % BuildingCatalogScript.state_label(id),
		"[b]walkable[/b]: %s" % str(BuildingCatalogScript.is_walkable(id)),
		"[b]color[/b]: %s" % BuildingCatalogScript.color(id).to_html(false),
		"[b]model[/b]: %s" % model_state,
	]

	var glyph := str(BuildingCatalogScript.definition(id).get("glyph", ""))
	if not glyph.is_empty():
		rows.insert(rows.size() - 1, "[b]glyph[/b]: %s" % glyph)

	return "\n".join(rows)


func _preview_model(id: String, model_path: String) -> void:
	_clear_preview()

	var instance: Node3D = null
	if not model_path.is_empty() and ResourceLoader.exists(model_path):
		var packed := load(model_path) as PackedScene
		if packed != null:
			instance = packed.instantiate() as Node3D

	if instance == null:
		instance = _fallback_mesh(BuildingCatalogScript.color(id))

	preview_root.add_child(instance)
	_frame_preview(instance)


func _fallback_mesh(color: Color) -> Node3D:
	var instance := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = Vector3(0.9, 0.48, 0.9)
	instance.mesh = mesh
	instance.position.y = 0.24
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.82
	instance.material_override = material
	return instance


func _frame_preview(instance: Node3D) -> void:
	await get_tree().process_frame
	if preview_camera == null or not is_instance_valid(instance):
		return

	var bounds := _node_bounds(instance)
	var center := bounds.get_center()
	var size := bounds.size
	var radius: float = max(size.x, max(size.y, size.z)) * 0.65
	radius = max(radius, 1.0)
	preview_camera.position = center + Vector3(radius * 1.7, radius * 1.25, radius * 2.1)
	preview_camera.look_at(center, Vector3.UP)
	preview_camera.near = 0.01
	preview_camera.far = max(32.0, radius * 12.0)


func _node_bounds(node: Node) -> AABB:
	var bounds := AABB(Vector3(-0.5, 0.0, -0.5), Vector3(1.0, 1.0, 1.0))
	var found := false
	for child: Node in node.find_children("*", "MeshInstance3D", true, false):
		var mesh_instance := child as MeshInstance3D
		if mesh_instance.mesh == null:
			continue
		var child_aabb := mesh_instance.global_transform * mesh_instance.get_aabb()
		if found:
			bounds = bounds.merge(child_aabb)
		else:
			bounds = child_aabb
			found = true
	return bounds


func _clear_preview() -> void:
	for child: Node in preview_root.get_children():
		preview_root.remove_child(child)
		child.queue_free()
