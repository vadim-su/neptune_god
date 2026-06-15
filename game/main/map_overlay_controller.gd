extends Node
class_name MapOverlayController

const MapOverlayScript := preload("res://game/ui/map_overlay.gd")
const MinimapScene := preload("res://game/ui/minimap.tscn")

signal view_changed

const MAP_SNAPSHOT_UPDATE_INTERVAL_SEC := 0.20
const FULLSCREEN_MAP_CHUNK_SNAPSHOT_MARGIN := 1

var environment: Node
var sim: NeptuneSim
var player: Node3D
var hotbar: Control
var minimap: Control
var map_overlay: Control
var map_snapshot_dirty := true
var map_snapshot_view_dirty_only := false
var map_snapshot_update_cooldown := 0.0
var buildings_dirty := true
var cached_buildings: Array = []
var cached_buildings_key := ""
var fullscreen_snapshot_chunk_rect_valid := false
var fullscreen_snapshot_chunk_rect := Rect2i()
var fullscreen_snapshot_chunk_grid_size := Vector2i.ZERO


func _ready() -> void:
	_ensure_overlays()


func setup(
	hud: Node,
	environment_node: Node,
	simulation: NeptuneSim,
	player_node: Node3D,
	hotbar_control: Control
) -> void:
	environment = environment_node
	sim = simulation
	player = player_node
	hotbar = hotbar_control
	_ensure_overlays(hud)


func process(delta: float) -> void:
	if map_snapshot_dirty:
		if map_snapshot_view_dirty_only:
			update_player_position()
		map_snapshot_update_cooldown -= delta
		if map_snapshot_update_cooldown <= 0.0:
			update_snapshots()
	else:
		update_player_position()


func toggle_fullscreen() -> void:
	if map_overlay == null:
		return
	if map_overlay.is_fullscreen_open():
		map_overlay.set_fullscreen_open(false)
	else:
		map_overlay.center_on_player()
		map_overlay.set_fullscreen_open(true)
	update_snapshots(true)


func close_fullscreen() -> void:
	if map_overlay == null:
		return
	map_overlay.set_fullscreen_open(false)
	update_snapshots(true)


func is_fullscreen_open() -> bool:
	return map_overlay != null and map_overlay.is_fullscreen_open()


func should_render_3d_world() -> bool:
	if map_overlay == null or not map_overlay.is_fullscreen_open():
		return true
	if not map_overlay.has_method("detailed_world_visible"):
		return true
	return map_overlay.detailed_world_visible()


func detailed_world_blend() -> float:
	if map_overlay == null:
		return 0.0
	return map_overlay.detailed_world_blend()


func detailed_world_visible() -> bool:
	return map_overlay != null and map_overlay.detailed_world_visible()


func pixels_per_tile() -> float:
	if map_overlay == null:
		return 1.0
	return map_overlay.pixels_per_tile()


func mark_dirty() -> void:
	map_snapshot_dirty = true
	map_snapshot_view_dirty_only = false
	map_snapshot_update_cooldown = 0.0


func mark_view_dirty() -> void:
	update_player_position()


func mark_buildings_dirty() -> void:
	buildings_dirty = true
	mark_dirty()


func update_snapshots(force_snapshot := false) -> void:
	if environment == null or minimap == null or map_overlay == null or sim == null or player == null:
		return
	minimap.visible = not map_overlay.is_fullscreen_open()
	if not force_snapshot and not map_snapshot_dirty:
		update_player_position()
		return
	if (
		not force_snapshot
		and map_snapshot_view_dirty_only
		and _fullscreen_view_still_inside_snapshotted_chunks()
	):
		update_player_position()
		map_snapshot_dirty = false
		map_snapshot_view_dirty_only = false
		map_snapshot_update_cooldown = MAP_SNAPSHOT_UPDATE_INTERVAL_SEC
		return

	var buildings: Array = _building_snapshot()
	var fullscreen_open: bool = map_overlay.is_fullscreen_open()
	var current_visible_rect: Rect2i = map_overlay.current_tile_bounds() if fullscreen_open else Rect2i()
	var visible_snapshot := {
		"chunks": [],
		"rect": current_visible_rect,
	}
	var map_snapshot := visible_snapshot
	if map_overlay.is_fullscreen_open():
		var snapshot_bounds := _fullscreen_snapshot_tile_bounds_for(current_visible_rect)
		map_snapshot = environment.explored_chunk_snapshot_for_rect(snapshot_bounds)
		_remember_fullscreen_snapshot_chunk_rect(snapshot_bounds)
	else:
		visible_snapshot = environment.visible_chunk_snapshot()
		map_snapshot = {
			"chunks": [],
			"rect": visible_snapshot["rect"],
		}
		fullscreen_snapshot_chunk_rect_valid = false
	apply_chunk_snapshots(
		map_snapshot,
		visible_snapshot,
		buildings,
		player.global_position,
		not fullscreen_open,
		_building_snapshot_key()
	)
	map_snapshot_dirty = false
	map_snapshot_view_dirty_only = false
	map_snapshot_update_cooldown = MAP_SNAPSHOT_UPDATE_INTERVAL_SEC


func _building_snapshot() -> Array:
	if buildings_dirty:
		cached_buildings = sim.buildings()
		cached_buildings_key = _buildings_key(cached_buildings)
		buildings_dirty = false
	return cached_buildings


func _building_snapshot_key() -> String:
	if buildings_dirty:
		_building_snapshot()
	return cached_buildings_key


func _fullscreen_view_still_inside_snapshotted_chunks() -> bool:
	if map_overlay == null or not map_overlay.is_fullscreen_open():
		return false
	if not fullscreen_snapshot_chunk_rect_valid:
		return false
	var current_chunk_rect := _fullscreen_snapshot_chunk_rect_for(map_overlay.current_tile_bounds())
	return (
		current_chunk_rect.size.x > 0
		and current_chunk_rect.size.y > 0
		and _chunk_rect_contains(fullscreen_snapshot_chunk_rect, current_chunk_rect)
	)


func _remember_fullscreen_snapshot_chunk_rect(tile_bounds: Rect2i) -> void:
	fullscreen_snapshot_chunk_grid_size = _environment_chunk_snapshot_grid_size()
	fullscreen_snapshot_chunk_rect = _fullscreen_snapshot_chunk_rect_for(tile_bounds)
	fullscreen_snapshot_chunk_rect_valid = fullscreen_snapshot_chunk_rect.size.x > 0 and fullscreen_snapshot_chunk_rect.size.y > 0


func _fullscreen_snapshot_chunk_rect_for(tile_bounds: Rect2i) -> Rect2i:
	if tile_bounds.size.x <= 0 or tile_bounds.size.y <= 0:
		return Rect2i()
	var grid_size := fullscreen_snapshot_chunk_grid_size
	if grid_size.x <= 0 or grid_size.y <= 0:
		grid_size = _environment_chunk_snapshot_grid_size()
	if grid_size.x <= 0 or grid_size.y <= 0:
		return Rect2i()
	var min_chunk := _tile_to_snapshot_chunk(tile_bounds.position, grid_size)
	var max_chunk := _tile_to_snapshot_chunk(tile_bounds.position + tile_bounds.size - Vector2i.ONE, grid_size)
	return Rect2i(min_chunk, max_chunk - min_chunk + Vector2i.ONE)


func _fullscreen_snapshot_tile_bounds_for(tile_bounds: Rect2i) -> Rect2i:
	if tile_bounds.size.x <= 0 or tile_bounds.size.y <= 0:
		return tile_bounds
	var grid_size := _environment_chunk_snapshot_grid_size()
	if grid_size.x <= 0 or grid_size.y <= 0:
		return tile_bounds
	var chunk_rect := _fullscreen_snapshot_chunk_rect_for(tile_bounds)
	if chunk_rect.size.x <= 0 or chunk_rect.size.y <= 0:
		return tile_bounds
	var margin := Vector2i(FULLSCREEN_MAP_CHUNK_SNAPSHOT_MARGIN, FULLSCREEN_MAP_CHUNK_SNAPSHOT_MARGIN)
	var buffered_chunk_rect := Rect2i(
		chunk_rect.position - margin,
		chunk_rect.size + margin * 2
	)
	return Rect2i(
		Vector2i(buffered_chunk_rect.position.x * grid_size.x, buffered_chunk_rect.position.y * grid_size.y),
		Vector2i(buffered_chunk_rect.size.x * grid_size.x, buffered_chunk_rect.size.y * grid_size.y)
	)


func _chunk_rect_contains(outer: Rect2i, inner: Rect2i) -> bool:
	var outer_end := outer.position + outer.size
	var inner_end := inner.position + inner.size
	return (
		inner.position.x >= outer.position.x
		and inner.position.y >= outer.position.y
		and inner_end.x <= outer_end.x
		and inner_end.y <= outer_end.y
	)


func _environment_chunk_snapshot_grid_size() -> Vector2i:
	if environment != null and environment.has_method("chunk_snapshot_grid_size"):
		return environment.chunk_snapshot_grid_size()
	return Vector2i.ZERO


func _tile_to_snapshot_chunk(tile: Vector2i, grid_size: Vector2i) -> Vector2i:
	return Vector2i(
		int(floor(float(tile.x) / float(maxi(grid_size.x, 1)))),
		int(floor(float(tile.y) / float(maxi(grid_size.y, 1))))
	)


func apply_chunk_snapshots(
	map_snapshot: Dictionary,
	visible_snapshot: Dictionary,
	buildings: Array,
	player_position: Vector3,
	update_minimap := true,
	buildings_key := ""
) -> void:
	var current_visible_rect: Rect2i = visible_snapshot["rect"]
	var effective_buildings_key := buildings_key
	if effective_buildings_key.is_empty():
		effective_buildings_key = _buildings_key(buildings)
	var visible_chunk_key := str(visible_snapshot.get("key", ""))
	var map_chunk_key := str(map_snapshot.get("key", ""))
	if update_minimap:
		minimap.set_chunk_snapshot(
			visible_snapshot["chunks"],
			current_visible_rect,
			buildings,
			player_position,
			current_visible_rect,
			true,
			effective_buildings_key,
			visible_chunk_key
		)
	if map_overlay.is_fullscreen_open():
		map_overlay.set_chunk_snapshot(
			map_snapshot["chunks"],
			map_snapshot["rect"],
			buildings,
			player_position,
			current_visible_rect,
			false,
			effective_buildings_key,
			map_chunk_key
		)
	else:
		map_overlay.set_player_position(player_position)


func update_player_position() -> void:
	if minimap == null or map_overlay == null or player == null:
		return
	minimap.visible = not map_overlay.is_fullscreen_open()
	minimap.set_player_position(player.global_position)
	map_overlay.set_player_position(player.global_position)


func refresh_resource_selection() -> void:
	if map_overlay != null and map_overlay.has_method("refresh_resource_selection"):
		map_overlay.refresh_resource_selection()


func keep_hotbar_above_map_overlay() -> void:
	if hotbar == null or map_overlay == null:
		return
	hotbar.z_index = map_overlay.z_index + 10


func _buildings_key(buildings: Array) -> String:
	var hash_value := hash(buildings.size())
	for raw_building: Variant in buildings:
		var building: Dictionary = raw_building
		hash_value = hash([
			hash_value,
			int(building.get("id", 0)),
			str(building.get("def_id", "")),
			hash(building.get("footprint", [])),
		])
	return str(hash_value)


func _ensure_overlays(_hud: Node = null) -> void:
	if minimap == null:
		minimap = get_node_or_null("Minimap") as Control
	if minimap == null:
		minimap = MinimapScene.instantiate()
		minimap.name = "Minimap"
		add_child(minimap)
	minimap.configure_minimap()

	if map_overlay == null:
		map_overlay = get_node_or_null("MapOverlay") as Control
	if map_overlay == null:
		map_overlay = MapOverlayScript.new()
		map_overlay.name = "MapOverlay"
		add_child(map_overlay)
	map_overlay.configure_fullscreen()
	if not map_overlay.view_changed.is_connected(_on_map_overlay_view_changed):
		map_overlay.view_changed.connect(_on_map_overlay_view_changed)
	keep_hotbar_above_map_overlay()


func _on_map_overlay_view_changed() -> void:
	mark_view_dirty()
	view_changed.emit()
