extends Node

var _undo_manager: Node

const ALLOWED_NODE_TYPES: Array = [
	"Node3D", "MeshInstance3D", "StaticBody3D", "RigidBody3D",
	"CharacterBody3D", "Camera3D", "Light3D", "DirectionalLight3D",
	"OmniLight3D", "SpotLight3D", "CollisionShape3D", "RayCast3D",
	"Area3D", "Marker3D", "PathFollow3D", "VisibleOnScreenNotifier3D",
	"Node", "Node2D", "Sprite2D", "AnimatedSprite2D",
	"CollisionShape2D", "Area2D", "RigidBody2D", "CharacterBody2D",
	"AudioStreamPlayer", "AudioStreamPlayer2D", "AudioStreamPlayer3D",
	"AnimationPlayer", "AnimationTree", "Timer",
]

func setup(undo_manager: Node) -> void:
	_undo_manager = undo_manager

func handle_add_node(params: Dictionary, request_id: int) -> Dictionary:
	var ei = Engine.get_singleton("EditorInterface") as EditorInterface
	var root = ei.get_edited_scene_root()
	if not root:
		return {"error": {"code": -32003, "message": "No scene loaded"}}

	var node_type: String = params.get("node_type", "Node")
	var node_name: String = params.get("node_name", "NewNode")
	var parent_path: String = params.get("parent_node_path", "")

	if not _is_allowed_node_type(node_type):
		return {"error": {"code": -32004, "message": "Blocked node type: %s" % node_type}}

	var parent_node: Node = root
	if not parent_path.is_empty():
		parent_node = root.get_node(parent_path)
		if not parent_node:
			return {"error": {"code": -32002, "message": "Parent not found: %s" % parent_path}}

	var cls = ClassDB.instantiate(node_type)
	if not cls:
		return {"error": {"code": -32000, "message": "Cannot instantiate: %s" % node_type}}
	cls.name = node_name

	if _undo_manager != null:
		_undo_manager.create_action_mixed(request_id,
			[
				{"type": "method", "target": parent_node, "method": "add_child", "args": [cls]},
				{"type": "method", "target": cls, "method": "set_owner", "args": [root]},
				{"type": "reference", "value": cls}
			],
			[
				{"type": "method", "target": parent_node, "method": "remove_child", "args": [cls]}
			]
		)
	else:
		parent_node.add_child(cls)
		cls.owner = root
	return {"result": {"node_path": str(cls.get_path()), "status": "created"}}

func _is_allowed_node_type(node_type: String) -> bool:
	if node_type in ALLOWED_NODE_TYPES:
		return true
	if not ClassDB.class_exists(node_type):
		return false
	for allowed in ALLOWED_NODE_TYPES:
		if ClassDB.is_parent_class(node_type, allowed):
			return true
	return false
