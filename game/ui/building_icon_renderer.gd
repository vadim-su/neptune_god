extends Node
class_name BuildingIconRenderer

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")

const VIEWPORT_SIZE := Vector2i(192, 192)
const SUBJECT_TARGET_SIZE := 1.75

var _texture_cache: Dictionary = {}
var _scene_cache: Dictionary = {}


func prepare_icons(def_ids: Array) -> void:
	for raw_def_id: Variant in def_ids:
		texture_for(str(raw_def_id))


func texture_for(def_id: String) -> Texture2D:
	if def_id.is_empty():
		return null
	if _texture_cache.has(def_id):
		return _texture_cache[def_id]

	var viewport := _build_viewport(def_id)
	add_child(viewport)
	var texture := viewport.get_texture()
	_texture_cache[def_id] = texture
	return texture


func _build_viewport(def_id: String) -> SubViewport:
	var viewport := SubViewport.new()
	viewport.name = "IconViewport_%s" % def_id
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

	var subject := _instantiate_subject(def_id)
	viewport.add_child(subject)
	_normalize_subject(subject)

	var key_light := DirectionalLight3D.new()
	key_light.name = "KeyLight"
	key_light.rotation_degrees = Vector3(-52.0, 34.0, -18.0)
	key_light.light_energy = 2.2
	key_light.shadow_enabled = false
	viewport.add_child(key_light)

	var fill_light := OmniLight3D.new()
	fill_light.name = "FillLight"
	fill_light.position = Vector3(-2.4, 1.8, 1.8)
	fill_light.light_energy = 1.45
	fill_light.omni_range = 6.0
	viewport.add_child(fill_light)

	var camera := Camera3D.new()
	camera.name = "IconCamera"
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 1.85
	camera.look_at_from_position(Vector3(2.35, 1.7, 2.55), Vector3(0.0, 0.16, 0.0), Vector3.UP)
	camera.current = true
	viewport.add_child(camera)

	return viewport


func _icon_environment() -> Environment:
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.0, 0.0, 0.0, 0.0)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.72, 0.76, 0.70, 1.0)
	environment.ambient_light_energy = 0.74
	environment.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	return environment


func _instantiate_subject(def_id: String) -> Node3D:
	var root := Node3D.new()
	root.name = "IconSubject_%s" % def_id
	root.rotation_degrees = _subject_rotation(def_id)

	var model := _instantiate_model(def_id)
	if model != null:
		root.add_child(model)
		_add_variant_marks(root, def_id)
		return root

	var fallback := _fallback_subject(def_id)
	root.add_child(fallback)
	return root


func _instantiate_model(def_id: String) -> Node3D:
	var path := BuildingCatalogScript.model_path(def_id)
	if path.is_empty():
		return null

	var scene := _scene_for_path(path)
	if scene == null:
		push_warning("Missing icon model scene for %s at %s" % [def_id, path])
		return null

	var instance := scene.instantiate() as Node3D
	if instance == null:
		push_warning("Icon model scene is not Node3D for %s at %s" % [def_id, path])
		return null

	return instance


func _scene_for_path(path: String) -> PackedScene:
	if _scene_cache.has(path):
		return _scene_cache[path] as PackedScene

	var scene: PackedScene = load(path) as PackedScene
	if scene != null:
		_scene_cache[path] = scene
	return scene


func _subject_rotation(def_id: String) -> Vector3:
	match def_id:
		"basic_inserter":
			return Vector3(0.0, -22.0, 0.0)
		"basic_splitter":
			return Vector3(0.0, 34.0, 0.0)
		"basic_belt", "accelerated_belt", "fast_belt", "basic_underground_belt":
			return Vector3(0.0, -36.0, 0.0)
		_:
			return Vector3(0.0, -28.0, 0.0)


func _normalize_subject(subject: Node3D) -> void:
	var bounds: AABB = _combined_global_aabb(subject)
	if bounds.size == Vector3.ZERO:
		return

	var center: Vector3 = bounds.get_center()
	subject.position -= center

	var largest_axis: float = maxf(bounds.size.x, maxf(bounds.size.y, bounds.size.z))
	if largest_axis <= 0.001:
		return

	var scale_factor: float = SUBJECT_TARGET_SIZE / largest_axis
	subject.scale *= scale_factor


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


func _add_variant_marks(parent: Node3D, def_id: String) -> void:
	var mark_color := Color.TRANSPARENT
	var mark_count := 0
	match def_id:
		"accelerated_belt":
			mark_color = Color(0.36, 0.76, 0.95, 1.0)
			mark_count = 1
		"fast_belt":
			mark_color = Color(0.96, 0.77, 0.24, 1.0)
			mark_count = 2
		_:
			return

	for index in mark_count:
		_make_box(
			parent,
			"SpeedMark%d" % index,
			Vector3(-0.18 + float(index) * 0.36, 0.18, -0.02),
			Vector3(0.08, 0.035, 0.68),
			mark_color
		)


func _fallback_subject(def_id: String) -> Node3D:
	match def_id:
		"wooden_chest":
			return _fallback_chest()
		"basic_assembler":
			return _fallback_assembler()
		"basic_underground_belt":
			return _fallback_underground_belt()
		_:
			return _fallback_crate(def_id)


func _fallback_chest() -> Node3D:
	var root := Node3D.new()
	_make_box(root, "ChestBody", Vector3(0.0, 0.25, 0.0), Vector3(1.02, 0.48, 0.72), Color(0.46, 0.28, 0.14, 1.0))
	_make_box(root, "ChestLid", Vector3(0.0, 0.55, 0.0), Vector3(1.08, 0.20, 0.78), Color(0.58, 0.36, 0.18, 1.0))
	_make_box(root, "LeftBand", Vector3(-0.34, 0.42, 0.0), Vector3(0.06, 0.54, 0.82), Color(0.24, 0.22, 0.18, 1.0))
	_make_box(root, "RightBand", Vector3(0.34, 0.42, 0.0), Vector3(0.06, 0.54, 0.82), Color(0.24, 0.22, 0.18, 1.0))
	_make_box(root, "Latch", Vector3(0.0, 0.39, -0.42), Vector3(0.18, 0.18, 0.05), Color(0.78, 0.62, 0.30, 1.0))
	return root


func _fallback_assembler() -> Node3D:
	var root := Node3D.new()
	_make_box(root, "Base", Vector3(0.0, 0.30, 0.0), Vector3(1.14, 0.56, 0.92), Color(0.25, 0.34, 0.44, 1.0))
	_make_box(root, "TopPlate", Vector3(0.0, 0.66, 0.0), Vector3(0.92, 0.16, 0.70), Color(0.42, 0.50, 0.58, 1.0))
	_make_box(root, "InputPort", Vector3(-0.66, 0.34, 0.0), Vector3(0.24, 0.22, 0.42), Color(0.18, 0.22, 0.24, 1.0))
	_make_box(root, "OutputPort", Vector3(0.66, 0.34, 0.0), Vector3(0.24, 0.22, 0.42), Color(0.18, 0.22, 0.24, 1.0))
	_make_cylinder(root, "CenterDrum", Vector3(0.0, 0.80, 0.0), 0.25, 0.22, Vector3(90.0, 0.0, 0.0), Color(0.34, 0.42, 0.50, 1.0))
	return root


func _fallback_underground_belt() -> Node3D:
	var root := Node3D.new()
	_make_box(root, "BeltBed", Vector3(0.0, 0.10, 0.0), Vector3(1.12, 0.20, 0.86), Color(0.15, 0.17, 0.19, 1.0))
	_make_box(root, "LeftRamp", Vector3(-0.28, 0.32, 0.0), Vector3(0.38, 0.44, 0.80), Color(0.19, 0.19, 0.25, 1.0))
	_make_box(root, "RightRamp", Vector3(0.28, 0.24, 0.0), Vector3(0.38, 0.28, 0.80), Color(0.12, 0.13, 0.17, 1.0))
	_make_box(root, "Mouth", Vector3(-0.48, 0.56, 0.0), Vector3(0.18, 0.18, 0.76), Color(0.36, 0.38, 0.42, 1.0))
	_make_box(root, "Roller", Vector3(0.18, 0.34, -0.02), Vector3(0.58, 0.08, 0.58), Color(0.36, 0.72, 0.86, 1.0))
	return root


func _fallback_crate(def_id: String) -> Node3D:
	var root := Node3D.new()
	_make_box(root, "Body", Vector3(0.0, 0.32, 0.0), Vector3(0.94, 0.64, 0.94), BuildingCatalogScript.color(def_id))
	_make_box(root, "Top", Vector3(0.0, 0.72, 0.0), Vector3(0.72, 0.12, 0.72), Color(0.70, 0.72, 0.66, 1.0))
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
	material.roughness = 0.78
	return material
