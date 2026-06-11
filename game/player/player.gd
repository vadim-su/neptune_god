extends Node3D
class_name PlayerController

const MOVE_SPEED := 9.0
const RUN_MULTIPLIER := 1.55
const MAX_COLLISION_STEP := 0.2

var movement_yaw := 0.0
var velocity := Vector3.ZERO
var can_move_to: Callable


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
		_move_with_collision(velocity * delta)
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


func _move_with_collision(motion: Vector3) -> void:
	var step_count := int(ceil(motion.length() / MAX_COLLISION_STEP))
	if step_count < 1:
		step_count = 1

	var step_motion := motion / float(step_count)
	for _step in range(step_count):
		if not _move_collision_step(step_motion):
			return


func _move_collision_step(motion: Vector3) -> bool:
	var target := global_position + motion
	if _can_stand_at(target):
		global_position = target
		return true

	var moved := false
	var x_target := global_position + Vector3(motion.x, 0.0, 0.0)
	if _can_stand_at(x_target):
		global_position = x_target
		moved = true

	var z_target := global_position + Vector3(0.0, 0.0, motion.z)
	if _can_stand_at(z_target):
		global_position = z_target
		moved = true

	return moved


func _can_stand_at(position: Vector3) -> bool:
	if can_move_to.is_valid():
		return bool(can_move_to.call(position))
	return true
