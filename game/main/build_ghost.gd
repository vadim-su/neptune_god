extends Node3D
class_name BuildGhost

const BUILD_GHOST_Y := 0.075
const GHOST_VALID_COLOR := Color(0.28, 0.78, 1.0, 0.38)
const GHOST_BLOCKED_COLOR := Color(1.0, 0.22, 0.18, 0.38)


func show_footprint(footprint: Array, is_valid: bool) -> void:
	if footprint.is_empty():
		hide_preview()
		return
	_sync_tiles(footprint, GHOST_VALID_COLOR if is_valid else GHOST_BLOCKED_COLOR)
	visible = true


func hide_preview() -> void:
	visible = false
	_clear_children(self)


func _sync_tiles(footprint: Array, color: Color) -> void:
	_clear_children(self)
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		var instance := MeshInstance3D.new()
		var mesh := PlaneMesh.new()
		mesh.size = Vector2(0.96, 0.96)
		instance.mesh = mesh
		instance.position = Vector3(float(tile["x"]), BUILD_GHOST_Y, float(tile["y"]))
		instance.material_override = _transparent_material(color)
		add_child(instance)


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
