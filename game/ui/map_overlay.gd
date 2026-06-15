extends Control
class_name MapOverlay

const ItemCatalogScript := preload("res://game/items/item_catalog.gd")

signal view_changed

const RESOURCE_VEIN_NEIGHBOR_DISTANCE := 2
const DETAILED_WORLD_PIXELS_PER_TILE := 18.0
const DETAILED_WORLD_TRANSITION_PIXELS := 8.0
const MIN_PIXELS_PER_TILE := 1.0
const MAX_PIXELS_PER_TILE := 48.0
const MINIMAP_SIZE := Vector2(160.0, 160.0)
const MINIMAP_MARGIN := 16.0
const MINIMAP_VIEW_RADIUS_TILES := 64
const MAX_CHUNK_SNAPSHOT_SYNCS_PER_FRAME := 4
const MAX_MAP_TEXTURE_JOBS_STARTED_PER_FRAME := 1
const MAX_ACTIVE_MAP_TEXTURE_TASKS := 1
const MAX_CHUNK_TEXTURE_UPLOADS_PER_FRAME := 1
const MAX_RETAINED_MAP_CHUNK_TEXTURES := 128
const FULLSCREEN_CHART_CACHE_PADDING_TILES := 96
const MAX_FULLSCREEN_CHART_CACHE_PIXELS := 12000000
const PLAYER_MARKER_RADIUS := 6.0
const MAP_TEXTURE_BACKGROUND := Color(0.0, 0.0, 0.0, 0.0)
const MAP_BACKGROUND_COLOR := Color(0.028, 0.038, 0.034, 1.0)
const FOGGED_TILE_BLEND := 0.58

const TERRAIN_COLORS := {
	"ground": Color(0.16, 0.23, 0.17, 1.0),
	"stone": Color(0.34, 0.35, 0.32, 1.0),
	"water": Color(0.08, 0.22, 0.34, 1.0),
}

const RESOURCE_COLORS := {
	"iron_ore": Color(0.63, 0.56, 0.46, 1.0),
	"copper_ore": Color(0.86, 0.42, 0.20, 1.0),
	"coal": Color(0.04, 0.04, 0.04, 1.0),
}

const RESOURCE_LABELS := {
	"iron_ore": "Iron ore",
	"copper_ore": "Copper ore",
	"coal": "Coal",
}

const BUILDING_COLORS := {
	"basic_mining_drill": Color(0.42, 0.58, 0.82, 1.0),
	"stone_furnace": Color(0.70, 0.64, 0.54, 1.0),
	"basic_belt": Color(0.88, 0.75, 0.28, 1.0),
	"accelerated_belt": Color(0.45, 0.80, 0.95, 1.0),
	"fast_belt": Color(0.35, 0.60, 1.0, 1.0),
	"inserter": Color(0.84, 0.46, 0.36, 1.0),
	"wooden_chest": Color(0.56, 0.34, 0.18, 1.0),
}


class MapChunkTextureTask:
	extends RefCounted

	var chunk_key := ""
	var bounds := Rect2i()
	var tiles: Array = []
	var buildings: Array = []
	var terrain_colors := {}
	var building_colors := {}
	var resource_colors := {}
	var background := Color.TRANSPARENT
	var mirror_x := false
	var mirror_y := false
	var snapshot_key := ""
	var epoch := 0
	var width := 1
	var height := 1
	var data := PackedByteArray()

	func _init(
		next_chunk_key: String,
		next_bounds: Rect2i,
		next_tiles: Array,
		next_buildings: Array,
		next_terrain_colors: Dictionary,
		next_building_colors: Dictionary,
		next_resource_colors: Dictionary,
		next_background: Color,
		next_mirror_x: bool,
		next_mirror_y: bool,
		next_snapshot_key: String,
		next_epoch: int
	) -> void:
		chunk_key = next_chunk_key
		bounds = next_bounds
		tiles = next_tiles
		buildings = next_buildings
		terrain_colors = next_terrain_colors
		building_colors = next_building_colors
		resource_colors = next_resource_colors
		background = next_background
		mirror_x = next_mirror_x
		mirror_y = next_mirror_y
		snapshot_key = next_snapshot_key
		epoch = next_epoch
		width = maxi(bounds.size.x, 1)
		height = maxi(bounds.size.y, 1)

	func run() -> void:
		data = PackedByteArray()
		data.resize(width * height * 4)
		if background != Color.TRANSPARENT:
			for y in range(height):
				for x in range(width):
					_write_rgba8(data, width, x, y, background)

		for raw_tile: Variant in tiles:
			var tile: Dictionary = raw_tile
			if not bool(tile.get("render", true)):
				continue
			var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
			if not bounds.has_point(pos):
				continue
			var color: Color = terrain_colors.get(str(tile.get("terrain", "ground")), terrain_colors["ground"])
			var resource_id := str(tile.get("resource", ""))
			if not resource_id.is_empty() and int(tile.get("amount", 0)) > 0:
				color = Color(resource_colors.get(resource_id, color)).lerp(Color.WHITE, 0.08)
			_write_rgba8(
				data,
				width,
				_tile_image_x_for(pos, bounds, mirror_x),
				_tile_image_y_for(pos, bounds, mirror_y),
				color
			)

		for raw_building: Variant in buildings:
			var building: Dictionary = raw_building
			var def_id := str(building.get("def_id", ""))
			var building_color: Color = building_colors.get(def_id, _color_from_id(def_id))
			for raw_tile: Variant in building.get("footprint", []):
				var tile: Dictionary = raw_tile
				var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
				if bounds.has_point(pos):
					_write_rgba8(
						data,
						width,
						_tile_image_x_for(pos, bounds, mirror_x),
						_tile_image_y_for(pos, bounds, mirror_y),
						building_color
					)

	func result() -> Dictionary:
		return {
			"epoch": epoch,
			"chunk_key": chunk_key,
			"bounds": bounds,
			"snapshot_key": snapshot_key,
			"width": width,
			"height": height,
			"data": data,
		}

	static func _tile_image_x_for(tile: Vector2i, image_bounds: Rect2i, should_mirror_x: bool) -> int:
		if should_mirror_x:
			return image_bounds.position.x + image_bounds.size.x - 1 - tile.x
		return tile.x - image_bounds.position.x

	static func _tile_image_y_for(tile: Vector2i, image_bounds: Rect2i, should_mirror_y: bool) -> int:
		if should_mirror_y:
			return image_bounds.position.y + image_bounds.size.y - 1 - tile.y
		return tile.y - image_bounds.position.y

	static func _write_rgba8(target_data: PackedByteArray, target_width: int, x: int, y: int, color: Color) -> void:
		var offset := (y * target_width + x) * 4
		target_data[offset] = clampi(int(round(color.r * 255.0)), 0, 255)
		target_data[offset + 1] = clampi(int(round(color.g * 255.0)), 0, 255)
		target_data[offset + 2] = clampi(int(round(color.b * 255.0)), 0, 255)
		target_data[offset + 3] = clampi(int(round(color.a * 255.0)), 0, 255)

	static func _color_from_id(id: String) -> Color:
		var hash_value := id.hash()
		var r := 0.30 + float(hash_value & 0x3f) / 160.0
		var g := 0.30 + float((hash_value >> 8) & 0x3f) / 160.0
		var b := 0.30 + float((hash_value >> 16) & 0x3f) / 160.0
		return Color(r, g, b, 1.0)


var _is_minimap := false
var _fullscreen_open := false
var _tiles: Array = []
var _buildings: Array = []
var _visible_rect := Rect2i(Vector2i.ZERO, Vector2i.ONE)
var _current_visible_rect := Rect2i(Vector2i.ZERO, Vector2i.ONE)
var _player_position := Vector3.ZERO
var _pixels_per_tile := 4.0
var _map_center := Vector2.ZERO
var _center_initialized := false
var _follow_player_in_fullscreen := true
var _chunk_sync_key := ""
var _chunk_sync_key_compute_count := 0
var _buildings_snapshot_key := ""
var _buildings_key_compute_count := 0
var _building_grid_chunk_cache_key := ""
var _building_grid_chunk_cache := {}
var _building_grid_chunk_building_key_cache := {}
var _building_grid_chunk_cache_rebuild_count := 0
var _resource_selection_revision := 0
var _resource_hover_suspended := false
var _map_chunk_entries := {}
var _target_map_chunk_keys := {}
var _pending_map_chunk_syncs: Array[Dictionary] = []
var _pending_map_chunk_sync_read_index := 0
var _pending_map_chunk_sync_lookup := {}
var _pending_map_texture_jobs := {}
var _loading_map_texture_tasks := {}
var _ready_map_texture_results: Array[Dictionary] = []
var _ready_map_texture_result_read_index := 0
var _map_texture_mutex := Mutex.new()
var _map_texture_epoch := 0
var _map_chunk_texture_access_sequence := 0
var _map_chunk_entries_revision := 0
var _redraw_request_count := 0
var _map_chunk_keys_by_grid_pos := {}
var _map_chunk_grid_cell_size := Vector2i.ZERO
var _map_chunk_grid_complete := false
var _visible_map_chunk_keys_cache: Array[String] = []
var _visible_map_chunk_cache_bounds := Rect2i()
var _visible_map_chunk_cache_revision := -1
var _query_tiles_cache: Array = []
var _query_tiles_cache_rect := Rect2i()
var _query_tiles_cache_revision := -1
var _query_tiles_cache_rebuild_count := 0
var _visible_resource_lookup_cache := {}
var _visible_resource_lookup_cache_rect := Rect2i()
var _visible_resource_lookup_cache_revision := -1
var _visible_resource_lookup_cache_rebuild_count := 0
var _hovered_resource_vein_cache_key := ""
var _hovered_resource_vein_cache: Dictionary = {}
var _fullscreen_chart_cache_texture: ImageTexture = null
var _fullscreen_chart_cache_bounds := Rect2i()
var _fullscreen_chart_cache_revision := -1
var _fullscreen_chart_cache_current_visible_rect := Rect2i()
var _fullscreen_chart_cache_background_alpha := -1.0
var _fullscreen_chart_cache_build_count := 0


static func collect_resource_vein(start: Vector2i, tiles: Array, visible_rect: Rect2i) -> Dictionary:
	var lookup := _visible_resource_lookup(tiles, visible_rect)
	return collect_resource_vein_from_lookup(start, lookup)


static func collect_resource_vein_from_lookup(start: Vector2i, lookup: Dictionary) -> Dictionary:
	if not lookup.has(start):
		return {}

	var start_deposit: Dictionary = lookup[start]
	var resource_id := str(start_deposit["resource"])
	var queue: Array[Vector2i] = [start]
	var queue_read_index := 0
	var visited := {start: true}
	var vein_tiles: Array[Vector2i] = []
	var amount := 0

	while queue_read_index < queue.size():
		var tile := queue[queue_read_index] as Vector2i
		queue_read_index += 1
		if not lookup.has(tile):
			continue
		var deposit: Dictionary = lookup[tile]
		if str(deposit["resource"]) != resource_id:
			continue

		vein_tiles.append(tile)
		amount += int(deposit["amount"])

		for neighbor: Vector2i in _resource_vein_neighbors(tile):
			if visited.has(neighbor):
				continue
			visited[neighbor] = true
			queue.append(neighbor)

	vein_tiles.sort_custom(func(left: Vector2i, right: Vector2i) -> bool:
		if left.y == right.y:
			return left.x < right.x
		return left.y < right.y
	)
	return {
		"resource": resource_id,
		"amount": amount,
		"tiles": vein_tiles,
	}


static func _visible_resource_lookup(tiles: Array, visible_rect: Rect2i) -> Dictionary:
	var lookup := {}
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		if not bool(tile.get("render", true)):
			continue
		var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
		if not visible_rect.has_point(pos):
			continue
		var resource_id := str(tile.get("resource", ""))
		var amount := int(tile.get("amount", 0))
		if resource_id.is_empty() or amount <= 0:
			continue
		lookup[pos] = {
			"resource": resource_id,
			"amount": amount,
		}
	return lookup


static func _resource_vein_neighbors(tile: Vector2i) -> Array[Vector2i]:
	var neighbors: Array[Vector2i] = []
	for offset_y in range(-RESOURCE_VEIN_NEIGHBOR_DISTANCE, RESOURCE_VEIN_NEIGHBOR_DISTANCE + 1):
		for offset_x in range(-RESOURCE_VEIN_NEIGHBOR_DISTANCE, RESOURCE_VEIN_NEIGHBOR_DISTANCE + 1):
			if offset_x == 0 and offset_y == 0:
				continue
			neighbors.append(tile + Vector2i(offset_x, offset_y))
	return neighbors


func configure_minimap() -> void:
	_is_minimap = true
	_fullscreen_open = false
	visible = true
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	z_index = 20
	anchor_left = 1.0
	anchor_top = 1.0
	anchor_right = 1.0
	anchor_bottom = 1.0
	offset_left = -MINIMAP_SIZE.x - MINIMAP_MARGIN
	offset_top = -MINIMAP_SIZE.y - MINIMAP_MARGIN
	offset_right = -MINIMAP_MARGIN
	offset_bottom = -MINIMAP_MARGIN


func configure_fullscreen() -> void:
	_is_minimap = false
	_fullscreen_open = false
	visible = false
	mouse_filter = Control.MOUSE_FILTER_STOP
	texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	z_index = 40
	set_anchors_preset(Control.PRESET_FULL_RECT)
	offset_left = 0.0
	offset_top = 0.0
	offset_right = 0.0
	offset_bottom = 0.0


func set_fullscreen_open(open: bool) -> void:
	if _is_minimap:
		return
	if _fullscreen_open == open and visible == open:
		return
	_fullscreen_open = open
	visible = open
	_resource_hover_suspended = false
	if open and not _center_initialized:
		center_on_player()
	_request_redraw()


func is_fullscreen_open() -> bool:
	return not _is_minimap and _fullscreen_open


func set_chunk_snapshot(
	chunks: Array,
	visible_rect: Rect2i,
	buildings: Array,
	player_position: Vector3,
	current_visible_rect: Rect2i = Rect2i(),
	cache_tiles_for_queries := true,
	buildings_key := "",
	chunk_snapshot_key := ""
) -> void:
	var effective_current_visible_rect := current_visible_rect
	if effective_current_visible_rect.size.x <= 0 or effective_current_visible_rect.size.y <= 0:
		effective_current_visible_rect = visible_rect
	var next_buildings_key := buildings_key
	if next_buildings_key.is_empty():
		next_buildings_key = _buildings_key(buildings)
	var next_chunk_sync_key := ""
	if chunk_snapshot_key.is_empty():
		next_chunk_sync_key = _chunk_sync_key_for(chunks, next_buildings_key)
	else:
		next_chunk_sync_key = str(hash([chunk_snapshot_key, next_buildings_key]))
	var chunks_changed := next_chunk_sync_key != _chunk_sync_key
	var previous_visible_rect := _visible_rect
	var previous_current_visible_rect := _current_visible_rect
	_buildings = buildings.duplicate(false)
	_buildings_snapshot_key = next_buildings_key
	if cache_tiles_for_queries and chunks_changed:
		_tiles = _flatten_chunk_tiles(chunks)
	if visible_rect.size.x > 0 and visible_rect.size.y > 0:
		_visible_rect = visible_rect
	_current_visible_rect = effective_current_visible_rect
	_chunk_sync_key = next_chunk_sync_key
	if chunks_changed:
		var priority_rect := effective_current_visible_rect
		if is_fullscreen_open():
			priority_rect = _current_bounds()
		_sync_map_chunk_entries(
			_prioritized_map_chunks(chunks, priority_rect, effective_current_visible_rect),
			_buildings
		)
	set_player_position(player_position)
	if chunks_changed or _visible_rect != previous_visible_rect or _current_visible_rect != previous_current_visible_rect:
		_request_redraw()


func set_player_position(position: Vector3) -> void:
	if _center_initialized and _player_position == position:
		return
	var previous_tile := _player_marker_tile()
	var previous_bounds := _current_bounds()
	_player_position = position
	if _is_minimap:
		_map_center = Vector2(_player_position.x, _player_position.z)
		_center_initialized = true
		if _current_bounds() != previous_bounds:
			view_changed.emit()
		if visible:
			_request_redraw()
		return
	if not _center_initialized:
		center_on_player()
		return
	elif is_fullscreen_open() and _follow_player_in_fullscreen:
		_map_center = Vector2(_player_position.x, _player_position.z)
		if _current_bounds() != previous_bounds:
			view_changed.emit()
	if visible and _player_marker_tile() != previous_tile:
		_request_redraw()


func center_on_player() -> void:
	var previous_bounds := _current_bounds()
	_map_center = Vector2(_player_position.x, _player_position.z)
	_center_initialized = true
	_follow_player_in_fullscreen = true
	if _current_bounds() != previous_bounds:
		view_changed.emit()
	_request_redraw()


func player_marker_snapshot() -> Dictionary:
	return {
		"tile": _player_marker_tile(),
		"label": "player",
	}


func _player_marker_tile() -> Vector2i:
	return Vector2i(int(round(_player_position.x)), int(round(_player_position.z)))


func map_center_for_tests() -> Vector2:
	return _map_center


func map_center() -> Vector2:
	return _map_center


func current_tile_bounds() -> Rect2i:
	return _current_bounds()


func set_pixels_per_tile(value: float) -> void:
	var next_pixels_per_tile: float = clampf(value, MIN_PIXELS_PER_TILE, MAX_PIXELS_PER_TILE)
	if is_equal_approx(_pixels_per_tile, next_pixels_per_tile):
		return
	_pixels_per_tile = next_pixels_per_tile
	if is_fullscreen_open():
		_resource_hover_suspended = true
	view_changed.emit()
	_request_redraw()


func pixels_per_tile() -> float:
	return _pixels_per_tile


func zoom_by(factor: float) -> void:
	set_pixels_per_tile(_pixels_per_tile * factor)


func drag_by(screen_delta: Vector2) -> void:
	if not is_fullscreen_open() or screen_delta == Vector2.ZERO:
		return
	var center_delta := Vector2(
		screen_delta.x / maxf(_pixels_per_tile, 1.0),
		screen_delta.y / maxf(_pixels_per_tile, 1.0)
	)
	if not _mirror_chart_x():
		center_delta.x = -center_delta.x
	if not _mirror_chart_y():
		center_delta.y = -center_delta.y
	_map_center += center_delta
	_follow_player_in_fullscreen = false
	_resource_hover_suspended = true
	view_changed.emit()
	_request_redraw()


func detailed_world_blend() -> float:
	if not is_fullscreen_open():
		return 0.0
	var half_width := DETAILED_WORLD_TRANSITION_PIXELS * 0.5
	return smoothstep(
		DETAILED_WORLD_PIXELS_PER_TILE - half_width,
		DETAILED_WORLD_PIXELS_PER_TILE + half_width,
		_pixels_per_tile
	)


func detailed_world_visible() -> bool:
	return detailed_world_blend() >= 1.0


func should_draw_chart_layer() -> bool:
	return chart_layer_alpha() > 0.0


func map_background_alpha() -> float:
	return 1.0 if visible else 0.0


func chart_layer_alpha() -> float:
	if not visible:
		return 0.0
	return 1.0


func should_draw_map_markers() -> bool:
	return visible and not detailed_world_visible()


func refresh_resource_selection() -> void:
	_resource_hover_suspended = false
	_resource_selection_revision += 1
	if visible:
		_request_redraw()


func resource_selection_revision_for_tests() -> int:
	return _resource_selection_revision


func tile_uses_detailed_world_for_tests(tile: Vector2i) -> bool:
	return _tile_uses_detailed_world(tile)


func tile_region_local_rect_for_tests(tile_bounds: Rect2i, target_bounds: Rect2i, tile_scale: float) -> Rect2:
	return _tile_region_local_rect(tile_bounds, target_bounds, tile_scale)


func background_regions_for_tests(bounds: Rect2i) -> Array[Rect2i]:
	return _map_background_regions(bounds)


func resource_color_for_tests(resource_id: String, terrain_color: Color) -> Color:
	return _resource_color(resource_id, terrain_color)


func pending_map_texture_job_count_for_tests() -> int:
	return (
		_pending_map_chunk_sync_count()
		+ _pending_map_texture_jobs.size()
		+ _loading_map_texture_tasks.size()
	)


func uploaded_map_chunk_texture_count_for_tests() -> int:
	var count := 0
	for chunk_key: String in _map_chunk_entries.keys():
		var entry: Dictionary = _map_chunk_entries[chunk_key]
		if entry.has("texture"):
			count += 1
	return count


func visible_map_chunk_keys_for_tests(bounds: Rect2i) -> Array[String]:
	return _visible_map_chunk_keys_for_bounds(bounds)


func uploaded_map_chunk_image_for_tests(chunk_key: String) -> Image:
	if not _map_chunk_entries.has(chunk_key):
		return null
	var entry: Dictionary = _map_chunk_entries[chunk_key]
	if not entry.has("texture"):
		return null
	var texture: Texture2D = entry["texture"]
	return texture.get_image()


func ensure_fullscreen_chart_cache_for_tests(bounds: Rect2i) -> bool:
	return _ensure_fullscreen_chart_cache(bounds, map_background_alpha())


func fullscreen_chart_cache_texture_for_tests() -> Texture2D:
	return _fullscreen_chart_cache_texture


func fullscreen_chart_cache_build_count_for_tests() -> int:
	return _fullscreen_chart_cache_build_count


func hovered_resource_vein_for_tests(hovered: Vector2i) -> Dictionary:
	return _resource_vein_for_hovered_tile(hovered)


func player_marker_radius_for_tests(_tile_scale: float) -> float:
	return PLAYER_MARKER_RADIUS


func resource_hover_suspended_for_tests() -> bool:
	return _resource_hover_suspended


func buildings_key_compute_count_for_tests() -> int:
	return _buildings_key_compute_count


func chunk_sync_key_compute_count_for_tests() -> int:
	return _chunk_sync_key_compute_count


func redraw_request_count_for_tests() -> int:
	return _redraw_request_count


func building_grid_chunk_cache_rebuild_count_for_tests() -> int:
	return _building_grid_chunk_cache_rebuild_count


func query_tiles_cache_rebuild_count_for_tests() -> int:
	return _query_tiles_cache_rebuild_count


func visible_resource_lookup_cache_rebuild_count_for_tests() -> int:
	return _visible_resource_lookup_cache_rebuild_count


func process_map_texture_jobs_for_tests() -> void:
	_process_pending_map_chunk_syncs(MAX_CHUNK_SNAPSHOT_SYNCS_PER_FRAME)
	_collect_completed_map_texture_tasks()
	_apply_ready_map_textures(MAX_CHUNK_TEXTURE_UPLOADS_PER_FRAME)


func _process(_delta: float) -> void:
	_process_pending_map_chunk_syncs(MAX_CHUNK_SNAPSHOT_SYNCS_PER_FRAME)
	_collect_completed_map_texture_tasks()
	_apply_ready_map_textures(MAX_CHUNK_TEXTURE_UPLOADS_PER_FRAME)


func _exit_tree() -> void:
	_map_texture_epoch += 1
	for task_id: int in _loading_map_texture_tasks.keys():
		WorkerThreadPool.wait_for_task_completion(task_id)
	_loading_map_texture_tasks.clear()
	_pending_map_texture_jobs.clear()
	_pending_map_chunk_syncs.clear()
	_pending_map_chunk_sync_read_index = 0
	_pending_map_chunk_sync_lookup.clear()
	_ready_map_texture_results.clear()
	_ready_map_texture_result_read_index = 0


func _request_redraw() -> void:
	_redraw_request_count += 1
	queue_redraw()


func _draw() -> void:
	if not visible:
		return

	var bounds := _current_bounds()
	var tile_scale := _tile_scale(bounds)
	var chart_alpha := chart_layer_alpha()
	var background_alpha := map_background_alpha()
	var drew_cached_chart := false
	if chart_alpha > 0.0:
		drew_cached_chart = _draw_fullscreen_chart_cache(bounds, tile_scale, chart_alpha, background_alpha)
	if not drew_cached_chart and background_alpha > 0.0:
		_draw_map_background(bounds, tile_scale, background_alpha)
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.45, 0.58, 0.48, 0.80), false, 1.0)

	if chart_alpha > 0.0 and not drew_cached_chart:
		_draw_schematic_texture(bounds, tile_scale, chart_alpha)
	if should_draw_map_markers():
		_draw_player_marker(bounds, tile_scale)
	if is_fullscreen_open() and should_draw_map_markers():
		_draw_hovered_resource_vein(bounds, tile_scale)


func _current_bounds() -> Rect2i:
	if _is_minimap:
		var minimap_center := Vector2i(int(round(_map_center.x)), int(round(_map_center.y)))
		var minimap_size := Vector2i(MINIMAP_VIEW_RADIUS_TILES * 2, MINIMAP_VIEW_RADIUS_TILES * 2)
		return Rect2i(
			minimap_center - Vector2i(MINIMAP_VIEW_RADIUS_TILES, MINIMAP_VIEW_RADIUS_TILES),
			minimap_size
		)

	var width_tiles := maxi(1, int(ceil(size.x / _pixels_per_tile)))
	var height_tiles := maxi(1, int(ceil(size.y / _pixels_per_tile)))
	var center := Vector2i(int(round(_map_center.x)), int(round(_map_center.y)))
	return Rect2i(
		center - Vector2i(width_tiles / 2, height_tiles / 2),
		Vector2i(width_tiles, height_tiles)
	)


func _tile_scale(bounds: Rect2i) -> float:
	if not _is_minimap:
		return _pixels_per_tile
	return minf(size.x / float(maxi(bounds.size.x, 1)), size.y / float(maxi(bounds.size.y, 1)))


func _draw_map_background(bounds: Rect2i, tile_scale: float, alpha: float) -> void:
	var color := MAP_BACKGROUND_COLOR
	color.a *= alpha
	for region: Rect2i in _map_background_regions(bounds):
		draw_rect(
			_tile_region_local_rect(region, bounds, tile_scale),
			color,
			true
		)


func _map_background_regions(bounds: Rect2i) -> Array[Rect2i]:
	if detailed_world_visible():
		return []
	return [bounds]


func _draw_schematic_texture(bounds: Rect2i, tile_scale: float, alpha: float) -> void:
	for chunk_key: String in _visible_map_chunk_keys_for_bounds(bounds):
		var entry: Dictionary = _map_chunk_entries[chunk_key]
		if not entry.has("texture"):
			continue
		var chunk_bounds: Rect2i = entry["bounds"]
		var visible_bounds := _rect_intersection(bounds, chunk_bounds)
		if visible_bounds.size.x <= 0 or visible_bounds.size.y <= 0:
			continue
		if detailed_world_visible():
			for region: Rect2i in _tile_regions_outside_rect(
				visible_bounds,
				_rect_intersection(visible_bounds, _current_visible_rect)
			):
				_draw_schematic_texture_region(entry["texture"], region, bounds, chunk_bounds, tile_scale, alpha)
			continue
		_draw_schematic_texture_region(entry["texture"], visible_bounds, bounds, chunk_bounds, tile_scale, alpha)


func _draw_schematic_texture_region(
	texture: Texture2D,
	tile_bounds: Rect2i,
	target_bounds: Rect2i,
	texture_bounds: Rect2i,
	tile_scale: float,
	alpha: float
) -> void:
	if tile_bounds.size.x <= 0 or tile_bounds.size.y <= 0:
		return
	var source_rect := _tile_range_image_rect(tile_bounds, texture_bounds)
	draw_texture_rect_region(
		texture,
		_tile_region_local_rect(tile_bounds, target_bounds, tile_scale),
		Rect2(source_rect),
		Color(1.0, 1.0, 1.0, alpha)
	)
	if not _is_minimap and not detailed_world_visible():
		_draw_fog_regions(tile_bounds, target_bounds, tile_scale)


func _draw_fullscreen_chart_cache(bounds: Rect2i, tile_scale: float, alpha: float, background_alpha: float) -> bool:
	if not _can_use_fullscreen_chart_cache():
		return false
	if not _ensure_fullscreen_chart_cache(bounds, background_alpha):
		return false
	var source_rect := _tile_range_image_rect(bounds, _fullscreen_chart_cache_bounds)
	draw_texture_rect_region(
		_fullscreen_chart_cache_texture,
		_tile_region_local_rect(bounds, bounds, tile_scale),
		Rect2(source_rect),
		Color(1.0, 1.0, 1.0, alpha)
	)
	return true


func _can_use_fullscreen_chart_cache() -> bool:
	return is_fullscreen_open() and not detailed_world_visible()


func _ensure_fullscreen_chart_cache(bounds: Rect2i, background_alpha: float) -> bool:
	if not _can_use_fullscreen_chart_cache():
		return false
	if (
		_fullscreen_chart_cache_texture != null
		and _fullscreen_chart_cache_revision == _map_chunk_entries_revision
		and _fullscreen_chart_cache_current_visible_rect == _current_visible_rect
		and is_equal_approx(_fullscreen_chart_cache_background_alpha, background_alpha)
		and _rect_contains_rect(_fullscreen_chart_cache_bounds, bounds)
	):
		return true

	var cache_bounds := _expanded_fullscreen_chart_cache_bounds(bounds)
	if cache_bounds.size.x <= 0 or cache_bounds.size.y <= 0:
		return false
	if cache_bounds.size.x * cache_bounds.size.y > MAX_FULLSCREEN_CHART_CACHE_PIXELS:
		return false

	var image := Image.create(cache_bounds.size.x, cache_bounds.size.y, false, Image.FORMAT_RGBA8)
	var background := MAP_BACKGROUND_COLOR
	background.a *= background_alpha
	image.fill(background)
	for chunk_key: String in _visible_map_chunk_keys_for_bounds(cache_bounds):
		var entry: Dictionary = _map_chunk_entries[chunk_key]
		if not entry.has("texture"):
			continue
		var chunk_bounds: Rect2i = entry["bounds"]
		var visible_bounds := _rect_intersection(cache_bounds, chunk_bounds)
		if visible_bounds.size.x <= 0 or visible_bounds.size.y <= 0:
			continue
		var texture: Texture2D = entry["texture"]
		var chunk_image := texture.get_image()
		var source_rect := _tile_range_image_rect(visible_bounds, chunk_bounds)
		var destination_rect := _tile_range_image_rect(visible_bounds, cache_bounds)
		image.blit_rect(chunk_image, source_rect, destination_rect.position)
	_blend_fullscreen_chart_cache_fog(image, cache_bounds)

	_fullscreen_chart_cache_texture = ImageTexture.create_from_image(image)
	_fullscreen_chart_cache_bounds = cache_bounds
	_fullscreen_chart_cache_revision = _map_chunk_entries_revision
	_fullscreen_chart_cache_current_visible_rect = _current_visible_rect
	_fullscreen_chart_cache_background_alpha = background_alpha
	_fullscreen_chart_cache_build_count += 1
	return true


func _expanded_fullscreen_chart_cache_bounds(bounds: Rect2i) -> Rect2i:
	var padding := Vector2i(FULLSCREEN_CHART_CACHE_PADDING_TILES, FULLSCREEN_CHART_CACHE_PADDING_TILES)
	return Rect2i(bounds.position - padding, bounds.size + padding * 2)


func _blend_fullscreen_chart_cache_fog(image: Image, cache_bounds: Rect2i) -> void:
	var fog_color := MAP_BACKGROUND_COLOR
	fog_color.a = 0.44
	for fog_region: Rect2i in _tile_regions_outside_rect(
		cache_bounds,
		_rect_intersection(cache_bounds, _current_visible_rect)
	):
		var image_rect := _tile_range_image_rect(fog_region, cache_bounds)
		if image_rect.size.x <= 0 or image_rect.size.y <= 0:
			continue
		var fog_image := Image.create(image_rect.size.x, image_rect.size.y, false, Image.FORMAT_RGBA8)
		fog_image.fill(fog_color)
		image.blend_rect(fog_image, Rect2i(Vector2i.ZERO, image_rect.size), image_rect.position)


func _invalidate_fullscreen_chart_cache() -> void:
	_fullscreen_chart_cache_texture = null
	_fullscreen_chart_cache_bounds = Rect2i()
	_fullscreen_chart_cache_revision = -1


func _draw_fog_regions(tile_bounds: Rect2i, target_bounds: Rect2i, tile_scale: float) -> void:
	for fog_region: Rect2i in _tile_regions_outside_rect(
		tile_bounds,
		_rect_intersection(tile_bounds, _current_visible_rect)
	):
		var fog_color := MAP_BACKGROUND_COLOR
		fog_color.a = 0.44
		draw_rect(_tile_region_local_rect(fog_region, target_bounds, tile_scale), fog_color, true)


func _sync_map_chunk_entries(chunks: Array, buildings: Array) -> void:
	if not is_inside_tree():
		return
	_pending_map_chunk_syncs.clear()
	_pending_map_chunk_sync_read_index = 0
	_pending_map_chunk_sync_lookup.clear()
	var live_keys := {}
	var next_target_keys := {}
	var buildings_by_chunk_key := _buildings_by_chunk_key(chunks, buildings, _buildings_snapshot_key)
	var next_grid_keys_by_pos := {}
	var next_grid_cell_size := Vector2i.ZERO
	var next_grid_complete := not chunks.is_empty()
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		var bounds: Rect2i = chunk.get("bounds", Rect2i())
		if bounds.size.x <= 0 or bounds.size.y <= 0:
			next_grid_complete = false
			continue
		var chunk_key := str(chunk.get("key", _chunk_key_from_bounds(bounds)))
		var chunk_buildings: Array = buildings_by_chunk_key.get(chunk_key, [])
		if chunk.has("chunk"):
			var chunk_coord: Vector2i = chunk["chunk"]
			next_grid_keys_by_pos[chunk_coord] = chunk_key
			if next_grid_cell_size == Vector2i.ZERO:
				next_grid_cell_size = bounds.size
			elif next_grid_cell_size != bounds.size:
				next_grid_complete = false
		else:
			next_grid_complete = false
		live_keys[chunk_key] = true
		next_target_keys[chunk_key] = true
		var chunk_signature := _chunk_signature(chunk)
		var chunk_building_key := _building_key_for_chunk(chunk, chunk_buildings)
		var next_snapshot_key := _map_chunk_snapshot_key(chunk_signature, chunk_building_key, bounds)
		var existing: Dictionary = _map_chunk_entries.get(chunk_key, {})
		if (
			str(existing.get("snapshot_key", "")) == next_snapshot_key
			and (existing.has("texture") or _pending_map_texture_jobs.has(chunk_key))
		):
			if existing.has("texture"):
				_touch_map_chunk_texture_entry(chunk_key, existing)
			continue
		var pending_chunk_sync := {
			"key": chunk_key,
			"bounds": bounds,
			"signature": chunk_signature,
			"tiles": chunk.get("tiles", []),
			"buildings": chunk_buildings,
			"building_key": chunk_building_key,
		}
		if chunk.has("chunk"):
			pending_chunk_sync["chunk"] = chunk["chunk"]
		_pending_map_chunk_sync_lookup[chunk_key] = true
		_pending_map_chunk_syncs.append(pending_chunk_sync)

	_map_chunk_grid_complete = next_grid_complete and next_grid_cell_size.x > 0 and next_grid_cell_size.y > 0
	_map_chunk_keys_by_grid_pos = next_grid_keys_by_pos if _map_chunk_grid_complete else {}
	_map_chunk_grid_cell_size = next_grid_cell_size if _map_chunk_grid_complete else Vector2i.ZERO
	_target_map_chunk_keys = next_target_keys
	_prune_stale_pending_map_chunk_syncs()
	_map_chunk_entries_revision += 1

	for chunk_key: String in _map_chunk_entries.keys():
		if live_keys.has(chunk_key):
			continue
		_pending_map_texture_jobs.erase(chunk_key)
		var existing_entry: Dictionary = _map_chunk_entries[chunk_key]
		if existing_entry.has("texture"):
			continue
		_map_chunk_entries.erase(chunk_key)
		_map_chunk_entries_revision += 1
	_prune_retained_map_chunk_texture_cache()


func _prune_stale_pending_map_chunk_syncs() -> void:
	var pending_syncs: Array[Dictionary] = []
	_pending_map_chunk_sync_lookup.clear()
	for index in range(_pending_map_chunk_sync_read_index, _pending_map_chunk_syncs.size()):
		var raw_chunk: Variant = _pending_map_chunk_syncs[index]
		var chunk: Dictionary = raw_chunk
		var chunk_key := str(chunk.get("key", ""))
		if not _target_map_chunk_keys.has(chunk_key):
			continue
		if _pending_map_chunk_sync_lookup.has(chunk_key):
			continue
		pending_syncs.append(chunk)
		_pending_map_chunk_sync_lookup[chunk_key] = true
	_pending_map_chunk_syncs = pending_syncs
	_pending_map_chunk_sync_read_index = 0


func _prioritized_map_chunks(chunks: Array, primary_rect: Rect2i, secondary_rect := Rect2i()) -> Array:
	var prioritized: Array = []
	var added := {}
	_append_chunks_intersecting_rect(prioritized, added, chunks, primary_rect)
	_append_chunks_intersecting_rect(prioritized, added, chunks, secondary_rect)
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		var bounds: Rect2i = chunk.get("bounds", Rect2i())
		var chunk_key := str(chunk.get("key", _chunk_key_from_bounds(bounds)))
		if added.has(chunk_key):
			continue
		prioritized.append(chunk)
		added[chunk_key] = true
	return prioritized


func _append_chunks_intersecting_rect(target: Array, added: Dictionary, chunks: Array, query_rect: Rect2i) -> void:
	if query_rect.size.x <= 0 or query_rect.size.y <= 0:
		return
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		var bounds: Rect2i = chunk.get("bounds", Rect2i())
		var chunk_key := str(chunk.get("key", _chunk_key_from_bounds(bounds)))
		if added.has(chunk_key):
			continue
		var intersection := _rect_intersection(bounds, query_rect)
		if intersection.size.x <= 0 or intersection.size.y <= 0:
			continue
		target.append(chunk)
		added[chunk_key] = true


func _process_pending_map_chunk_syncs(limit: int) -> void:
	_normalize_pending_map_chunk_sync_queue()
	var processed := 0
	var jobs_started := 0
	while processed < limit and _pending_map_chunk_sync_read_index < _pending_map_chunk_syncs.size():
		var chunk: Dictionary = _pending_map_chunk_syncs[_pending_map_chunk_sync_read_index]
		_pending_map_chunk_sync_read_index += 1
		var chunk_key := str(chunk.get("key", ""))
		_pending_map_chunk_sync_lookup.erase(chunk_key)
		if not _target_map_chunk_keys.has(chunk_key):
			processed += 1
			continue
		var bounds: Rect2i = chunk.get("bounds", Rect2i())
		if bounds.size.x <= 0 or bounds.size.y <= 0:
			processed += 1
			continue
		var tiles: Array = chunk.get("tiles", [])
		var chunk_buildings: Array = chunk.get("buildings", [])
		var chunk_building_key := str(chunk.get("building_key", ""))
		var chunk_signature := str(chunk.get("signature", _tiles_signature(tiles)))
		var next_key := _map_chunk_snapshot_key(chunk_signature, chunk_building_key, bounds)
		var existing: Dictionary = _map_chunk_entries.get(chunk_key, {})
		if existing.get("snapshot_key", "") != next_key:
			if (
				jobs_started >= MAX_MAP_TEXTURE_JOBS_STARTED_PER_FRAME
				or _loading_map_texture_tasks.size() >= MAX_ACTIVE_MAP_TEXTURE_TASKS
			):
				_requeue_map_chunk_sync(chunk)
				break
			var next_entry := {
				"bounds": bounds,
				"snapshot_key": next_key,
				"tiles": tiles,
				"buildings": chunk_buildings,
				"building_key": chunk_building_key,
			}
			if chunk.has("chunk"):
				next_entry["chunk"] = chunk["chunk"]
			_map_chunk_entries[chunk_key] = next_entry
			_map_chunk_entries_revision += 1
			if not _queue_map_texture_job(chunk_key, bounds, tiles, chunk_buildings, next_key):
				_map_chunk_entries.erase(chunk_key)
				_map_chunk_entries_revision += 1
				_requeue_map_chunk_sync(chunk)
				break
			jobs_started += 1
		processed += 1
	_compact_pending_map_chunk_sync_queue()


func _requeue_map_chunk_sync(chunk: Dictionary) -> void:
	var chunk_key := str(chunk.get("key", ""))
	if chunk_key.is_empty() or _pending_map_chunk_sync_lookup.has(chunk_key):
		return
	_pending_map_chunk_sync_lookup[chunk_key] = true
	_pending_map_chunk_sync_read_index = maxi(_pending_map_chunk_sync_read_index - 1, 0)


func _pending_map_chunk_sync_count() -> int:
	_normalize_pending_map_chunk_sync_queue()
	return maxi(_pending_map_chunk_syncs.size() - _pending_map_chunk_sync_read_index, 0)


func _normalize_pending_map_chunk_sync_queue() -> void:
	if _pending_map_chunk_syncs.is_empty() and _pending_map_chunk_sync_read_index != 0:
		_pending_map_chunk_sync_read_index = 0
	elif _pending_map_chunk_sync_read_index > _pending_map_chunk_syncs.size():
		_pending_map_chunk_sync_read_index = _pending_map_chunk_syncs.size()


func _compact_pending_map_chunk_sync_queue() -> void:
	if _pending_map_chunk_sync_read_index <= 0:
		return
	if _pending_map_chunk_sync_read_index >= _pending_map_chunk_syncs.size():
		_pending_map_chunk_syncs.clear()
		_pending_map_chunk_sync_read_index = 0
	elif _pending_map_chunk_sync_read_index >= 32:
		_pending_map_chunk_syncs = _pending_map_chunk_syncs.slice(_pending_map_chunk_sync_read_index)
		_pending_map_chunk_sync_read_index = 0


func _queue_map_texture_job(chunk_key: String, bounds: Rect2i, tiles: Array, buildings: Array, snapshot_key: String) -> bool:
	_pending_map_texture_jobs[chunk_key] = true
	var resource_colors := _resource_colors_for_tiles(tiles)
	var task := MapChunkTextureTask.new(
		chunk_key,
		bounds,
		tiles.duplicate(false),
		buildings.duplicate(false),
		TERRAIN_COLORS,
		BUILDING_COLORS,
		resource_colors,
		MAP_TEXTURE_BACKGROUND,
		_mirror_chart_x(),
		_mirror_chart_y(),
		snapshot_key,
		_map_texture_epoch
	)
	var task_id := WorkerThreadPool.add_task(
		Callable(task, "run"),
		false,
		"Prepare map chunk texture %s" % chunk_key
	)
	if task_id < 0:
		_pending_map_texture_jobs.erase(chunk_key)
		push_error("Failed to start map chunk texture task for %s" % chunk_key)
		return false
	_loading_map_texture_tasks[task_id] = task
	return true


func _collect_completed_map_texture_tasks() -> void:
	for task_id: int in _loading_map_texture_tasks.keys():
		if not WorkerThreadPool.is_task_completed(task_id):
			continue
		var task: MapChunkTextureTask = _loading_map_texture_tasks[task_id]
		WorkerThreadPool.wait_for_task_completion(task_id)
		_loading_map_texture_tasks.erase(task_id)
		_pending_map_texture_jobs.erase(task.chunk_key)
		_ready_map_texture_results.append(task.result())


func _apply_ready_map_textures(limit: int) -> void:
	var ready_results: Array[Dictionary] = []
	_map_texture_mutex.lock()
	while (
		_ready_map_texture_result_read_index < _ready_map_texture_results.size()
		and ready_results.size() < limit
	):
		ready_results.append(_ready_map_texture_results[_ready_map_texture_result_read_index])
		_ready_map_texture_result_read_index += 1
	_compact_ready_map_texture_results()
	_map_texture_mutex.unlock()

	var applied_any := false
	for result: Dictionary in ready_results:
		if int(result.get("epoch", -1)) != _map_texture_epoch:
			continue
		var chunk_key := str(result.get("chunk_key", ""))
		if not _map_chunk_entries.has(chunk_key):
			continue
		var entry: Dictionary = _map_chunk_entries[chunk_key]
		if str(entry.get("snapshot_key", "")) != str(result.get("snapshot_key", "")):
			continue
		var image := Image.create_from_data(
			int(result["width"]),
			int(result["height"]),
			false,
			Image.FORMAT_RGBA8,
			result["data"]
		)
		entry["texture"] = ImageTexture.create_from_image(image)
		entry["bounds"] = result["bounds"]
		_touch_map_chunk_texture_entry(chunk_key, entry)
		_map_chunk_entries[chunk_key] = entry
		applied_any = true
	if applied_any:
		_invalidate_fullscreen_chart_cache()
		_prune_retained_map_chunk_texture_cache()
		_request_redraw()


func _touch_map_chunk_texture_entry(chunk_key: String, entry: Dictionary) -> void:
	if not entry.has("texture"):
		return
	_map_chunk_texture_access_sequence += 1
	entry["last_texture_use"] = _map_chunk_texture_access_sequence
	_map_chunk_entries[chunk_key] = entry


func _prune_retained_map_chunk_texture_cache(max_retained := MAX_RETAINED_MAP_CHUNK_TEXTURES) -> void:
	if max_retained < 0:
		return
	var retained: Array[Dictionary] = []
	for chunk_key: String in _map_chunk_entries.keys():
		if _target_map_chunk_keys.has(chunk_key):
			continue
		if _pending_map_texture_jobs.has(chunk_key):
			continue
		var entry: Dictionary = _map_chunk_entries[chunk_key]
		if not entry.has("texture"):
			continue
		retained.append({
			"key": chunk_key,
			"last_texture_use": int(entry.get("last_texture_use", 0)),
		})
	if retained.size() <= max_retained:
		return
	retained.sort_custom(func(left: Dictionary, right: Dictionary) -> bool:
		return int(left["last_texture_use"]) < int(right["last_texture_use"])
	)
	var evict_count := retained.size() - max_retained
	for index in range(evict_count):
		var chunk_key := str(retained[index]["key"])
		_map_chunk_entries.erase(chunk_key)
	if evict_count > 0:
		_map_chunk_entries_revision += 1


func _compact_ready_map_texture_results() -> void:
	if _ready_map_texture_result_read_index <= 0:
		return
	if _ready_map_texture_result_read_index >= _ready_map_texture_results.size():
		_ready_map_texture_results.clear()
		_ready_map_texture_result_read_index = 0
	elif _ready_map_texture_result_read_index >= 32:
		_ready_map_texture_results = _ready_map_texture_results.slice(_ready_map_texture_result_read_index)
		_ready_map_texture_result_read_index = 0


func _resource_colors_for_tiles(tiles: Array) -> Dictionary:
	var resource_colors := {}
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		var resource_id := str(tile.get("resource", ""))
		if resource_id.is_empty() or resource_colors.has(resource_id):
			continue
		var terrain_color: Color = TERRAIN_COLORS.get(str(tile.get("terrain", "ground")), TERRAIN_COLORS["ground"])
		resource_colors[resource_id] = _resource_color(resource_id, terrain_color)
	return resource_colors


func _flatten_chunk_tiles(chunks: Array) -> Array:
	var tiles: Array = []
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		for raw_tile: Variant in chunk.get("tiles", []):
			tiles.append(raw_tile)
	return tiles


func _tiles_for_query_rect(query_rect: Rect2i) -> Array:
	if _query_tiles_cache_revision == _map_chunk_entries_revision and _query_tiles_cache_rect == query_rect:
		return _query_tiles_cache

	var tiles: Array = []
	for chunk_key: String in _visible_map_chunk_keys_for_bounds(query_rect):
		var entry: Dictionary = _map_chunk_entries[chunk_key]
		for raw_tile: Variant in entry.get("tiles", []):
			var tile: Dictionary = raw_tile
			var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
			if not query_rect.has_point(pos):
				continue
			tiles.append(raw_tile)
	_query_tiles_cache = tiles
	_query_tiles_cache_rect = query_rect
	_query_tiles_cache_revision = _map_chunk_entries_revision
	_query_tiles_cache_rebuild_count += 1
	return tiles


func _buildings_by_chunk_key(chunks: Array, buildings: Array, buildings_key := "") -> Dictionary:
	var buildings_by_key := {}
	if chunks.size() == 1:
		var only_chunk: Dictionary = chunks[0]
		var only_bounds: Rect2i = only_chunk.get("bounds", Rect2i())
		var only_key := str(only_chunk.get("key", _chunk_key_from_bounds(only_bounds)))
		buildings_by_key[only_key] = buildings.duplicate(false)
		return buildings_by_key
	var grid_index := _chunk_grid_key_index(chunks)
	if bool(grid_index.get("complete", false)):
		return _buildings_by_chunk_key_from_grid(chunks, buildings, grid_index, buildings_key)

	var chunk_bounds_by_key: Array[Dictionary] = []
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		var bounds: Rect2i = chunk.get("bounds", Rect2i())
		if bounds.size.x <= 0 or bounds.size.y <= 0:
			continue
		var chunk_key := str(chunk.get("key", _chunk_key_from_bounds(bounds)))
		buildings_by_key[chunk_key] = []
		chunk_bounds_by_key.append({
			"key": chunk_key,
			"bounds": bounds,
		})

	for raw_building: Variant in buildings:
		var building: Dictionary = raw_building
		var assigned_keys := {}
		for raw_tile: Variant in building.get("footprint", []):
			var tile: Dictionary = raw_tile
			var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
			for chunk_entry: Dictionary in chunk_bounds_by_key:
				var bounds: Rect2i = chunk_entry["bounds"]
				if not bounds.has_point(pos):
					continue
				var chunk_key := str(chunk_entry["key"])
				if assigned_keys.has(chunk_key):
					continue
				buildings_by_key[chunk_key].append(building)
				assigned_keys[chunk_key] = true
	return buildings_by_key


func _chunk_grid_key_index(chunks: Array) -> Dictionary:
	var keys_by_pos := {}
	var cell_size := Vector2i.ZERO
	if chunks.is_empty():
		return {"complete": false}
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		if not chunk.has("chunk"):
			return {"complete": false}
		var bounds: Rect2i = chunk.get("bounds", Rect2i())
		if bounds.size.x <= 0 or bounds.size.y <= 0:
			return {"complete": false}
		if cell_size == Vector2i.ZERO:
			cell_size = bounds.size
		elif cell_size != bounds.size:
			return {"complete": false}
		var chunk_pos: Vector2i = chunk["chunk"]
		keys_by_pos[chunk_pos] = str(chunk.get("key", _chunk_key_from_bounds(bounds)))
	return {
		"complete": cell_size.x > 0 and cell_size.y > 0,
		"keys_by_pos": keys_by_pos,
		"cell_size": cell_size,
	}


func _buildings_by_chunk_key_from_grid(
	chunks: Array,
	buildings: Array,
	grid_index: Dictionary,
	buildings_key := ""
) -> Dictionary:
	var buildings_by_key := {}
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		var bounds: Rect2i = chunk.get("bounds", Rect2i())
		var chunk_key := str(chunk.get("key", _chunk_key_from_bounds(bounds)))
		buildings_by_key[chunk_key] = []
	var keys_by_pos: Dictionary = grid_index["keys_by_pos"]
	var cell_size: Vector2i = grid_index["cell_size"]
	var buildings_by_grid_chunk := _buildings_by_grid_chunk(buildings, cell_size, buildings_key)
	for chunk_pos: Vector2i in keys_by_pos.keys():
		if not buildings_by_grid_chunk.has(chunk_pos):
			continue
		var chunk_key := str(keys_by_pos[chunk_pos])
		if not buildings_by_key.has(chunk_key):
			continue
		buildings_by_key[chunk_key] = buildings_by_grid_chunk[chunk_pos]
	return buildings_by_key


func _buildings_by_grid_chunk(buildings: Array, cell_size: Vector2i, buildings_key := "") -> Dictionary:
	var effective_buildings_key := buildings_key
	if effective_buildings_key.is_empty():
		effective_buildings_key = _buildings_key(buildings)
	var cache_key := "%s:%d:%d" % [effective_buildings_key, cell_size.x, cell_size.y]
	if _building_grid_chunk_cache_key == cache_key:
		return _building_grid_chunk_cache
	var buildings_by_grid_chunk := {}
	for raw_building: Variant in buildings:
		var building: Dictionary = raw_building
		var assigned_chunks := {}
		for raw_tile: Variant in building.get("footprint", []):
			var tile: Dictionary = raw_tile
			var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
			var chunk_pos := _tile_to_grid_chunk(pos, cell_size)
			if assigned_chunks.has(chunk_pos):
				continue
			if not buildings_by_grid_chunk.has(chunk_pos):
				buildings_by_grid_chunk[chunk_pos] = []
			buildings_by_grid_chunk[chunk_pos].append(building)
			assigned_chunks[chunk_pos] = true
	_building_grid_chunk_cache_key = cache_key
	_building_grid_chunk_cache = buildings_by_grid_chunk
	_building_grid_chunk_building_key_cache = {}
	for chunk_pos: Vector2i in buildings_by_grid_chunk.keys():
		_building_grid_chunk_building_key_cache[chunk_pos] = _buildings_key_value(buildings_by_grid_chunk[chunk_pos])
	_building_grid_chunk_cache_rebuild_count += 1
	return _building_grid_chunk_cache


func _building_key_for_chunk(chunk: Dictionary, chunk_buildings: Array) -> String:
	if chunk.has("chunk"):
		var chunk_pos: Vector2i = chunk["chunk"]
		if _building_grid_chunk_building_key_cache.has(chunk_pos):
			return str(_building_grid_chunk_building_key_cache[chunk_pos])
		if not chunk_buildings.is_empty():
			return _buildings_key(chunk_buildings)
		return _buildings_key_value([])
	return _buildings_key(chunk_buildings)


func _visible_map_chunk_keys_for_bounds(bounds: Rect2i) -> Array[String]:
	if (
		_visible_map_chunk_cache_revision == _map_chunk_entries_revision
		and _visible_map_chunk_cache_bounds == bounds
	):
		return _visible_map_chunk_keys_cache

	var keys: Array[String] = []
	if _map_chunk_grid_complete:
		keys = _visible_map_chunk_grid_keys_for_bounds(bounds)
	else:
		for chunk_key: String in _map_chunk_entries.keys():
			if not _target_map_chunk_keys.has(chunk_key):
				continue
			var entry: Dictionary = _map_chunk_entries[chunk_key]
			var chunk_bounds: Rect2i = entry.get("bounds", Rect2i())
			var visible_bounds := _rect_intersection(bounds, chunk_bounds)
			if visible_bounds.size.x <= 0 or visible_bounds.size.y <= 0:
				continue
			keys.append(chunk_key)
	_visible_map_chunk_keys_cache = keys
	_visible_map_chunk_cache_bounds = bounds
	_visible_map_chunk_cache_revision = _map_chunk_entries_revision
	return _visible_map_chunk_keys_cache


func _visible_map_chunk_grid_keys_for_bounds(bounds: Rect2i) -> Array[String]:
	var keys: Array[String] = []
	if bounds.size.x <= 0 or bounds.size.y <= 0:
		return keys
	var min_chunk := _tile_to_grid_chunk(bounds.position, _map_chunk_grid_cell_size)
	var max_chunk := _tile_to_grid_chunk(bounds.position + bounds.size - Vector2i.ONE, _map_chunk_grid_cell_size)
	for chunk_y in range(min_chunk.y, max_chunk.y + 1):
		for chunk_x in range(min_chunk.x, max_chunk.x + 1):
			var chunk_pos := Vector2i(chunk_x, chunk_y)
			if not _map_chunk_keys_by_grid_pos.has(chunk_pos):
				continue
			var chunk_key := str(_map_chunk_keys_by_grid_pos[chunk_pos])
			if not _map_chunk_entries.has(chunk_key):
				continue
			var entry: Dictionary = _map_chunk_entries[chunk_key]
			var chunk_bounds: Rect2i = entry.get("bounds", Rect2i())
			var visible_bounds := _rect_intersection(bounds, chunk_bounds)
			if visible_bounds.size.x <= 0 or visible_bounds.size.y <= 0:
				continue
			keys.append(chunk_key)
	return keys


func _tile_to_grid_chunk(tile: Vector2i, grid_cell_size: Vector2i) -> Vector2i:
	return Vector2i(
		int(floor(float(tile.x) / float(maxi(grid_cell_size.x, 1)))),
		int(floor(float(tile.y) / float(maxi(grid_cell_size.y, 1))))
	)


func _chunk_sync_key_for(chunks: Array, buildings_key: String) -> String:
	_chunk_sync_key_compute_count += 1
	var hash_value := hash([chunks.size(), buildings_key])
	for raw_chunk: Variant in chunks:
		var chunk: Dictionary = raw_chunk
		hash_value = hash([
			hash_value,
			str(chunk.get("key", "")),
			chunk.get("bounds", Rect2i()),
			_chunk_signature(chunk),
		])
	return str(hash_value)


func _map_chunk_snapshot_key(chunk_signature: String, buildings_key: String, bounds: Rect2i) -> String:
	return str(hash([bounds, chunk_signature, buildings_key]))


func _chunk_signature(chunk: Dictionary) -> String:
	if chunk.has("signature"):
		return str(chunk["signature"])
	return _tiles_signature(chunk.get("tiles", []))


func _tiles_signature(tiles: Array) -> String:
	var hash_value := hash(tiles.size())
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		hash_value = hash([
			hash_value,
			int(tile.get("x", 0)),
			int(tile.get("y", 0)),
			str(tile.get("terrain", "")),
			str(tile.get("resource", "")),
			int(tile.get("amount", 0)),
			bool(tile.get("render", true)),
		])
	return str(hash_value)


func _chunk_key_from_bounds(bounds: Rect2i) -> String:
	return "%d:%d:%d:%d" % [bounds.position.x, bounds.position.y, bounds.size.x, bounds.size.y]


func _buildings_key(buildings: Array) -> String:
	_buildings_key_compute_count += 1
	return _buildings_key_value(buildings)


func _buildings_key_value(buildings: Array) -> String:
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


func _draw_player_marker(bounds: Rect2i, tile_scale: float) -> void:
	var player_tile := Vector2i(int(round(_player_position.x)), int(round(_player_position.z)))
	if not bounds.has_point(player_tile):
		return
	var center := size * 0.5 if _is_minimap else _tile_to_local(player_tile, bounds, tile_scale, true) + Vector2(tile_scale, tile_scale) * 0.5
	var radius := PLAYER_MARKER_RADIUS
	draw_circle(center, radius, Color(0.12, 0.92, 1.0, 1.0))
	if is_fullscreen_open():
		var font := get_theme_default_font()
		draw_string(
			font,
			center + Vector2(radius + 5.0, -radius - 2.0),
			"player",
			HORIZONTAL_ALIGNMENT_LEFT,
			-1.0,
			14,
			Color(0.86, 1.0, 1.0, 1.0)
		)


func _draw_hovered_resource_vein(bounds: Rect2i, tile_scale: float) -> void:
	if _resource_hover_suspended:
		return
	var hovered := _local_to_tile(get_local_mouse_position(), bounds, tile_scale, true)
	if not bounds.has_point(hovered):
		return
	var vein := _resource_vein_for_hovered_tile(hovered)
	if vein.is_empty():
		return
	var color := Color(0.50, 0.95, 1.0, 0.92)
	if detailed_world_blend() < 1.0:
		for raw_tile: Variant in vein["tiles"]:
				var tile: Vector2i = raw_tile
				if bounds.has_point(tile):
					draw_rect(Rect2(_tile_to_local(tile, bounds, tile_scale, true), Vector2(tile_scale, tile_scale)), color, false, 2.0)
	var font := get_theme_default_font()
	var label := "%s: %d" % [_resource_label(str(vein["resource"])), int(vein["amount"])]
	draw_string(
		font,
		get_local_mouse_position() + Vector2(14.0, -8.0),
		label,
		HORIZONTAL_ALIGNMENT_LEFT,
		-1.0,
		14,
		Color(0.90, 1.0, 1.0, 1.0)
	)


func _resource_vein_for_hovered_tile(hovered: Vector2i) -> Dictionary:
	if _resource_hover_suspended:
		return {}
	var cache_key := _hovered_resource_vein_key(hovered)
	if _hovered_resource_vein_cache_key == cache_key:
		return _hovered_resource_vein_cache
	if not _current_visible_rect.has_point(hovered):
		_hovered_resource_vein_cache = {}
		_hovered_resource_vein_cache_key = cache_key
		return _hovered_resource_vein_cache
	_hovered_resource_vein_cache = collect_resource_vein_from_lookup(
		hovered,
		_visible_resource_lookup_for_query_rect(_current_visible_rect)
	)
	_hovered_resource_vein_cache_key = cache_key
	return _hovered_resource_vein_cache


func _visible_resource_lookup_for_query_rect(query_rect: Rect2i) -> Dictionary:
	if (
		_visible_resource_lookup_cache_revision == _map_chunk_entries_revision
		and _visible_resource_lookup_cache_rect == query_rect
	):
		return _visible_resource_lookup_cache

	var query_tiles := _tiles
	if query_tiles.is_empty():
		query_tiles = _tiles_for_query_rect(query_rect)
	_visible_resource_lookup_cache = _visible_resource_lookup(query_tiles, query_rect)
	_visible_resource_lookup_cache_rect = query_rect
	_visible_resource_lookup_cache_revision = _map_chunk_entries_revision
	_visible_resource_lookup_cache_rebuild_count += 1
	return _visible_resource_lookup_cache


func _hovered_resource_vein_key(hovered: Vector2i) -> String:
	return str(hash([
		hovered,
		_current_visible_rect,
		_map_chunk_entries_revision,
	]))


func _tile_to_local(tile: Vector2i, bounds: Rect2i, tile_scale: float, use_viewport_offset := false) -> Vector2:
	var local := Vector2(
		float(_tile_image_x(tile, bounds)) * tile_scale,
		float(_tile_image_y(tile, bounds)) * tile_scale
	)
	if use_viewport_offset:
		local += _viewport_pixel_offset(bounds, tile_scale)
	return local


func _local_to_tile(local: Vector2, bounds: Rect2i, tile_scale: float, use_viewport_offset := false) -> Vector2i:
	var adjusted_local := local
	if use_viewport_offset:
		adjusted_local -= _viewport_pixel_offset(bounds, tile_scale)
	var local_tile_x := int(floor(adjusted_local.x / tile_scale))
	var tile_x := bounds.position.x + local_tile_x
	if _mirror_chart_x():
		tile_x = bounds.position.x + bounds.size.x - 1 - local_tile_x
	var local_tile_y := int(floor(adjusted_local.y / tile_scale))
	var tile_y := bounds.position.y + local_tile_y
	if _mirror_chart_y():
		tile_y = bounds.position.y + bounds.size.y - 1 - local_tile_y
	return Vector2i(tile_x, tile_y)


func _tile_image_x(tile: Vector2i, bounds: Rect2i) -> int:
	return _tile_image_x_for(tile, bounds, _mirror_chart_x())


func _tile_image_y(tile: Vector2i, bounds: Rect2i) -> int:
	return _tile_image_y_for(tile, bounds, _mirror_chart_y())


func _tile_image_x_for(tile: Vector2i, bounds: Rect2i, mirror_x: bool) -> int:
	if mirror_x:
		return bounds.position.x + bounds.size.x - 1 - tile.x
	return tile.x - bounds.position.x


func _tile_image_y_for(tile: Vector2i, bounds: Rect2i, mirror_y: bool) -> int:
	if mirror_y:
		return bounds.position.y + bounds.size.y - 1 - tile.y
	return tile.y - bounds.position.y


func _rect_intersection(a: Rect2i, b: Rect2i) -> Rect2i:
	var min_pos := Vector2i(maxi(a.position.x, b.position.x), maxi(a.position.y, b.position.y))
	var a_end := a.position + a.size
	var b_end := b.position + b.size
	var max_pos := Vector2i(mini(a_end.x, b_end.x), mini(a_end.y, b_end.y))
	return Rect2i(min_pos, Vector2i(maxi(max_pos.x - min_pos.x, 0), maxi(max_pos.y - min_pos.y, 0)))


func _rect_contains_rect(outer: Rect2i, inner: Rect2i) -> bool:
	if inner.size.x <= 0 or inner.size.y <= 0:
		return true
	return _rect_intersection(outer, inner) == inner


func _tile_range_image_rect(tile_bounds: Rect2i, image_bounds: Rect2i) -> Rect2i:
	var first := tile_bounds.position
	var last := tile_bounds.position + tile_bounds.size - Vector2i.ONE
	var min_x := mini(_tile_image_x(first, image_bounds), _tile_image_x(last, image_bounds))
	var min_y := mini(_tile_image_y(first, image_bounds), _tile_image_y(last, image_bounds))
	return Rect2i(Vector2i(min_x, min_y), tile_bounds.size)


func _tile_region_local_rect(tile_bounds: Rect2i, target_bounds: Rect2i, tile_scale: float) -> Rect2:
	var target_rect := _tile_range_image_rect(tile_bounds, target_bounds)
	return Rect2(
		Vector2(target_rect.position) * tile_scale + _viewport_pixel_offset(target_bounds, tile_scale),
		Vector2(target_rect.size) * tile_scale
	)


func _viewport_pixel_offset(bounds: Rect2i, tile_scale: float) -> Vector2:
	var anchored_center := Vector2(bounds.position) + Vector2(bounds.size) * 0.5
	var center_delta := _map_center - anchored_center
	return Vector2(
		center_delta.x * tile_scale if _mirror_chart_x() else -center_delta.x * tile_scale,
		center_delta.y * tile_scale if _mirror_chart_y() else -center_delta.y * tile_scale
	)


func _tile_regions_outside_rect(outer: Rect2i, inner: Rect2i) -> Array[Rect2i]:
	if outer.size.x <= 0 or outer.size.y <= 0:
		return []
	if inner.size.x <= 0 or inner.size.y <= 0:
		return [outer]
	var clipped_inner := _rect_intersection(outer, inner)
	if clipped_inner.size.x <= 0 or clipped_inner.size.y <= 0:
		return [outer]

	var regions: Array[Rect2i] = []
	var outer_end := outer.position + outer.size
	var inner_end := clipped_inner.position + clipped_inner.size
	if clipped_inner.position.y > outer.position.y:
		regions.append(Rect2i(
			outer.position,
			Vector2i(outer.size.x, clipped_inner.position.y - outer.position.y)
		))
	if inner_end.y < outer_end.y:
		regions.append(Rect2i(
			Vector2i(outer.position.x, inner_end.y),
			Vector2i(outer.size.x, outer_end.y - inner_end.y)
		))
	if clipped_inner.position.x > outer.position.x:
		regions.append(Rect2i(
			Vector2i(outer.position.x, clipped_inner.position.y),
			Vector2i(clipped_inner.position.x - outer.position.x, clipped_inner.size.y)
		))
	if inner_end.x < outer_end.x:
		regions.append(Rect2i(
			Vector2i(inner_end.x, clipped_inner.position.y),
			Vector2i(outer_end.x - inner_end.x, clipped_inner.size.y)
		))
	return regions


func _tile_uses_detailed_world(tile: Vector2i) -> bool:
	return detailed_world_visible() and _current_visible_rect.has_point(tile)


func _mirror_chart_x() -> bool:
	return not _is_minimap


func _mirror_chart_y() -> bool:
	return not _is_minimap


func _color_from_id(id: String) -> Color:
	var hash_value := id.hash()
	var r := 0.30 + float(hash_value & 0x3f) / 160.0
	var g := 0.30 + float((hash_value >> 8) & 0x3f) / 160.0
	var b := 0.30 + float((hash_value >> 16) & 0x3f) / 160.0
	return Color(r, g, b, 1.0)


func _fogged_color(color: Color) -> Color:
	return color.darkened(FOGGED_TILE_BLEND).lerp(MAP_BACKGROUND_COLOR, 0.28)


func _resource_color(resource_id: String, terrain_color: Color) -> Color:
	if RESOURCE_COLORS.has(resource_id):
		return RESOURCE_COLORS[resource_id]
	var catalog_color := ItemCatalogScript.color(resource_id)
	if catalog_color == Color(0.54, 0.56, 0.54):
		return terrain_color
	return catalog_color


func _resource_label(resource_id: String) -> String:
	return RESOURCE_LABELS.get(resource_id, ItemCatalogScript.display_name(resource_id))


func _gui_input(event: InputEvent) -> void:
	if not is_fullscreen_open():
		return
	if event is InputEventMouseMotion:
		handle_fullscreen_mouse_motion(event)
	elif event is InputEventMouseButton:
		if event.pressed:
			if event.button_index == MOUSE_BUTTON_WHEEL_UP:
				zoom_by(1.25)
				accept_event()
			elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
				zoom_by(0.80)
				accept_event()
		elif event.button_index == MOUSE_BUTTON_LEFT:
			_resource_hover_suspended = false
			refresh_resource_selection()
			accept_event()


func handle_fullscreen_mouse_motion(event: InputEventMouseMotion) -> void:
	if not is_fullscreen_open():
		return
	if (event.button_mask & MOUSE_BUTTON_MASK_LEFT) != 0:
		_resource_hover_suspended = true
		drag_by(event.relative)
	else:
		_resource_hover_suspended = false
		refresh_resource_selection()
	accept_event()
