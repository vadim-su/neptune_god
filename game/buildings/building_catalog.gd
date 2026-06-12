extends RefCounted
class_name BuildingCatalog

const MODEL_PATHS := {
	"basic_miner": "res://assets/models/buildings/basic_mining_drill.blend",
	"basic_belt": "res://assets/models/logistics/conveyor_belt_straight.blend",
	"accelerated_belt": "res://assets/models/logistics/conveyor_belt_straight.blend",
	"fast_belt": "res://assets/models/logistics/conveyor_belt_straight.blend",
	"basic_splitter": "res://assets/models/logistics/conveyor_splitter.blend",
	"basic_inserter": "res://assets/models/buildings/industrial_robot_arm.blend",
	"stone_furnace": "res://assets/models/buildings/stone_industrial_furnace.blend",
}

const COLORS := {
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

const DISPLAY_NAMES := {
	"basic_miner": "Miner",
	"wooden_chest": "Chest",
	"basic_belt": "Belt",
	"stone_furnace": "Stone Furnace",
	"basic_inserter": "Inserter",
	"basic_assembler": "Assembler",
	"accelerated_belt": "Accelerated Belt",
	"fast_belt": "Fast Belt",
	"basic_splitter": "Splitter",
	"basic_underground_belt": "Underground Belt",
}

const UI_TYPES := {
	"basic_miner": "Machine",
	"wooden_chest": "Container",
	"basic_belt": "Transport",
	"stone_furnace": "Machine",
	"basic_inserter": "Inserter",
	"basic_assembler": "Machine",
	"accelerated_belt": "Transport",
	"fast_belt": "Transport",
	"basic_splitter": "Transport",
	"basic_underground_belt": "Transport",
}

const WALKABLE_IDS := {
	"basic_belt": true,
	"accelerated_belt": true,
	"fast_belt": true,
	"basic_splitter": true,
	"basic_underground_belt": true,
}


static func model_path(def_id: String) -> String:
	return MODEL_PATHS.get(def_id, "")


static func color(def_id: String) -> Color:
	return COLORS.get(def_id, Color(0.34, 0.36, 0.34))


static func display_name(def_id: String) -> String:
	return DISPLAY_NAMES.get(def_id, def_id.replace("_", " ").capitalize())


static func ui_type(def_id: String) -> String:
	return UI_TYPES.get(def_id, "Building")


static func state_label(def_id: String) -> String:
	if def_id == "basic_inserter":
		return "inserter"
	if is_walkable(def_id):
		return "transport"
	if UI_TYPES.get(def_id, "") == "Machine":
		return "machine"
	return "idle"


static func is_walkable(def_id: String) -> bool:
	return WALKABLE_IDS.has(def_id)
