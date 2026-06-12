extends Node3D

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const BuildingRendererScript := preload("res://game/buildings/building_renderer.gd")
const SelectionOutlineScript := preload("res://game/buildings/selection_outline.gd")
const BuildingInspectorScript := preload("res://game/ui/building_inspector.gd")

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
const PLAYER_COLLISION_RADIUS := 0.32
const GHOST_VALID_COLOR := Color(0.28, 0.78, 1.0, 0.38)
const GHOST_BLOCKED_COLOR := Color(1.0, 0.22, 0.18, 0.38)

enum BuildMode { NEUTRAL, BUILD }

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
var selection_outline_root: Node3D
var building_inspector := BuildingInspectorScript.new()


func _ready() -> void:
	sim = NeptuneSim.new()
	sim.generate_starting_map(MAP_RADIUS)
	player.can_move_to = Callable(self, "_is_player_position_walkable")
	environment.build_from_sim(sim)
	hotbar.selected.connect(_on_hotbar_selected)
	build_ghost_root = Node3D.new()
	build_ghost_root.name = "BuildGhost"
	add_child(build_ghost_root)
	selection_outline_root = Node3D.new()
	selection_outline_root.name = "SelectionOutline"
	selection_outline_root.visible = false
	add_child(selection_outline_root)
	building_inspector.setup($Hud)
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
	BuildingRendererScript.render_from_sim(sim, buildings_root, building_tile_index, blocked_building_tiles)
	_refresh_selected_object_after_render()


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
	building_inspector.update(selected_object)
	SelectionOutlineScript.sync(selection_outline_root, building["footprint"])


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
	building_inspector.update(selected_object)
	SelectionOutlineScript.sync(selection_outline_root, building["footprint"])
	_update_status_label()


func _clear_selected_object() -> void:
	selected_object_id = -1
	selected_object.clear()
	building_inspector.hide()
	SelectionOutlineScript.hide(selection_outline_root)
	_update_status_label()


func _footprint_allows_player(def_id: String, footprint: Array) -> bool:
	if BuildingCatalogScript.is_walkable(def_id):
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


func _transparent_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	return material


func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
