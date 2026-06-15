extends RefCounted
class_name CameraController

var min_distance := 10.0
var max_distance := 96.0
var target_height := 0.75
var min_elevation := deg_to_rad(30.0)
var max_elevation := deg_to_rad(76.0)
var far_padding := 32.0
var max_visible_tile_radius := 64
var yaw := deg_to_rad(42.0)
var elevation := deg_to_rad(58.0)
var distance := 32.0


func rotate(mouse_delta: Vector2) -> void:
	yaw -= mouse_delta.x * 0.006
	elevation = clamp(elevation - mouse_delta.y * 0.006, min_elevation, max_elevation)


func zoom_in(step: float) -> void:
	distance = max(min_distance, distance - step)


func zoom_out(step: float) -> void:
	distance = min(max_distance, distance + step)


func apply(camera: Camera3D, player_position: Vector3, map_overlay: Control, viewport: Viewport) -> void:
	_update_3d_rendering_gate(viewport, map_overlay)
	var detailed_blend := 0.0
	if map_overlay != null:
		detailed_blend = map_overlay.detailed_world_blend()

	var target := player_position + Vector3.UP * target_height
	var horizontal_distance := distance * cos(elevation)
	var offset := Vector3(
		horizontal_distance * sin(yaw),
		distance * sin(elevation),
		horizontal_distance * cos(yaw)
	)
	var gameplay_position := target + offset
	camera.far = gameplay_far_clip()
	if detailed_blend <= 0.0:
		camera.global_position = gameplay_position
		camera.look_at(target, Vector3.UP)
		return

	var detailed_center := Vector2(player_position.x, player_position.z)
	if map_overlay != null and map_overlay.has_method("map_center"):
		detailed_center = map_overlay.map_center()
	var detailed_target := Vector3(detailed_center.x, 0.0, detailed_center.y)
	var viewport_height := viewport.get_visible_rect().size.y
	var height := detailed_map_camera_height(
		map_overlay.pixels_per_tile(),
		viewport_height,
		camera.fov
	)
	var detailed_position := detailed_target + Vector3(0.0, height, 0.02)
	camera.global_position = gameplay_position.lerp(detailed_position, detailed_blend)
	camera.look_at(target.lerp(detailed_target, detailed_blend), map_camera_up_vector(detailed_blend))


func gameplay_far_clip() -> float:
	return distance + float(max_visible_tile_radius) + far_padding


func should_render_3d_world(map_overlay: Control) -> bool:
	if map_overlay == null or not map_overlay.is_fullscreen_open():
		return true
	if not map_overlay.has_method("detailed_world_visible"):
		return true
	return map_overlay.detailed_world_visible()


static func map_camera_up_vector(detailed_blend: float) -> Vector3:
	return Vector3.UP.lerp(Vector3.BACK, detailed_blend).normalized()


static func detailed_map_camera_height(pixels_per_tile: float, viewport_height: float, fov_degrees: float) -> float:
	var visible_world_height := maxf(viewport_height, 1.0) / maxf(pixels_per_tile, 1.0)
	var half_fov := deg_to_rad(fov_degrees) * 0.5
	return visible_world_height / (2.0 * tan(half_fov))


func _update_3d_rendering_gate(viewport: Viewport, map_overlay: Control) -> void:
	if viewport == null:
		return
	viewport.disable_3d = not should_render_3d_world(map_overlay)
