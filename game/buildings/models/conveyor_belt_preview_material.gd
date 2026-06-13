@tool
extends Node3D

const ConveyorBeltSurfaceShader := preload("res://game/buildings/conveyor_belt_surface.gdshader")
const ConveyorBeltSurfaceTexture := preload("res://assets/images/buildings/belt_surface.png")

@export var preview_speed_tiles_per_second := 1.0:
	set(value):
		preview_speed_tiles_per_second = value
		_apply_preview_material()

@export var preview_tint := Color.WHITE:
	set(value):
		preview_tint = value
		_apply_preview_material()

@export var apply_at_runtime := false

var _preview_material: ShaderMaterial


func _ready() -> void:
	if Engine.is_editor_hint() or apply_at_runtime:
		_apply_preview_material()


func _notification(what: int) -> void:
	if what == NOTIFICATION_CHILD_ORDER_CHANGED and Engine.is_editor_hint():
		_apply_preview_material.call_deferred()


func _apply_preview_material() -> void:
	if not Engine.is_editor_hint() and not apply_at_runtime:
		return

	if _preview_material == null:
		_preview_material = ShaderMaterial.new()
		_preview_material.shader = ConveyorBeltSurfaceShader
		_preview_material.set_shader_parameter("belt_texture", ConveyorBeltSurfaceTexture)

	_preview_material.set_shader_parameter("belt_speed_tiles_per_second", preview_speed_tiles_per_second)
	_preview_material.set_shader_parameter("tint", preview_tint)
	_apply_to_belt_surfaces(self)


func _apply_to_belt_surfaces(node: Node) -> void:
	if node is MeshInstance3D:
		var mesh_instance := node as MeshInstance3D
		if _is_belt_surface_mesh(mesh_instance):
			mesh_instance.material_override = _preview_material

	for child: Node in node.get_children():
		_apply_to_belt_surfaces(child)


func _is_belt_surface_mesh(mesh_instance: MeshInstance3D) -> bool:
	var node_name := str(mesh_instance.name).to_lower()
	if node_name.contains("belt_surface"):
		return true
	if mesh_instance.mesh != null:
		var mesh_name := str(mesh_instance.mesh.resource_name).to_lower()
		return mesh_name.contains("belt_surface")
	return false
