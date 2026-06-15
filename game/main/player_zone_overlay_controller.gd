extends RefCounted
class_name PlayerZoneOverlayController

const PLAYER_ZONE_RADIUS := 12.0
const PLAYER_ZONE_Y := 0.045

var player: Node3D
var visible_provider: Callable
var overlay: MeshInstance3D


func setup(parent: Node, player_node: Node3D, should_show_overlay: Callable) -> void:
	player = player_node
	visible_provider = should_show_overlay
	overlay = MeshInstance3D.new()
	overlay.name = "PlayerZoneOverlay"
	overlay.mesh = _player_zone_mesh()
	overlay.material_override = _transparent_material(Color(0.28, 0.78, 1.0, 0.18))
	parent.add_child(overlay)
	update()


func update() -> void:
	if overlay == null:
		return
	overlay.visible = should_show()
	if not overlay.visible or player == null:
		return
	overlay.global_position = Vector3(player.global_position.x, PLAYER_ZONE_Y, player.global_position.z)


func should_show() -> bool:
	if not visible_provider.is_valid():
		return true
	return bool(visible_provider.call())


func _player_zone_mesh() -> ArrayMesh:
	var segments := 96
	var inner_radius := PLAYER_ZONE_RADIUS - 0.08
	var outer_radius := PLAYER_ZONE_RADIUS + 0.08
	var vertices := PackedVector3Array()
	var normals := PackedVector3Array()
	var colors := PackedColorArray()
	var indices := PackedInt32Array()

	for index in range(segments):
		var angle := TAU * float(index) / float(segments)
		var direction := Vector3(cos(angle), 0.0, sin(angle))
		vertices.append(direction * inner_radius)
		vertices.append(direction * outer_radius)
		normals.append(Vector3.UP)
		normals.append(Vector3.UP)
		colors.append(Color(0.28, 0.78, 1.0, 0.10))
		colors.append(Color(0.28, 0.78, 1.0, 0.34))

	for index in range(segments):
		var next_index := (index + 1) % segments
		var inner_a := index * 2
		var outer_a := inner_a + 1
		var inner_b := next_index * 2
		var outer_b := inner_b + 1
		indices.append(inner_a)
		indices.append(outer_a)
		indices.append(outer_b)
		indices.append(inner_a)
		indices.append(outer_b)
		indices.append(inner_b)

	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_COLOR] = colors
	arrays[Mesh.ARRAY_INDEX] = indices

	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	return mesh


func _transparent_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	return material
