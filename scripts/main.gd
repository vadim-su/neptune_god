extends Node3D

@onready var camera: Camera3D = %Camera3D
@onready var status_label: Label = %StatusLabel
@onready var hotbar = %Hotbar
@onready var environment: Node3D = $Environment

const MAP_RADIUS := 72
const CAMERA_TARGET := Vector3(0.0, 0.0, 0.0)
const CAMERA_MIN_DISTANCE := 10.0
const CAMERA_MAX_DISTANCE := 140.0

var sim: NeptuneSim
var camera_yaw := deg_to_rad(42.0)
var camera_elevation := deg_to_rad(58.0)
var camera_distance := 96.0
var selected_building_id := ""


func _ready() -> void:
	sim = NeptuneSim.new()
	sim.generate_starting_map(MAP_RADIUS)
	environment.build_from_sim(sim)
	hotbar.selected.connect(_on_hotbar_selected)
	selected_building_id = hotbar.selected_entry_id()
	sim.tick_many(3)
	_update_status_label()
	_update_camera()
	print(status_label.text.replace("\n", " | "))


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.is_mouse_button_pressed(MOUSE_BUTTON_RIGHT):
		camera_yaw -= event.relative.x * 0.006
		camera_elevation = clamp(
			camera_elevation - event.relative.y * 0.006,
			deg_to_rad(18.0),
			deg_to_rad(76.0)
		)
		_update_camera()
	elif event is InputEventMouseButton and event.pressed:
		match event.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				camera_distance = max(CAMERA_MIN_DISTANCE, camera_distance - 0.5)
				_update_camera()
			MOUSE_BUTTON_WHEEL_DOWN:
				camera_distance = min(CAMERA_MAX_DISTANCE, camera_distance + 0.5)
				_update_camera()


func _update_camera() -> void:
	var horizontal_distance := camera_distance * cos(camera_elevation)
	var offset := Vector3(
		horizontal_distance * sin(camera_yaw),
		camera_distance * sin(camera_elevation),
		horizontal_distance * cos(camera_yaw)
	)
	camera.global_position = CAMERA_TARGET + offset
	camera.look_at(CAMERA_TARGET, Vector3.UP)


func _on_hotbar_selected(entry_id: String) -> void:
	selected_building_id = entry_id
	_update_status_label()


func _update_status_label() -> void:
	status_label.text = "Neptune Godot runtime loaded\nTick: %d\nDigest: %d\nTiles: %d\nResources: %d\nSelected: %s" % [
		sim.core_tick(),
		sim.digest(),
		sim.map_tile_count(),
		sim.resource_count(),
		selected_building_id,
	]
