extends Node3D

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const RecipeCatalogScript := preload("res://game/recipes/recipe_catalog.gd")
const BuildingRendererScript := preload("res://game/buildings/building_renderer.gd")
const SelectionOutlineScript := preload("res://game/buildings/selection_outline.gd")
const MachineWindowScene := preload("res://game/ui/machine_window.tscn")
const CatalogSelectorScene := preload("res://game/ui/catalog_selector.tscn")
const InventoryWindowScene := preload("res://game/ui/inventory_window.tscn")
const DevConsoleScene := preload("res://game/ui/dev_console.tscn")
const BuildGhostScene := preload("res://game/main/build_ghost.tscn")
const CameraControllerScript := preload("res://game/main/camera_controller.gd")
const DevConsoleControllerScript := preload("res://game/main/dev_console_controller.gd")
const InventoryControllerScript := preload("res://game/main/inventory_controller.gd")
const MapOverlayControllerScene := preload("res://game/main/map_overlay_controller.tscn")
const PlayerZoneOverlayScene := preload("res://game/main/player_zone_overlay.tscn")
const WorldStreamingControllerScript := preload("res://game/main/world_streaming_controller.gd")
const HOTBAR_SELECTOR_OWNER_PREFIX := "hotbar:"

@onready var camera: Camera3D = %Camera3D
@onready var player: PlayerController = %Player
@onready var buildings_root: Node3D = %Buildings
@onready var status_label: Label = %StatusLabel
@onready var fps_counter: Control = %FpsCounter
@onready var hotbar = %Hotbar
@onready var environment: Node3D = $Environment

const CAMERA_MIN_DISTANCE := 10.0
const CAMERA_MAX_DISTANCE := 96.0
const CAMERA_TARGET_HEIGHT := 0.75
const CAMERA_GENERATION_MARGIN_TILES := 16
const CAMERA_MAX_VISIBLE_TILE_RADIUS := 64
const STREAMING_PRELOAD_CHUNK_RING := 1
const CAMERA_MIN_ELEVATION := deg_to_rad(30.0)
const CAMERA_MAX_ELEVATION := deg_to_rad(76.0)
const CAMERA_FAR_PADDING := 32.0
const PLAYER_COLLISION_RADIUS := 0.32

enum BuildMode { NEUTRAL, BUILD }

var sim: NeptuneSim
var chunk_tile_provider: NeptuneChunkTileProvider
var camera_controller: RefCounted
var world_streaming_controller: RefCounted
var build_mode := BuildMode.NEUTRAL
var selected_building_id := ""
var selected_object_id := -1
var selected_object: Dictionary = {}
var build_quarter_turns := 0
var build_preview_tile := Vector2i.ZERO
var build_preview_valid := false
var build_ghost_root: Variant
var blocked_building_tiles := {}
var building_tile_index := {}
var selection_outline_root: Node3D
var machine_window: MachineWindow
var catalog_selector: Node
var inventory_window: Node
var inventory_controller: RefCounted
var dev_console: Node
var dev_console_controller: RefCounted
var map_overlay_controller: MapOverlayController
var minimap: Control
var map_overlay: Control
var chunk_size := 32
var player_zone_controller: Node
var initialized := false


func _ready() -> void:
	sim = NeptuneSim.new()
	if not sim.configure_catalogs(
		ItemCatalogScript.definitions(),
		RecipeCatalogScript.definitions(),
		BuildingCatalogScript.definitions(),
		_catalog_rows("terrain"),
		_catalog_rows("player"),
		_catalog_rows("resources"),
		_catalog_rows("worldgen")
	):
		push_error("Failed to configure simulation catalogs")
		return
	chunk_tile_provider = NeptuneChunkTileProvider.new()
	if not chunk_tile_provider.configure_worldgen(_catalog_rows("resources"), _catalog_rows("worldgen")):
		push_error("Failed to configure render worldgen provider")
		return
	chunk_size = int(sim.chunk_size())
	camera_controller = CameraControllerScript.new()
	camera_controller.min_distance = CAMERA_MIN_DISTANCE
	camera_controller.max_distance = CAMERA_MAX_DISTANCE
	camera_controller.target_height = CAMERA_TARGET_HEIGHT
	camera_controller.min_elevation = CAMERA_MIN_ELEVATION
	camera_controller.max_elevation = CAMERA_MAX_ELEVATION
	camera_controller.far_padding = CAMERA_FAR_PADDING
	camera_controller.max_visible_tile_radius = CAMERA_MAX_VISIBLE_TILE_RADIUS
	world_streaming_controller = WorldStreamingControllerScript.new()
	world_streaming_controller.setup(
		environment,
		chunk_tile_provider,
		chunk_size,
		CAMERA_MAX_VISIBLE_TILE_RADIUS,
		STREAMING_PRELOAD_CHUNK_RING
	)
	if environment.has_signal("chunks_changed"):
		environment.chunks_changed.connect(_on_environment_chunks_changed)
	player.can_move_to = Callable(self, "_is_player_position_walkable")
	_update_camera()
	_sync_world_around_camera(true)
	hotbar.selected.connect(_on_hotbar_selected)
	hotbar.assignment_requested.connect(_on_hotbar_assignment_requested)
	build_ghost_root = BuildGhostScene.instantiate()
	add_child(build_ghost_root)
	selection_outline_root = Node3D.new()
	selection_outline_root.name = "SelectionOutline"
	selection_outline_root.visible = false
	add_child(selection_outline_root)
	player_zone_controller = PlayerZoneOverlayScene.instantiate()
	add_child(player_zone_controller)
	player_zone_controller.setup(player, Callable(self, "_player_zone_should_be_visible"))
	machine_window = MachineWindowScene.instantiate() as MachineWindow
	$Hud.add_child(machine_window)
	machine_window.recipe_selected.connect(_on_machine_recipe_selected)
	inventory_window = InventoryWindowScene.instantiate()
	$Hud.add_child(inventory_window)
	inventory_controller = InventoryControllerScript.new()
	inventory_controller.setup(
		inventory_window,
		machine_window,
		sim,
		Callable(self, "_selected_object_snapshot")
	)
	dev_console = DevConsoleScene.instantiate()
	$Hud.add_child(dev_console)
	dev_console_controller = DevConsoleControllerScript.new()
	dev_console_controller.setup(
		dev_console,
		sim,
		inventory_window,
		Callable(inventory_controller, "update"),
		Callable(self, "_selected_object_snapshot")
	)
	_create_map_overlays()
	catalog_selector = CatalogSelectorScene.instantiate()
	$Hud.add_child(catalog_selector)
	catalog_selector.entry_selected.connect(_on_catalog_selector_entry_selected)
	catalog_selector.closed.connect(_on_catalog_selector_closed)
	sim.tick_many(3)
	_update_status_label()
	_update_camera()
	_update_map_overlays()
	initialized = true


func _catalog_rows(catalog_kind: String) -> Array:
	var root := get_tree().root
	if root.has_meta("catalog_registry"):
		var catalog_registry: Variant = root.get_meta("catalog_registry")
		if catalog_registry != null and catalog_registry.has_method("rows"):
			var rows: Array = catalog_registry.rows(catalog_kind)
			if rows.is_empty():
				push_error("Catalog registry has no rows for '%s'" % catalog_kind)
			return rows

	push_error("Catalog registry is not available for '%s'" % catalog_kind)
	return []


func _process(delta: float) -> void:
	if not initialized:
		return

	player.input_blocked = _ui_blocks_gameplay_input()
	if inventory_controller != null and inventory_controller.is_open():
		inventory_controller.update()
	_update_camera()
	_sync_world_around_camera(false)
	_update_build_preview()
	if map_overlay_controller != null:
		map_overlay_controller.process(delta)


func _physics_process(_delta: float) -> void:
	if camera_controller != null:
		player.movement_yaw = camera_controller.yaw


func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_F1:
		dev_console.toggle_console()
		get_viewport().set_input_as_handled()
		return

	if dev_console != null and dev_console.is_open():
		if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_ESCAPE:
			dev_console.close_console()
			get_viewport().set_input_as_handled()
		return

	if catalog_selector != null and catalog_selector.is_open():
		if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_ESCAPE:
			catalog_selector.close_selector()
			get_viewport().set_input_as_handled()
		return

	if inventory_window != null and inventory_window.is_open():
		if event is InputEventKey and event.pressed and not event.echo:
			if event.keycode == KEY_ESCAPE or event.keycode == KEY_E:
				inventory_window.hide_window()
				get_viewport().set_input_as_handled()
		return

	if map_overlay != null and map_overlay.is_fullscreen_open():
		if event is InputEventKey and event.pressed and not event.echo:
			if event.keycode == KEY_ESCAPE or event.keycode == KEY_M:
				if map_overlay_controller != null:
					map_overlay_controller.close_fullscreen()
				else:
					map_overlay.set_fullscreen_open(false)
				_update_camera()
				get_viewport().set_input_as_handled()
		elif not (event is InputEventMouse):
			get_viewport().set_input_as_handled()
		return

	if event is InputEventMouseMotion and Input.is_mouse_button_pressed(MOUSE_BUTTON_MIDDLE):
		camera_controller.rotate(event.relative)
		_update_camera()
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.pressed:
		match event.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				camera_controller.zoom_in(0.5)
				_update_camera()
				get_viewport().set_input_as_handled()
			MOUSE_BUTTON_WHEEL_DOWN:
				camera_controller.zoom_out(0.5)
				_update_camera()
				get_viewport().set_input_as_handled()
			MOUSE_BUTTON_RIGHT:
				if not _is_pointer_over_ui():
					if build_mode == BuildMode.BUILD:
						_enter_neutral_mode()
					else:
						_try_remove_building_at_pointer()
					get_viewport().set_input_as_handled()
			MOUSE_BUTTON_LEFT:
				if not _is_pointer_over_ui():
					if build_mode == BuildMode.BUILD:
						_try_place_selected_building()
					else:
						_try_select_building_at_pointer()
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.keycode == KEY_ESCAPE:
			if build_mode == BuildMode.BUILD:
				_enter_neutral_mode()
			else:
				_clear_selected_object()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_E:
			_toggle_inventory_window()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_M:
			_toggle_map_overlay()
			get_viewport().set_input_as_handled()
		elif event.keycode == KEY_R and build_mode == BuildMode.BUILD and not selected_building_id.is_empty():
			build_quarter_turns = (build_quarter_turns + 1) % 4
			_update_build_preview()
			get_viewport().set_input_as_handled()


func _is_pointer_over_ui() -> bool:
	return get_viewport().gui_get_hovered_control() != null


func _ui_blocks_gameplay_input() -> bool:
	return (
		(dev_console != null and dev_console.is_open())
		or (catalog_selector != null and catalog_selector.is_open())
		or (inventory_window != null and inventory_window.is_open())
		or (map_overlay != null and map_overlay.is_fullscreen_open())
	)


func _update_camera() -> void:
	if camera_controller == null:
		return
	camera_controller.apply(camera, player.global_position, map_overlay, get_viewport())


func _map_camera_up_vector(detailed_blend: float) -> Vector3:
	return CameraControllerScript.map_camera_up_vector(detailed_blend)


func _detailed_map_camera_height(pixels_per_tile: float, viewport_height: float, fov_degrees: float) -> float:
	return CameraControllerScript.detailed_map_camera_height(pixels_per_tile, viewport_height, fov_degrees)


func _gameplay_camera_far_clip() -> float:
	if camera_controller == null:
		return CAMERA_MAX_DISTANCE + float(CAMERA_MAX_VISIBLE_TILE_RADIUS) + CAMERA_FAR_PADDING
	return camera_controller.gameplay_far_clip()


func _should_render_3d_world() -> bool:
	if map_overlay_controller != null:
		return map_overlay_controller.should_render_3d_world()
	if camera_controller == null:
		if map_overlay == null or not map_overlay.is_fullscreen_open():
			return true
		if not map_overlay.has_method("detailed_world_visible"):
			return true
		return map_overlay.detailed_world_visible()
	return camera_controller.should_render_3d_world(map_overlay)


func _update_3d_rendering_gate() -> void:
	if camera_controller == null:
		return
	camera_controller._update_3d_rendering_gate(get_viewport(), map_overlay)


func _sync_world_around_camera(force: bool) -> void:
	if world_streaming_controller == null:
		return
	var changed: bool = world_streaming_controller.sync_around_position(_streaming_center_position(player.global_position), force)
	if not changed:
		return
	_mark_map_snapshot_dirty()
	_update_status_label()


func _create_map_overlays() -> void:
	map_overlay_controller = MapOverlayControllerScene.instantiate() as MapOverlayController
	$Hud.add_child(map_overlay_controller)
	map_overlay_controller.setup($Hud, environment, sim, player, hotbar)
	map_overlay_controller.view_changed.connect(_on_map_overlay_view_changed)
	minimap = map_overlay_controller.minimap
	map_overlay = map_overlay_controller.map_overlay
	if fps_counter != null:
		fps_counter.keep_above_control(map_overlay)


func _keep_hotbar_above_map_overlay() -> void:
	if map_overlay_controller != null:
		map_overlay_controller.keep_hotbar_above_map_overlay()
		return
	if hotbar == null or map_overlay == null:
		return
	hotbar.z_index = map_overlay.z_index + 10


func _toggle_map_overlay() -> void:
	if map_overlay_controller == null:
		return
	map_overlay_controller.toggle_fullscreen()
	_update_camera()


func _update_map_overlays(force_snapshot: bool = false) -> void:
	if map_overlay_controller != null:
		map_overlay_controller.update_snapshots(force_snapshot)


func _apply_map_overlay_snapshots(
	map_snapshot: Dictionary,
	visible_snapshot: Dictionary,
	buildings: Array,
	player_position: Vector3
) -> void:
	if map_overlay_controller != null:
		map_overlay_controller.apply_chunk_snapshots(map_snapshot, visible_snapshot, buildings, player_position)


func _update_map_overlay_player_position() -> void:
	if map_overlay_controller != null:
		map_overlay_controller.update_player_position()
		return
	if minimap == null or map_overlay == null or player == null:
		return
	minimap.visible = not map_overlay.is_fullscreen_open()
	minimap.set_player_position(player.global_position)
	map_overlay.set_player_position(player.global_position)


func _refresh_map_overlay_resource_selection() -> void:
	if map_overlay_controller != null:
		map_overlay_controller.refresh_resource_selection()
		return
	if map_overlay != null and map_overlay.has_method("refresh_resource_selection"):
		map_overlay.refresh_resource_selection()


func _mark_map_snapshot_dirty() -> void:
	if map_overlay_controller != null:
		map_overlay_controller.mark_dirty()


func _on_environment_chunks_changed() -> void:
	_mark_map_snapshot_dirty()
	_update_status_label()


func _on_map_overlay_view_changed() -> void:
	if camera != null and player != null:
		_update_camera()


func _visible_chunk_rect_for(tile_rect: Rect2i, player_position: Vector3) -> Rect2i:
	return WorldStreamingControllerScript.visible_chunk_rect_for(
		tile_rect,
		player_position,
		chunk_size,
		CAMERA_GENERATION_MARGIN_TILES,
		CAMERA_MAX_VISIBLE_TILE_RADIUS
	)


func _streaming_chunk_rect_for(player_position: Vector3) -> Rect2i:
	return WorldStreamingControllerScript.streaming_chunk_rect_for(
		player_position,
		chunk_size,
		CAMERA_MAX_VISIBLE_TILE_RADIUS
	)


func _preload_chunk_rect_for(visible_rect: Rect2i) -> Rect2i:
	return WorldStreamingControllerScript.preload_chunk_rect_for(visible_rect, STREAMING_PRELOAD_CHUNK_RING)


func _streaming_center_position(player_position: Vector3) -> Vector3:
	return player_position


func _clamp_tile_to_visible_radius(tile: Vector2i, center: Vector2i) -> Vector2i:
	return WorldStreamingControllerScript.clamp_tile_to_visible_radius(tile, center, CAMERA_MAX_VISIBLE_TILE_RADIUS)


func _world_to_tile(position: Vector3) -> Vector2i:
	return WorldStreamingControllerScript.world_to_tile(position)


func _tile_to_chunk(tile: Vector2i) -> Vector2i:
	return WorldStreamingControllerScript.tile_to_chunk(tile, chunk_size)


func _chunks_in_rect(min_chunk: Vector2i, max_chunk: Vector2i) -> Array:
	return WorldStreamingControllerScript.chunks_in_rect(min_chunk, max_chunk)


func _should_show_player_zone_overlay() -> bool:
	if player_zone_controller != null:
		return player_zone_controller.should_show()
	return _player_zone_should_be_visible()


func _player_zone_should_be_visible() -> bool:
	return map_overlay == null or not map_overlay.is_fullscreen_open()


func _on_hotbar_selected(entry_id: String) -> void:
	if entry_id.is_empty():
		_enter_neutral_mode()
		return

	build_mode = BuildMode.BUILD
	selected_building_id = entry_id
	_clear_selected_object()
	build_quarter_turns = 0
	_update_build_preview()
	_update_status_label()


func _on_hotbar_assignment_requested(slot_index: int) -> void:
	var owner := "%s%d" % [HOTBAR_SELECTOR_OWNER_PREFIX, slot_index]
	catalog_selector.open_selector(owner, _building_selector_entries(), "Build Catalog")


func _on_catalog_selector_entry_selected(owner_id: String, entry: Dictionary) -> void:
	if not owner_id.begins_with(HOTBAR_SELECTOR_OWNER_PREFIX):
		return
	var slot_index := int(owner_id.trim_prefix(HOTBAR_SELECTOR_OWNER_PREFIX))
	hotbar.assign_slot(slot_index, entry)


func _on_catalog_selector_closed(owner_id: String) -> void:
	if not owner_id.begins_with(HOTBAR_SELECTOR_OWNER_PREFIX):
		return
	var slot_index := int(owner_id.trim_prefix(HOTBAR_SELECTOR_OWNER_PREFIX))
	hotbar.cancel_assignment(slot_index)


func _selected_object_snapshot() -> Dictionary:
	return {
		"id": selected_object_id,
		"object": selected_object,
	}


func _toggle_inventory_window() -> void:
	if inventory_controller != null:
		inventory_controller.toggle()


func _building_selector_entries() -> Array:
	var entries: Array = []
	for raw_definition: Variant in BuildingCatalogScript.definitions():
		var definition: Dictionary = raw_definition
		var id := str(definition.get("id", ""))
		if id.is_empty():
			continue
		var label := BuildingCatalogScript.display_name(id)
		var category := BuildingCatalogScript.ui_type(id)
		entries.append({
			"id": id,
			"label": label,
			"kind": "building",
			"category": category,
			"usage_tags": ["buildable"],
			"search_text": "%s %s %s" % [id, label, category],
		})
	return entries


func _update_status_label() -> void:
	var object_text := "none"
	if not selected_object.is_empty():
		object_text = "%s #%d" % [str(selected_object["def_id"]), selected_object_id]

	status_label.text = "Neptune Godot runtime loaded\nTick: %d\nDigest: %d\nTiles: %d\nResources: %d\nMode: %s\nBuild: %s\nObject: %s" % [
		sim.core_tick(),
		sim.digest(),
		sim.map_tile_count(),
		sim.resource_count(),
		_build_mode_label(),
		selected_building_id,
		object_text,
	]


func _update_build_preview() -> void:
	if build_mode != BuildMode.BUILD or selected_building_id.is_empty():
		build_ghost_root.hide_preview()
		build_preview_valid = false
		return

	var tile_variant: Variant = _mouse_ground_tile()
	if tile_variant == null:
		build_ghost_root.hide_preview()
		build_preview_valid = false
		return

	var tile: Vector2i = tile_variant
	build_preview_tile = tile
	var footprint: Array = sim.building_footprint(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	)
	if footprint.is_empty():
		build_ghost_root.hide_preview()
		build_preview_valid = false
		return

	build_preview_valid = sim.can_place_building(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	) and _footprint_allows_player(selected_building_id, footprint)
	build_ghost_root.show_footprint(footprint, build_preview_valid)


func _try_place_selected_building() -> void:
	if selected_building_id.is_empty() or not build_preview_valid:
		return

	var footprint: Array = sim.building_footprint(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	)
	if not sim.can_place_building(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	) or not _footprint_allows_player(selected_building_id, footprint):
		build_preview_valid = false
		build_ghost_root.show_footprint(footprint, false)
		return

	if sim.place_building(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	):
		_render_buildings_from_sim()
		_update_build_preview()
		_update_status_label()
		_mark_map_snapshot_dirty()
		get_viewport().set_input_as_handled()


func _try_select_building_at_pointer() -> void:
	var tile_variant: Variant = _mouse_ground_tile()
	if tile_variant == null:
		return

	var tile: Vector2i = tile_variant
	var building := _building_at_tile(tile)
	if building.is_empty():
		return

	_select_object(building)
	get_viewport().set_input_as_handled()


func _try_remove_building_at_pointer() -> void:
	var tile_variant: Variant = _mouse_ground_tile()
	if tile_variant == null:
		return

	var tile: Vector2i = tile_variant
	var building := _building_at_tile(tile)
	if building.is_empty():
		return

	var removed_id := int(building["id"])
	if not sim.remove_building(tile.x, tile.y):
		return

	if selected_object_id == removed_id:
		_clear_selected_object()
	_render_buildings_from_sim()
	_update_build_preview()
	_update_status_label()
	_mark_map_snapshot_dirty()


func _mouse_ground_tile() -> Variant:
	var mouse_position := get_viewport().get_mouse_position()
	var ray_origin := camera.project_ray_origin(mouse_position)
	var ray_direction := camera.project_ray_normal(mouse_position)
	if abs(ray_direction.y) < 0.0001:
		return null

	var distance := -ray_origin.y / ray_direction.y
	if distance < 0.0:
		return null

	var hit := ray_origin + ray_direction * distance
	return Vector2i(int(round(hit.x)), int(round(hit.z)))


func _render_buildings_from_sim() -> void:
	BuildingRendererScript.render_from_sim(sim, buildings_root, building_tile_index, blocked_building_tiles)
	if map_overlay_controller != null:
		map_overlay_controller.mark_buildings_dirty()
	_refresh_selected_object_after_render()


func _building_at_tile(tile: Vector2i) -> Dictionary:
	if not building_tile_index.has(tile):
		return {}
	return building_tile_index[tile]


func _building_by_id(id: int) -> Dictionary:
	for building: Dictionary in building_tile_index.values():
		if int(building["id"]) == id:
			return building
	return {}


func _refresh_selected_object_after_render() -> void:
	if selected_object_id == -1:
		return

	var building := _building_by_id(selected_object_id)
	if building.is_empty():
		_clear_selected_object()
		return

	selected_object = building
	_update_selected_machine_window()
	SelectionOutlineScript.sync(selection_outline_root, building["footprint"])


func _enter_neutral_mode() -> void:
	build_mode = BuildMode.NEUTRAL
	build_preview_valid = false
	if build_ghost_root != null:
		build_ghost_root.hide_preview()
	_update_status_label()


func _build_mode_label() -> String:
	return "Build" if build_mode == BuildMode.BUILD else "Neutral"


func _select_object(building: Dictionary) -> void:
	selected_object_id = int(building["id"])
	selected_object = building
	_update_selected_machine_window()
	SelectionOutlineScript.sync(selection_outline_root, building["footprint"])
	_update_status_label()


func _clear_selected_object() -> void:
	selected_object_id = -1
	selected_object.clear()
	machine_window.hide_window()
	SelectionOutlineScript.hide(selection_outline_root)
	_update_status_label()


func _update_selected_machine_window() -> void:
	if selected_object.is_empty():
		machine_window.hide_window()
		return
	if inventory_window != null and inventory_window.is_open():
		machine_window.hide_window()
		return
	var snapshot: Dictionary = sim.building_ui_snapshot(selected_object_id)
	machine_window.update(selected_object, snapshot)


func _on_machine_recipe_selected(building_id: int, recipe_id: String) -> void:
	if building_id != selected_object_id:
		return
	if sim.set_building_recipe(building_id, recipe_id):
		_update_selected_machine_window()


func _footprint_allows_player(def_id: String, footprint: Array) -> bool:
	if BuildingCatalogScript.is_walkable(def_id):
		return true
	return not _footprint_overlaps_player(footprint)


func _footprint_overlaps_player(footprint: Array) -> bool:
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		if _player_overlaps_tile(player.global_position, Vector2i(int(tile["x"]), int(tile["y"]))):
			return true
	return false


func _is_player_position_walkable(position: Vector3) -> bool:
	var min_tile_x := int(ceil(position.x - 0.5 - PLAYER_COLLISION_RADIUS))
	var max_tile_x := int(floor(position.x + 0.5 + PLAYER_COLLISION_RADIUS))
	var min_tile_y := int(ceil(position.z - 0.5 - PLAYER_COLLISION_RADIUS))
	var max_tile_y := int(floor(position.z + 0.5 + PLAYER_COLLISION_RADIUS))

	for tile_x in range(min_tile_x, max_tile_x + 1):
		for tile_y in range(min_tile_y, max_tile_y + 1):
			var tile := Vector2i(tile_x, tile_y)
			if blocked_building_tiles.has(tile) and _player_overlaps_tile(position, tile):
				return false
	return true


func _player_overlaps_tile(position: Vector3, tile: Vector2i) -> bool:
	var closest_x: float = clamp(position.x, float(tile.x) - 0.5, float(tile.x) + 0.5)
	var closest_z: float = clamp(position.z, float(tile.y) - 0.5, float(tile.y) + 0.5)
	var offset := Vector2(position.x - closest_x, position.z - closest_z)
	return offset.length_squared() < PLAYER_COLLISION_RADIUS * PLAYER_COLLISION_RADIUS
