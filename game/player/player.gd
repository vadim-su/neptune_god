extends Node3D
class_name PlayerController

const MOVE_SPEED := 9.0
const RUN_MULTIPLIER := 1.55

var movement_yaw := 0.0
var velocity := Vector3.ZERO


func _physics_process(delta: float) -> void:
	var input := _movement_input()
	var forward := Vector3(-sin(movement_yaw), 0.0, -cos(movement_yaw))
	var right := Vector3(cos(movement_yaw), 0.0, -sin(movement_yaw))
	var direction := (right * input.x + forward * input.y)

	if direction.length_squared() > 0.0:
		direction = direction.normalized()
		var speed := MOVE_SPEED
		if Input.is_key_pressed(KEY_SHIFT):
			speed *= RUN_MULTIPLIER
		velocity = direction * speed
		global_position += velocity * delta
		rotation.y = atan2(direction.x, direction.z)
	else:
		velocity = Vector3.ZERO


func _movement_input() -> Vector2:
	var input := Vector2.ZERO
	if Input.is_key_pressed(KEY_A) or Input.is_key_pressed(KEY_LEFT):
		input.x -= 1.0
	if Input.is_key_pressed(KEY_D) or Input.is_key_pressed(KEY_RIGHT):
		input.x += 1.0
	if Input.is_key_pressed(KEY_W) or Input.is_key_pressed(KEY_UP):
		input.y += 1.0
	if Input.is_key_pressed(KEY_S) or Input.is_key_pressed(KEY_DOWN):
		input.y -= 1.0
	return input
