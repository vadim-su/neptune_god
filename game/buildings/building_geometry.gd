extends RefCounted
class_name BuildingGeometry


static func footprint_center(footprint: Array) -> Vector3:
	var bounds := footprint_bounds(footprint)
	return Vector3(
		(float(bounds["min_x"]) + float(bounds["max_x"])) * 0.5,
		0.0,
		(float(bounds["min_y"]) + float(bounds["max_y"])) * 0.5
	)


static func footprint_bounds(footprint: Array) -> Dictionary:
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

	return {
		"min_x": min_x,
		"max_x": max_x,
		"min_y": min_y,
		"max_y": max_y,
	}


static func footprint_size_text(footprint: Array) -> String:
	if footprint.is_empty():
		return "0 tiles"

	var bounds := footprint_bounds(footprint)
	var width := int(bounds["max_x"]) - int(bounds["min_x"]) + 1
	var depth := int(bounds["max_y"]) - int(bounds["min_y"]) + 1
	return "%dx%d (%d tiles)" % [width, depth, footprint.size()]


static func rotation_y_for_quarter_turns(quarter_turns: int) -> float:
	return deg_to_rad(float(quarter_turns * 90))
