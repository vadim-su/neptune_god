extends Control
class_name MapOverlay

const ItemCatalogScript := preload("res://game/items/item_catalog.gd")

const RESOURCE_VEIN_NEIGHBOR_DISTANCE := 2
const DETAILED_WORLD_PIXELS_PER_TILE := 18.0
const DETAILED_WORLD_TRANSITION_PIXELS := 8.0
const MIN_PIXELS_PER_TILE := 1.0
const MAX_PIXELS_PER_TILE := 48.0
const MINIMAP_SIZE := Vector2(160.0, 160.0)
const MINIMAP_MARGIN := 16.0
const MAP_TEXTURE_BACKGROUND := Color(0.035, 0.055, 0.045, 1.0)

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

var _is_minimap := false
var _fullscreen_open := false
var _tiles: Array = []
var _buildings: Array = []
var _visible_rect := Rect2i(Vector2i.ZERO, Vector2i.ONE)
var _player_position := Vector3.ZERO
var _pixels_per_tile := 4.0
var _map_center := Vector2.ZERO
var _center_initialized := false
var _schematic_texture: ImageTexture
var _schematic_texture_bounds := Rect2i()
var _schematic_texture_dirty := true


static func collect_resource_vein(start: Vector2i, tiles: Array, visible_rect: Rect2i) -> Dictionary:
	var lookup := _visible_resource_lookup(tiles, visible_rect)
	if not lookup.has(start):
		return {}

	var start_deposit: Dictionary = lookup[start]
	var resource_id := str(start_deposit["resource"])
	var queue: Array[Vector2i] = [start]
	var visited := {start: true}
	var vein_tiles: Array[Vector2i] = []
	var amount := 0

	while not queue.is_empty():
		var tile := queue.pop_front() as Vector2i
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
	_fullscreen_open = open
	visible = open
	if open and not _center_initialized:
		center_on_player()
	queue_redraw()


func is_fullscreen_open() -> bool:
	return not _is_minimap and _fullscreen_open


func set_world_snapshot(tiles: Array, buildings: Array, visible_rect: Rect2i, player_position: Vector3) -> void:
	_tiles = tiles.duplicate(true)
	_buildings = buildings.duplicate(true)
	if visible_rect.size.x > 0 and visible_rect.size.y > 0:
		_visible_rect = visible_rect
	_schematic_texture_dirty = true
	set_player_position(player_position)
	queue_redraw()


func set_player_position(position: Vector3) -> void:
	if _center_initialized and _player_position == position:
		return
	_player_position = position
	if not _center_initialized:
		center_on_player()
	elif is_fullscreen_open():
		_map_center = Vector2(_player_position.x, _player_position.z)
	if visible:
		queue_redraw()


func center_on_player() -> void:
	_map_center = Vector2(_player_position.x, _player_position.z)
	_center_initialized = true
	queue_redraw()


func player_marker_snapshot() -> Dictionary:
	return {
		"tile": Vector2i(int(round(_player_position.x)), int(round(_player_position.z))),
		"label": "player",
	}


func set_pixels_per_tile(value: float) -> void:
	_pixels_per_tile = clamp(value, MIN_PIXELS_PER_TILE, MAX_PIXELS_PER_TILE)
	queue_redraw()


func pixels_per_tile() -> float:
	return _pixels_per_tile


func zoom_by(factor: float) -> void:
	set_pixels_per_tile(_pixels_per_tile * factor)


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


func chart_layer_alpha() -> float:
	if not visible:
		return 0.0
	return 1.0 - detailed_world_blend()


func schematic_image_for_tests(bounds: Rect2i) -> Image:
	return _schematic_image(bounds)


func schematic_texture_for_tests(bounds: Rect2i) -> ImageTexture:
	return _schematic_texture_for_bounds(bounds)


func resource_color_for_tests(resource_id: String, terrain_color: Color) -> Color:
	return _resource_color(resource_id, terrain_color)


func _draw() -> void:
	if not visible:
		return

	var bounds := _current_bounds()
	var tile_scale := _tile_scale(bounds)
	var chart_alpha := chart_layer_alpha()
	if chart_alpha > 0.0:
		var map_alpha := 0.86 if _is_minimap else 0.88
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.035, 0.055, 0.045, map_alpha * chart_alpha), true)
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.45, 0.58, 0.48, 0.80), false, 1.0)

	if chart_alpha > 0.0:
		_draw_schematic_texture(bounds, tile_scale, chart_alpha)
	_draw_player_marker(bounds, tile_scale)
	if is_fullscreen_open():
		_draw_hovered_resource_vein(bounds, tile_scale)


func _current_bounds() -> Rect2i:
	if _is_minimap:
		return _visible_rect

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


func _draw_schematic_texture(bounds: Rect2i, tile_scale: float, alpha: float) -> void:
	var texture := _schematic_texture_for_bounds(bounds)
	draw_texture_rect(
		texture,
		Rect2(Vector2.ZERO, Vector2(float(bounds.size.x), float(bounds.size.y)) * tile_scale),
		false,
		Color(1.0, 1.0, 1.0, alpha)
	)


func _schematic_texture_for_bounds(bounds: Rect2i) -> ImageTexture:
	if (
		not _schematic_texture_dirty
		and _schematic_texture != null
		and _schematic_texture_bounds.position == bounds.position
		and _schematic_texture_bounds.size == bounds.size
	):
		return _schematic_texture
	return _rebuild_schematic_texture(bounds)


func _rebuild_schematic_texture(bounds: Rect2i) -> ImageTexture:
	_schematic_texture = ImageTexture.create_from_image(_schematic_image(bounds))
	_schematic_texture_bounds = bounds
	_schematic_texture_dirty = false
	return _schematic_texture


func _schematic_image(bounds: Rect2i) -> Image:
	var width := maxi(bounds.size.x, 1)
	var height := maxi(bounds.size.y, 1)
	var image := Image.create_empty(width, height, false, Image.FORMAT_RGBA8)
	image.fill(MAP_TEXTURE_BACKGROUND)

	for raw_tile: Variant in _tiles:
		var tile: Dictionary = raw_tile
		if not bool(tile.get("render", true)):
			continue
		var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
		if not bounds.has_point(pos):
			continue
		var color: Color = TERRAIN_COLORS.get(str(tile.get("terrain", "ground")), TERRAIN_COLORS["ground"])
		var resource_id := str(tile.get("resource", ""))
		if not resource_id.is_empty() and int(tile.get("amount", 0)) > 0:
			color = _resource_color(resource_id, color).lerp(Color.WHITE, 0.08)
		image.set_pixel(_tile_image_x(pos, bounds), _tile_image_y(pos, bounds), color)

	for raw_building: Variant in _buildings:
		var building: Dictionary = raw_building
		var def_id := str(building.get("def_id", ""))
		var color: Color = BUILDING_COLORS.get(def_id, _color_from_id(def_id))
		for raw_tile: Variant in building.get("footprint", []):
			var tile: Dictionary = raw_tile
			var pos := Vector2i(int(tile.get("x", 0)), int(tile.get("y", 0)))
			if bounds.has_point(pos):
				image.set_pixel(_tile_image_x(pos, bounds), _tile_image_y(pos, bounds), color)

	return image


func _draw_player_marker(bounds: Rect2i, tile_scale: float) -> void:
	var player_tile := Vector2i(int(round(_player_position.x)), int(round(_player_position.z)))
	if not bounds.has_point(player_tile):
		return
	var center := _tile_to_local(player_tile, bounds, tile_scale) + Vector2(tile_scale, tile_scale) * 0.5
	var radius: float = clampf(tile_scale * 0.46, 3.0, 7.0)
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
	var hovered := _local_to_tile(get_local_mouse_position(), bounds, tile_scale)
	if not bounds.has_point(hovered):
		return
	var vein := collect_resource_vein(hovered, _tiles, _visible_rect)
	if vein.is_empty():
		return
	var color := Color(0.50, 0.95, 1.0, 0.92)
	if detailed_world_blend() < 1.0:
		for raw_tile: Variant in vein["tiles"]:
			var tile: Vector2i = raw_tile
			if bounds.has_point(tile):
				draw_rect(Rect2(_tile_to_local(tile, bounds, tile_scale), Vector2(tile_scale, tile_scale)), color, false, 2.0)
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


func _tile_to_local(tile: Vector2i, bounds: Rect2i, tile_scale: float) -> Vector2:
	return Vector2(
		float(_tile_image_x(tile, bounds)) * tile_scale,
		float(bounds.position.y + bounds.size.y - 1 - tile.y) * tile_scale
	)


func _local_to_tile(local: Vector2, bounds: Rect2i, tile_scale: float) -> Vector2i:
	var local_tile_x := int(floor(local.x / tile_scale))
	var tile_x := bounds.position.x + local_tile_x
	if _mirror_chart_x():
		tile_x = bounds.position.x + bounds.size.x - 1 - local_tile_x
	var tile_y := bounds.position.y + bounds.size.y - 1 - int(floor(local.y / tile_scale))
	return Vector2i(tile_x, tile_y)


func _tile_image_x(tile: Vector2i, bounds: Rect2i) -> int:
	if _mirror_chart_x():
		return bounds.position.x + bounds.size.x - 1 - tile.x
	return tile.x - bounds.position.x


func _tile_image_y(tile: Vector2i, bounds: Rect2i) -> int:
	return bounds.position.y + bounds.size.y - 1 - tile.y


func _mirror_chart_x() -> bool:
	return not _is_minimap


func _color_from_id(id: String) -> Color:
	var hash_value := id.hash()
	var r := 0.30 + float(hash_value & 0x3f) / 160.0
	var g := 0.30 + float((hash_value >> 8) & 0x3f) / 160.0
	var b := 0.30 + float((hash_value >> 16) & 0x3f) / 160.0
	return Color(r, g, b, 1.0)


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
		queue_redraw()
	elif event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			zoom_by(1.25)
			accept_event()
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			zoom_by(0.80)
			accept_event()
