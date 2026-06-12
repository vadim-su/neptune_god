extends Node3D

@onready var camera: Camera3D = %Camera3D
@onready var player: PlayerController = %Player
@onready var buildings_root: Node3D = %Buildings
@onready var status_label: Label = %StatusLabel
@onready var hotbar = %Hotbar
@onready var environment: Node3D = $Environment

const MAP_RADIUS := 72
const CAMERA_MIN_DISTANCE := 10.0
const CAMERA_MAX_DISTANCE := 140.0
const CAMERA_TARGET_HEIGHT := 0.75
const BUILD_GHOST_Y := 0.075
const BUILDING_VISUAL_Y := 0.24
const PLAYER_COLLISION_RADIUS := 0.32
const GHOST_VALID_COLOR := Color(0.28, 0.78, 1.0, 0.38)
const GHOST_BLOCKED_COLOR := Color(1.0, 0.22, 0.18, 0.38)
const INTERFACE_PANEL_BG := Color(0.070, 0.075, 0.065, 0.94)
const INTERFACE_PANEL_BORDER := Color(0.560, 0.760, 0.420, 0.72)

enum BuildMode { NEUTRAL, BUILD }

const BUILDING_MODEL_PATHS := {
	"basic_miner": "res://assets/models/buildings/basic_mining_drill.blend",
	"basic_belt": "res://assets/models/logistics/conveyor_belt_straight.blend",
	"accelerated_belt": "res://assets/models/logistics/conveyor_belt_straight.blend",
	"fast_belt": "res://assets/models/logistics/conveyor_belt_straight.blend",
	"basic_splitter": "res://assets/models/logistics/conveyor_splitter.blend",
	"basic_inserter": "res://assets/models/buildings/industrial_robot_arm.blend",
	"stone_furnace": "res://assets/models/buildings/stone_industrial_furnace.blend",
}

const BUILDING_COLORS := {
	"basic_miner": Color(0.38, 0.48, 0.36),
	"wooden_chest": Color(0.48, 0.32, 0.18),
	"basic_belt": Color(0.18, 0.22, 0.23),
	"stone_furnace": Color(0.42, 0.40, 0.36),
	"basic_inserter": Color(0.54, 0.46, 0.22),
	"basic_assembler": Color(0.28, 0.36, 0.46),
	"accelerated_belt": Color(0.24, 0.30, 0.34),
	"fast_belt": Color(0.18, 0.28, 0.38),
	"basic_splitter": Color(0.24, 0.24, 0.30),
	"basic_underground_belt": Color(0.18, 0.18, 0.22),
}

const WALKABLE_BUILDING_IDS := {
	"basic_belt": true,
	"accelerated_belt": true,
	"fast_belt": true,
	"basic_splitter": true,
	"basic_underground_belt": true,
}

var sim: NeptuneSim
var camera_yaw := deg_to_rad(42.0)
var camera_elevation := deg_to_rad(58.0)
var camera_distance := 32.0
var build_mode := BuildMode.NEUTRAL
var selected_building_id := ""
var selected_object_id := -1
var selected_object: Dictionary = {}
var build_quarter_turns := 0
var build_preview_tile := Vector2i.ZERO
var build_preview_valid := false
var build_ghost_root: Node3D
var blocked_building_tiles := {}
var building_tile_index := {}
var building_interface_panel: PanelContainer
var building_interface_title: Label
var building_interface_body: Label


func _ready() -> void:
	sim = NeptuneSim.new()
	sim.generate_starting_map(MAP_RADIUS)
	player.can_move_to = Callable(self, "_is_player_position_walkable")
	environment.build_from_sim(sim)
	hotbar.selected.connect(_on_hotbar_selected)
	build_ghost_root = Node3D.new()
	build_ghost_root.name = "BuildGhost"
	add_child(build_ghost_root)
	_building_interface_ui()
	sim.tick_many(3)
	_update_status_label()
	_update_camera()
	print(status_label.text.replace("\n", " | "))


func _process(_delta: float) -> void:
	_update_camera()
	_update_build_preview()


func _physics_process(_delta: float) -> void:
	player.movement_yaw = camera_yaw


func _input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.is_mouse_button_pressed(MOUSE_BUTTON_MIDDLE):
		camera_yaw -= event.relative.x * 0.006
		camera_elevation = clamp(
			camera_elevation - event.relative.y * 0.006,
			deg_to_rad(18.0),
			deg_to_rad(76.0)
		)
		_update_camera()
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.pressed:
		match event.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				camera_distance = max(CAMERA_MIN_DISTANCE, camera_distance - 0.5)
				_update_camera()
				get_viewport().set_input_as_handled()
			MOUSE_BUTTON_WHEEL_DOWN:
				camera_distance = min(CAMERA_MAX_DISTANCE, camera_distance + 0.5)
				_update_camera()
				get_viewport().set_input_as_handled()
			MOUSE_BUTTON_RIGHT:
				if not _is_pointer_over_ui():
					if build_mode == BuildMode.BUILD:
						_enter_neutral_mode()
					else:
						_try_remove_building_at_pointer()
					get_viewport().set_input_as_handled()
			MOUSE_BUTTON_LEFT:
				if not _is_pointer_over_ui():
					if build_mode == BuildMode.BUILD:
						_try_place_selected_building()
					else:
						_try_select_building_at_pointer()
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.keycode == KEY_ESCAPE:
			if build_mode == BuildMode.BUILD:
				_enter_neutral_mode()
			else:
				_clear_selected_object()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_R and build_mode == BuildMode.BUILD and not selected_building_id.is_empty():
			build_quarter_turns = (build_quarter_turns + 1) % 4
			_update_build_preview()
			get_viewport().set_input_as_handled()


func _is_pointer_over_ui() -> bool:
	return get_viewport().gui_get_hovered_control() != null


func _update_camera() -> void:
	var target: Vector3 = player.global_position + Vector3.UP * CAMERA_TARGET_HEIGHT
	var horizontal_distance := camera_distance * cos(camera_elevation)
	var offset := Vector3(
		horizontal_distance * sin(camera_yaw),
		camera_distance * sin(camera_elevation),
		horizontal_distance * cos(camera_yaw)
	)
	camera.global_position = target + offset
	camera.look_at(target, Vector3.UP)


func _on_hotbar_selected(entry_id: String) -> void:
	if entry_id.is_empty():
		_enter_neutral_mode()
		return

	build_mode = BuildMode.BUILD
	selected_building_id = entry_id
	_clear_selected_object()
	build_quarter_turns = 0
	_update_build_preview()
	_update_status_label()


func _update_status_label() -> void:
	var object_text := "none"
	if not selected_object.is_empty():
		object_text = "%s #%d" % [str(selected_object["def_id"]), selected_object_id]

	status_label.text = "Neptune Godot runtime loaded\nTick: %d\nDigest: %d\nTiles: %d\nResources: %d\nMode: %s\nBuild: %s\nObject: %s" % [
		sim.core_tick(),
		sim.digest(),
		sim.map_tile_count(),
		sim.resource_count(),
		_build_mode_label(),
		selected_building_id,
		object_text,
	]


func _update_build_preview() -> void:
	if build_mode != BuildMode.BUILD or selected_building_id.is_empty():
		build_ghost_root.visible = false
		build_preview_valid = false
		return

	var tile_variant: Variant = _mouse_ground_tile()
	if tile_variant == null:
		build_ghost_root.visible = false
		build_preview_valid = false
		return

	var tile: Vector2i = tile_variant
	build_preview_tile = tile
	var footprint: Array = sim.building_footprint(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	)
	if footprint.is_empty():
		build_ghost_root.visible = false
		build_preview_valid = false
		return

	build_preview_valid = sim.can_place_building(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	) and _footprint_allows_player(selected_building_id, footprint)
	_sync_ghost_tiles(footprint, GHOST_VALID_COLOR if build_preview_valid else GHOST_BLOCKED_COLOR)
	build_ghost_root.visible = true


func _try_place_selected_building() -> void:
	if selected_building_id.is_empty() or not build_preview_valid:
		return

	var footprint: Array = sim.building_footprint(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	)
	if not sim.can_place_building(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	) or not _footprint_allows_player(selected_building_id, footprint):
		build_preview_valid = false
		_sync_ghost_tiles(footprint, GHOST_BLOCKED_COLOR)
		return

	if sim.place_building(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	):
		_render_buildings_from_sim()
		_update_build_preview()
		_update_status_label()
		get_viewport().set_input_as_handled()


func _try_select_building_at_pointer() -> void:
	var tile_variant: Variant = _mouse_ground_tile()
	if tile_variant == null:
		_clear_selected_object()
		return

	var tile: Vector2i = tile_variant
	var building := _building_at_tile(tile)
	if building.is_empty():
		_clear_selected_object()
		return

	_select_object(building)
	get_viewport().set_input_as_handled()


func _try_remove_building_at_pointer() -> void:
	var tile_variant: Variant = _mouse_ground_tile()
	if tile_variant == null:
		return

	var tile: Vector2i = tile_variant
	var building := _building_at_tile(tile)
	if building.is_empty():
		return

	var removed_id := int(building["id"])
	if not sim.remove_building(tile.x, tile.y):
		return

	if selected_object_id == removed_id:
		_clear_selected_object()
	_render_buildings_from_sim()
	_update_build_preview()
	_update_status_label()


func _mouse_ground_tile() -> Variant:
	var mouse_position := get_viewport().get_mouse_position()
	var ray_origin := camera.project_ray_origin(mouse_position)
	var ray_direction := camera.project_ray_normal(mouse_position)
	if abs(ray_direction.y) < 0.0001:
		return null

	var distance := -ray_origin.y / ray_direction.y
	if distance < 0.0:
		return null

	var hit := ray_origin + ray_direction * distance
	return Vector2i(int(round(hit.x)), int(round(hit.z)))


func _sync_ghost_tiles(footprint: Array, color: Color) -> void:
	_clear_children(build_ghost_root)
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		var instance := MeshInstance3D.new()
		var mesh := PlaneMesh.new()
		mesh.size = Vector2(0.96, 0.96)
		instance.mesh = mesh
		instance.position = Vector3(float(tile["x"]), BUILD_GHOST_Y, float(tile["y"]))
		instance.material_override = _transparent_material(color)
		build_ghost_root.add_child(instance)


func _render_buildings_from_sim() -> void:
	_clear_children(buildings_root)
	blocked_building_tiles.clear()
	building_tile_index.clear()
	var snapshots: Array = sim.buildings()
	for raw_building: Variant in snapshots:
		var building: Dictionary = raw_building
		var building_node := Node3D.new()
		building_node.name = "Building_%s" % str(building["id"])
		buildings_root.add_child(building_node)

		var def_id := str(building["def_id"])
		var color: Color = BUILDING_COLORS.get(def_id, Color(0.34, 0.36, 0.34))
		var material := _solid_material(color)
		var footprint: Array = building["footprint"]
		_index_building_tiles(building, footprint)
		if not _is_walkable_building(def_id):
			_add_blocked_building_tiles(footprint)
		var model := _instantiate_building_model(def_id)
		if model != null:
			model.position = _footprint_center(footprint)
			model.rotation.y = _rotation_y_for_quarter_turns(int(building["quarter_turns"]))
			building_node.add_child(model)
		else:
			_add_fallback_building_tiles(building_node, footprint, material)
	_refresh_selected_object_after_render()


func _index_building_tiles(building: Dictionary, footprint: Array) -> void:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		building_tile_index[Vector2i(int(tile["x"]), int(tile["y"]))] = building


func _building_at_tile(tile: Vector2i) -> Dictionary:
	if not building_tile_index.has(tile):
		return {}
	return building_tile_index[tile]


func _building_by_id(id: int) -> Dictionary:
	for building: Dictionary in building_tile_index.values():
		if int(building["id"]) == id:
			return building
	return {}


func _refresh_selected_object_after_render() -> void:
	if selected_object_id == -1:
		return

	var building := _building_by_id(selected_object_id)
	if building.is_empty():
		_clear_selected_object()
		return

	selected_object = building
	_update_building_interface()


func _enter_neutral_mode() -> void:
	build_mode = BuildMode.NEUTRAL
	build_preview_valid = false
	if build_ghost_root != null:
		build_ghost_root.visible = false
	_update_status_label()


func _build_mode_label() -> String:
	return "Build" if build_mode == BuildMode.BUILD else "Neutral"


func _select_object(building: Dictionary) -> void:
	selected_object_id = int(building["id"])
	selected_object = building
	_update_building_interface()
	_update_status_label()


func _clear_selected_object() -> void:
	selected_object_id = -1
	selected_object.clear()
	if building_interface_panel != null:
		building_interface_panel.visible = false
	_update_status_label()


func _building_interface_ui() -> void:
	building_interface_panel = PanelContainer.new()
	building_interface_panel.name = "BuildingInterface"
	building_interface_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	building_interface_panel.visible = false
	building_interface_panel.anchor_left = 1.0
	building_interface_panel.anchor_right = 1.0
	building_interface_panel.anchor_top = 0.0
	building_interface_panel.anchor_bottom = 0.0
	building_interface_panel.offset_left = -320.0
	building_interface_panel.offset_top = 16.0
	building_interface_panel.offset_right = -16.0
	building_interface_panel.offset_bottom = 168.0
	building_interface_panel.add_theme_stylebox_override("panel", _interface_stylebox())
	$Hud.add_child(building_interface_panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 12)
	margin.add_theme_constant_override("margin_top", 12)
	margin.add_theme_constant_override("margin_right", 12)
	margin.add_theme_constant_override("margin_bottom", 12)
	building_interface_panel.add_child(margin)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 8)
	margin.add_child(column)

	building_interface_title = Label.new()
	building_interface_title.add_theme_font_size_override("font_size", 18)
	building_interface_title.add_theme_color_override("font_color", Color(0.92, 0.94, 0.86))
	column.add_child(building_interface_title)

	building_interface_body = Label.new()
	building_interface_body.add_theme_font_size_override("font_size", 14)
	building_interface_body.add_theme_color_override("font_color", Color(0.78, 0.80, 0.74))
	column.add_child(building_interface_body)


func _update_building_interface() -> void:
	if selected_object.is_empty() or building_interface_panel == null:
		return

	var footprint: Array = selected_object["footprint"]
	var origin := Vector2i(int(selected_object["x"]), int(selected_object["y"]))
	building_interface_title.text = "%s #%d" % [str(selected_object["def_id"]), selected_object_id]
	building_interface_body.text = "Origin: %d, %d\nRotation: %d deg\nFootprint: %d tile(s)" % [
		origin.x,
		origin.y,
		int(selected_object["quarter_turns"]) * 90,
		footprint.size(),
	]
	building_interface_panel.visible = true


func _interface_stylebox() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = INTERFACE_PANEL_BG
	style.border_color = INTERFACE_PANEL_BORDER
	style.border_width_left = 1
	style.border_width_top = 1
	style.border_width_right = 1
	style.border_width_bottom = 1
	return style


func _add_blocked_building_tiles(footprint: Array) -> void:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		blocked_building_tiles[Vector2i(int(tile["x"]), int(tile["y"]))] = true


func _is_walkable_building(def_id: String) -> bool:
	return WALKABLE_BUILDING_IDS.has(def_id)


func _footprint_allows_player(def_id: String, footprint: Array) -> bool:
	if _is_walkable_building(def_id):
		return true
	return not _footprint_overlaps_player(footprint)


func _footprint_overlaps_player(footprint: Array) -> bool:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		if _player_overlaps_tile(player.global_position, Vector2i(int(tile["x"]), int(tile["y"]))):
			return true
	return false


func _is_player_position_walkable(position: Vector3) -> bool:
	var min_tile_x := int(ceil(position.x - 0.5 - PLAYER_COLLISION_RADIUS))
	var max_tile_x := int(floor(position.x + 0.5 + PLAYER_COLLISION_RADIUS))
	var min_tile_y := int(ceil(position.z - 0.5 - PLAYER_COLLISION_RADIUS))
	var max_tile_y := int(floor(position.z + 0.5 + PLAYER_COLLISION_RADIUS))

	for tile_x in range(min_tile_x, max_tile_x + 1):
		for tile_y in range(min_tile_y, max_tile_y + 1):
			var tile := Vector2i(tile_x, tile_y)
			if blocked_building_tiles.has(tile) and _player_overlaps_tile(position, tile):
				return false
	return true


func _player_overlaps_tile(position: Vector3, tile: Vector2i) -> bool:
	var closest_x: float = clamp(position.x, float(tile.x) - 0.5, float(tile.x) + 0.5)
	var closest_z: float = clamp(position.z, float(tile.y) - 0.5, float(tile.y) + 0.5)
	var offset := Vector2(position.x - closest_x, position.z - closest_z)
	return offset.length_squared() < PLAYER_COLLISION_RADIUS * PLAYER_COLLISION_RADIUS


func _instantiate_building_model(def_id: String) -> Node3D:
	var path: String = BUILDING_MODEL_PATHS.get(def_id, "")
	if path.is_empty():
		return null

	var scene := load(path) as PackedScene
	if scene == null:
		push_warning("Missing building model scene for %s at %s" % [def_id, path])
		return null

	var instance := scene.instantiate() as Node3D
	if instance == null:
		push_warning("Building model scene is not Node3D for %s at %s" % [def_id, path])
		return null

	return instance


func _add_fallback_building_tiles(parent: Node3D, footprint: Array, material: Material) -> void:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		var instance := MeshInstance3D.new()
		var mesh := BoxMesh.new()
		mesh.size = Vector3(0.86, 0.48, 0.86)
		instance.mesh = mesh
		instance.position = Vector3(float(tile["x"]), BUILDING_VISUAL_Y, float(tile["y"]))
		instance.material_override = material
		parent.add_child(instance)


func _footprint_center(footprint: Array) -> Vector3:
	var min_x := INF
	var max_x := -INF
	var min_y := INF
	var max_y := -INF
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		var x := float(tile["x"])
		var y := float(tile["y"])
		min_x = min(min_x, x)
		max_x = max(max_x, x)
		min_y = min(min_y, y)
		max_y = max(max_y, y)

	return Vector3((min_x + max_x) * 0.5, 0.0, (min_y + max_y) * 0.5)


func _rotation_y_for_quarter_turns(quarter_turns: int) -> float:
	return deg_to_rad(float(quarter_turns * 90))


func _transparent_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	return material


func _solid_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.82
	return material


func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
