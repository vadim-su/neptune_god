extends Node3D

const FLOOR_SIZE := Vector2(12.0, 8.0)
const GRID_STEP := 1.0

const MODEL_PLACEMENTS := [
	{
		"name": "IronOre",
		"path": "res://assets/models/iron_ore.blend",
		"position": Vector3(-4.0, 0.0, -2.0),
		"rotation_degrees": 0.0,
	},
	{
		"name": "CopperOre",
		"path": "res://assets/models/copper_ore.blend",
		"position": Vector3(-4.0, 0.0, -3.0),
		"rotation_degrees": 0.0,
	},
	{
		"name": "BasicMiningDrill",
		"path": "res://assets/models/basic_mining_drill.blend",
		"position": Vector3(-2.8, 0.0, -2.2),
		"rotation_degrees": 0.0,
	},
	{
		"name": "ConveyorBeltStraightA",
		"path": "res://assets/models/conveyor_belt_straight.blend",
		"position": Vector3(-2.0, 0.0, 0.0),
		"rotation_degrees": 0.0,
	},
	{
		"name": "ConveyorBeltStraightB",
		"path": "res://assets/models/conveyor_belt_straight.blend",
		"position": Vector3(-1.0, 0.0, 0.0),
		"rotation_degrees": 0.0,
	},
	{
		"name": "ConveyorBeltCorner",
		"path": "res://assets/models/conveyor_belt_corner.blend",
		"position": Vector3(0.0, 0.0, 0.0),
		"rotation_degrees": 0.0,
	},
	{
		"name": "ConveyorSplitter",
		"path": "res://assets/models/conveyor_splitter.blend",
		"position": Vector3(1.2, 0.0, 0.0),
		"rotation_degrees": 0.0,
	},
	{
		"name": "IndustrialRobotArm",
		"path": "res://assets/models/industrial_robot_arm.blend",
		"position": Vector3(1.2, 0.0, -1.1),
		"rotation_degrees": 0.0,
	},
	{
		"name": "StoneIndustrialFurnace",
		"path": "res://assets/models/stone_industrial_furnace.blend",
		"position": Vector3(2.7, 0.0, -2.0),
		"rotation_degrees": -90.0,
	},
	{
		"name": "Radar",
		"path": "res://assets/models/radar.blend",
		"position": Vector3(4.0, 0.0, 1.5),
		"rotation_degrees": 35.0,
	},
	{
		"name": "PlayerEngineer",
		"path": "res://assets/models/player_engineer_refined.blend",
		"position": Vector3(0.0, 0.0, 1.2),
		"rotation_degrees": 180.0,
	},
	{
		"name": "RawCrystals",
		"path": "res://assets/models/raw_crystals.blend",
		"position": Vector3(-2.2, 0.0, 2.2),
		"rotation_degrees": 0.0,
	},
	{
		"name": "PileOfOre",
		"path": "res://assets/models/pile_of_ore.blend",
		"position": Vector3(-0.9, 0.0, 2.1),
		"rotation_degrees": 0.0,
	},
	{
		"name": "Wood",
		"path": "res://assets/models/wood.blend",
		"position": Vector3(0.2, 0.0, 2.2),
		"rotation_degrees": 20.0,
	},
	{
		"name": "IronSheet",
		"path": "res://assets/models/iron_sheet.blend",
		"position": Vector3(1.2, 0.0, 2.2),
		"rotation_degrees": -15.0,
	},
	{
		"name": "SteelBeams",
		"path": "res://assets/models/steel_beams.blend",
		"position": Vector3(2.3, 0.0, 2.1),
		"rotation_degrees": 0.0,
	},
]


func _ready() -> void:
	_add_floor()
	_add_grid()
	_add_model_preview()


func _add_floor() -> void:
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.075, 0.085, 0.075)
	material.roughness = 0.85

	var mesh := PlaneMesh.new()
	mesh.size = FLOOR_SIZE

	var floor := MeshInstance3D.new()
	floor.name = "Floor"
	floor.mesh = mesh
	floor.material_override = material
	floor.position.y = -0.01
	add_child(floor)


func _add_grid() -> void:
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.28, 0.34, 0.32)
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED

	var mesh := ImmediateMesh.new()
	mesh.surface_begin(Mesh.PRIMITIVE_LINES, material)

	var half_x := FLOOR_SIZE.x * 0.5
	var half_z := FLOOR_SIZE.y * 0.5
	var x := -half_x
	while x <= half_x:
		mesh.surface_add_vertex(Vector3(x, 0.0, -half_z))
		mesh.surface_add_vertex(Vector3(x, 0.0, half_z))
		x += GRID_STEP

	var z := -half_z
	while z <= half_z:
		mesh.surface_add_vertex(Vector3(-half_x, 0.0, z))
		mesh.surface_add_vertex(Vector3(half_x, 0.0, z))
		z += GRID_STEP

	mesh.surface_end()

	var grid := MeshInstance3D.new()
	grid.name = "TileGrid"
	grid.mesh = mesh
	grid.position.y = 0.012
	add_child(grid)


func _add_model_preview() -> void:
	var preview_root := Node3D.new()
	preview_root.name = "ModelPreview"
	add_child(preview_root)

	for placement: Dictionary in MODEL_PLACEMENTS:
		var model_path: String = placement["path"]
		var scene: Resource = load(model_path)
		if not scene is PackedScene:
			push_warning("Could not load model scene: %s" % model_path)
			continue

		var instance: Node3D = (scene as PackedScene).instantiate() as Node3D
		if instance == null:
			push_warning("Model root is not a Node3D: %s" % model_path)
			continue

		instance.name = placement["name"]
		instance.position = placement["position"]
		instance.rotation.y = deg_to_rad(placement["rotation_degrees"])
		preview_root.add_child(instance)
