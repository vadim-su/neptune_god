extends Control

@onready var status_label: Label = %StatusLabel

var sim: NeptuneSim


func _ready() -> void:
	sim = NeptuneSim.new()
	sim.tick_many(3)
	status_label.text = "Neptune Godot runtime loaded\nTick: %d\nDigest: %d\nBuildings: %d" % [
		sim.core_tick(),
		sim.digest(),
		sim.building_count(),
	]
	print(status_label.text.replace("\n", " | "))
