extends PanelContainer
class_name Minimap

@onready var map_overlay: Control = %MinimapMap

const MINIMAP_SIZE := Vector2(160.0, 160.0)
const MINIMAP_MARGIN := 16.0


func _ready() -> void:
	configure_minimap()


func configure_minimap() -> void:
	_resolve_map_overlay()
	visible = true
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	clip_contents = true
	anchor_left = 1.0
	anchor_top = 1.0
	anchor_right = 1.0
	anchor_bottom = 1.0
	offset_left = -MINIMAP_SIZE.x - MINIMAP_MARGIN
	offset_top = -MINIMAP_SIZE.y - MINIMAP_MARGIN
	offset_right = -MINIMAP_MARGIN
	offset_bottom = -MINIMAP_MARGIN
	custom_minimum_size = MINIMAP_SIZE
	size = MINIMAP_SIZE
	if map_overlay == null:
		return
	map_overlay.configure_minimap()
	map_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	map_overlay.offset_left = 0.0
	map_overlay.offset_top = 0.0
	map_overlay.offset_right = 0.0
	map_overlay.offset_bottom = 0.0
	map_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE


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
	_resolve_map_overlay()
	map_overlay.set_chunk_snapshot(
		chunks,
		visible_rect,
		buildings,
		player_position,
		current_visible_rect,
		cache_tiles_for_queries,
		buildings_key,
		chunk_snapshot_key
	)


func set_player_position(position: Vector3) -> void:
	_resolve_map_overlay()
	map_overlay.set_player_position(position)


func _current_bounds() -> Rect2i:
	_resolve_map_overlay()
	return map_overlay.current_tile_bounds()


func _resolve_map_overlay() -> void:
	if map_overlay != null:
		return
	map_overlay = get_node_or_null("MinimapMap") as Control
