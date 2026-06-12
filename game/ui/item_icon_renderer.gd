extends Node
class_name ItemIconRenderer

const ItemCatalogScript := preload("res://game/items/item_catalog.gd")

const VIEWPORT_SIZE := Vector2i(128, 128)
const SUBJECT_TARGET_SIZE := 1.55

var _texture_cache: Dictionary = {}
var _scene_cache: Dictionary = {}


func prepare_icons(item_ids: Array) -> void:
	for raw_item_id: Variant in item_ids:
		texture_for(str(raw_item_id))


func texture_for(item_id: String) -> Texture2D:
	if item_id.is_empty():
		return null
	if _texture_cache.has(item_id):
		return _texture_cache[item_id]

	var viewport := _build_viewport(item_id)
	add_child(viewport)
	var texture := viewport.get_texture()
	_texture_cache[item_id] = texture
	return texture


func _build_viewport(item_id: String) -> SubViewport:
	var viewport := SubViewport.new()
	viewport.name = "ItemIconViewport_%s" % item_id
	viewport.size = VIEWPORT_SIZE
	viewport.transparent_bg = true
	viewport.own_world_3d = true
	viewport.render_target_clear_mode = SubViewport.CLEAR_MODE_ALWAYS
	viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
	viewport.msaa_3d = Viewport.MSAA_4X
	viewport.screen_space_aa = Viewport.SCREEN_SPACE_AA_FXAA

	var world_environment := WorldEnvironment.new()
	world_environment.environment = _icon_environment()
	viewport.add_child(world_environment)

	var subject := _instantiate_subject(item_id)
	viewport.add_child(subject)
	_normalize_subject(subject)

	var key_light := DirectionalLight3D.new()
	key_light.name = "KeyLight"
	key_light.rotation_degrees = Vector3(-48.0, 32.0, -16.0)
	key_light.light_energy = 2.15
	key_light.shadow_enabled = false
	viewport.add_child(key_light)

	var fill_light := OmniLight3D.new()
	fill_light.name = "FillLight"
	fill_light.position = Vector3(-2.1, 1.6, 1.9)
	fill_light.light_energy = 1.25
	fill_light.omni_range = 5.0
	viewport.add_child(fill_light)

	var camera := Camera3D.new()
	camera.name = "ItemIconCamera"
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 1.72
	camera.look_at_from_position(Vector3(2.25, 1.65, 2.35), Vector3(0.0, 0.12, 0.0), Vector3.UP)
	camera.current = true
	viewport.add_child(camera)

	return viewport


func _icon_environment() -> Environment:
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.0, 0.0, 0.0, 0.0)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.72, 0.76, 0.70, 1.0)
	environment.ambient_light_energy = 0.78
	environment.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	return environment


func _instantiate_subject(item_id: String) -> Node3D:
	var root := Node3D.new()
	root.name = "ItemIconSubject_%s" % item_id
	root.rotation_degrees = _subject_rotation(item_id)

	var model := _instantiate_model(item_id)
	if model != null:
		root.add_child(model)
		if item_id == "copper_plate":
			_apply_material_override(root, ItemCatalogScript.color(item_id))
		return root

	root.add_child(_fallback_subject(item_id))
	return root


func _instantiate_model(item_id: String) -> Node3D:
	var path := ItemCatalogScript.model_path(item_id)
	if path.is_empty():
		return null

	var scene := _scene_for_path(path)
	if scene == null:
		push_warning("Missing item icon model scene for %s at %s" % [item_id, path])
		return null

	var instance := scene.instantiate() as Node3D
	if instance == null:
		push_warning("Item icon model scene is not Node3D for %s at %s" % [item_id, path])
		return null

	return instance


func _scene_for_path(path: String) -> PackedScene:
	if _scene_cache.has(path):
		return _scene_cache[path] as PackedScene

	var scene := load(path) as PackedScene
	if scene != null:
		_scene_cache[path] = scene
	return scene


func _subject_rotation(item_id: String) -> Vector3:
	match item_id:
		"copper_cable":
			return Vector3(0.0, -22.0, 0.0)
		"iron_stick", "iron_gear":
			return Vector3(0.0, -36.0, 0.0)
		_:
			return Vector3(0.0, -28.0, 0.0)


func _normalize_subject(subject: Node3D) -> void:
	var bounds := _combined_global_aabb(subject)
	if bounds.size == Vector3.ZERO:
		return

	subject.position -= bounds.get_center()
	var largest_axis: float = maxf(bounds.size.x, maxf(bounds.size.y, bounds.size.z))
	if largest_axis <= 0.001:
		return
	subject.scale *= SUBJECT_TARGET_SIZE / largest_axis


func _combined_global_aabb(root: Node) -> AABB:
	var state := {
		"found": false,
		"aabb": AABB(),
	}
	_collect_bounds(root, state, Transform3D.IDENTITY)
	return state["aabb"] if state["found"] else AABB()


func _collect_bounds(node: Node, state: Dictionary, parent_transform: Transform3D) -> void:
	var node_transform := parent_transform
	if node is Node3D:
		node_transform = parent_transform * (node as Node3D).transform

	if node is MeshInstance3D and node.mesh != null:
		var mesh_instance := node as MeshInstance3D
		var transformed := _transformed_aabb(mesh_instance.get_aabb(), node_transform)
		if state["found"]:
			state["aabb"] = state["aabb"].merge(transformed)
		else:
			state["aabb"] = transformed
			state["found"] = true

	for child: Node in node.get_children():
		_collect_bounds(child, state, node_transform)


func _transformed_aabb(local_aabb: AABB, transform: Transform3D) -> AABB:
	var result := AABB()
	var first := true
	for x in 2:
		for y in 2:
			for z in 2:
				var corner := local_aabb.position + Vector3(
					local_aabb.size.x * float(x),
					local_aabb.size.y * float(y),
					local_aabb.size.z * float(z)
				)
				var point := transform * corner
				if first:
					result = AABB(point, Vector3.ZERO)
					first = false
				else:
					result = result.expand(point)
	return result


func _apply_material_override(root: Node, color: Color) -> void:
	for child: Node in root.get_children():
		if child is MeshInstance3D:
			(child as MeshInstance3D).material_override = _material(color)
		_apply_material_override(child, color)


func _fallback_subject(item_id: String) -> Node3D:
	match item_id:
		"copper_cable":
			return _fallback_cable(ItemCatalogScript.color(item_id))
		"coal":
			return _fallback_lump(ItemCatalogScript.color(item_id))
		_:
			return _fallback_stack(ItemCatalogScript.color(item_id))


func _fallback_stack(color: Color) -> Node3D:
	var root := Node3D.new()
	_make_box(root, "Plate0", Vector3(0.0, 0.08, 0.0), Vector3(1.0, 0.12, 0.74), color)
	_make_box(root, "Plate1", Vector3(0.0, 0.22, 0.0), Vector3(0.92, 0.12, 0.68), color.lightened(0.08))
	_make_box(root, "Plate2", Vector3(0.0, 0.36, 0.0), Vector3(0.84, 0.12, 0.62), color.lightened(0.14))
	return root


func _fallback_lump(color: Color) -> Node3D:
	var root := Node3D.new()
	_make_box(root, "Lump0", Vector3(-0.22, 0.16, 0.0), Vector3(0.42, 0.30, 0.38), color)
	_make_box(root, "Lump1", Vector3(0.18, 0.20, -0.08), Vector3(0.48, 0.36, 0.34), color.lightened(0.08))
	_make_box(root, "Lump2", Vector3(0.02, 0.42, 0.12), Vector3(0.36, 0.28, 0.32), color.darkened(0.08))
	return root


func _fallback_cable(color: Color) -> Node3D:
	var root := Node3D.new()
	_make_cylinder(root, "CableA", Vector3(-0.20, 0.22, 0.0), 0.055, 1.08, Vector3(90.0, 0.0, 0.0), color)
	_make_cylinder(root, "CableB", Vector3(0.0, 0.34, 0.0), 0.055, 1.08, Vector3(90.0, 0.0, 16.0), color.lightened(0.1))
	_make_cylinder(root, "CableC", Vector3(0.20, 0.22, 0.0), 0.055, 1.08, Vector3(90.0, 0.0, -16.0), color.darkened(0.08))
	return root


func _make_box(parent: Node3D, name: String, position: Vector3, size: Vector3, color: Color) -> MeshInstance3D:
	var instance := MeshInstance3D.new()
	instance.name = name
	var mesh := BoxMesh.new()
	mesh.size = size
	instance.mesh = mesh
	instance.position = position
	instance.material_override = _material(color)
	parent.add_child(instance)
	return instance


func _make_cylinder(
	parent: Node3D,
	name: String,
	position: Vector3,
	radius: float,
	height: float,
	rotation_degrees: Vector3,
	color: Color
) -> MeshInstance3D:
	var instance := MeshInstance3D.new()
	instance.name = name
	var mesh := CylinderMesh.new()
	mesh.top_radius = radius
	mesh.bottom_radius = radius
	mesh.height = height
	mesh.radial_segments = 24
	instance.mesh = mesh
	instance.position = position
	instance.rotation_degrees = rotation_degrees
	instance.material_override = _material(color)
	parent.add_child(instance)
	return instance


func _material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.76
	return material
