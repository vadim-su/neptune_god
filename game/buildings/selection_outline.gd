extends RefCounted
class_name SelectionOutline

const BuildingGeometryScript := preload("res://game/buildings/building_geometry.gd")
const OUTLINE_Y := 0.11
const OUTLINE_THICKNESS := 0.055
const OUTLINE_HEIGHT := 0.03
const OUTLINE_COLOR := Color(1.0, 0.88, 0.12, 0.95)


static func sync(root: Node3D, footprint: Array) -> void:
	if root == null:
		return

	_clear_children(root)
	if footprint.is_empty():
		root.visible = false
		return

	var bounds := BuildingGeometryScript.footprint_bounds(footprint)
	var left := float(bounds["min_x"]) - 0.5
	var right := float(bounds["max_x"]) + 0.5
	var bottom := float(bounds["min_y"]) - 0.5
	var top := float(bounds["max_y"]) + 0.5
	var center_x := (left + right) * 0.5
	var center_z := (bottom + top) * 0.5
	var width := right - left
	var depth := top - bottom
	var material := _transparent_material(OUTLINE_COLOR)

	_add_edge(
		root,
		Vector3(center_x, OUTLINE_Y, bottom),
		Vector3(width + OUTLINE_THICKNESS, OUTLINE_HEIGHT, OUTLINE_THICKNESS),
		material
	)
	_add_edge(
		root,
		Vector3(center_x, OUTLINE_Y, top),
		Vector3(width + OUTLINE_THICKNESS, OUTLINE_HEIGHT, OUTLINE_THICKNESS),
		material
	)
	_add_edge(
		root,
		Vector3(left, OUTLINE_Y, center_z),
		Vector3(OUTLINE_THICKNESS, OUTLINE_HEIGHT, depth + OUTLINE_THICKNESS),
		material
	)
	_add_edge(
		root,
		Vector3(right, OUTLINE_Y, center_z),
		Vector3(OUTLINE_THICKNESS, OUTLINE_HEIGHT, depth + OUTLINE_THICKNESS),
		material
	)
	root.visible = true


static func hide(root: Node3D) -> void:
	if root != null:
		root.visible = false


static func _add_edge(root: Node3D, position: Vector3, size: Vector3, material: Material) -> void:
	var instance := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = size
	instance.mesh = mesh
	instance.position = position
	instance.material_override = material
	root.add_child(instance)


static func _transparent_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	return material


static func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
