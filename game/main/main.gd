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
const MapOverlayScript := preload("res://game/ui/map_overlay.gd")
const HOTBAR_SELECTOR_OWNER_PREFIX := "hotbar:"

@onready var camera: Camera3D = %Camera3D
@onready var player: PlayerController = %Player
@onready var buildings_root: Node3D = %Buildings
@onready var status_label: Label = %StatusLabel
@onready var hotbar = %Hotbar
@onready var environment: Node3D = $Environment

const CAMERA_MIN_DISTANCE := 10.0
const CAMERA_MAX_DISTANCE := 140.0
const CAMERA_TARGET_HEIGHT := 0.75
const CAMERA_GENERATION_MARGIN_TILES := 16
const CAMERA_FOOTPRINT_FALLBACK_RADIUS := 48
const BUILD_GHOST_Y := 0.075
const PLAYER_COLLISION_RADIUS := 0.32
const PLAYER_ZONE_RADIUS := 12.0
const PLAYER_ZONE_Y := 0.045
const GHOST_VALID_COLOR := Color(0.28, 0.78, 1.0, 0.38)
const GHOST_BLOCKED_COLOR := Color(1.0, 0.22, 0.18, 0.38)

enum BuildMode { NEUTRAL, BUILD }

var sim: NeptuneSim
var camera_yaw := deg_to_rad(42.0)
var camera_elevation := deg_to_rad(58.0)
var camera_distance := 32.0
var build_mode := BuildMode.NEUTRAL
var selected_building_id := ""
var selected_object_id := -1
var selected_object: Dictionary = {}
var build_quarter_turns := 0
var build_preview_tile := Vector2i.ZERO
var build_preview_valid := false
var build_ghost_root: Node3D
var blocked_building_tiles := {}
var building_tile_index := {}
var selection_outline_root: Node3D
var machine_window: MachineWindow
var catalog_selector: Node
var inventory_window: Node
var dev_console: Node
var minimap: Control
var map_overlay: Control
var chunk_size := 32
var visible_chunk_rect_valid := false
var visible_chunk_rect := Rect2i()
var player_zone_overlay: MeshInstance3D
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
	chunk_size = int(sim.chunk_size())
	player.can_move_to = Callable(self, "_is_player_position_walkable")
	_update_camera()
	_sync_world_around_camera(true)
	hotbar.selected.connect(_on_hotbar_selected)
	hotbar.assignment_requested.connect(_on_hotbar_assignment_requested)
	build_ghost_root = Node3D.new()
	build_ghost_root.name = "BuildGhost"
	add_child(build_ghost_root)
	selection_outline_root = Node3D.new()
	selection_outline_root.name = "SelectionOutline"
	selection_outline_root.visible = false
	add_child(selection_outline_root)
	_create_player_zone_overlay()
	machine_window = MachineWindowScene.instantiate() as MachineWindow
	$Hud.add_child(machine_window)
	machine_window.recipe_selected.connect(_on_machine_recipe_selected)
	inventory_window = InventoryWindowScene.instantiate()
	$Hud.add_child(inventory_window)
	inventory_window.slot_transfer_requested.connect(_on_inventory_slot_transfer_requested)
	inventory_window.slot_action_requested.connect(_on_inventory_slot_action_requested)
	dev_console = DevConsoleScene.instantiate()
	$Hud.add_child(dev_console)
	dev_console.command_submitted.connect(_on_dev_console_command_submitted)
	dev_console.set_completions(_dev_console_completions())
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


func _process(_delta: float) -> void:
	if not initialized:
		return

	player.input_blocked = _ui_blocks_gameplay_input()
	if inventory_window != null and inventory_window.is_open():
		_update_inventory_window()
	_update_camera()
	_sync_world_around_camera(false)
	_update_player_zone_overlay()
	_update_build_preview()
	_update_map_overlays()


func _physics_process(_delta: float) -> void:
	player.movement_yaw = camera_yaw


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
				map_overlay.set_fullscreen_open(false)
				_update_map_overlays()
				_update_camera()
				get_viewport().set_input_as_handled()
		elif event is InputEventMouseButton and event.pressed:
			if event.button_index == MOUSE_BUTTON_WHEEL_UP:
				map_overlay.zoom_by(1.25)
				_update_camera()
				get_viewport().set_input_as_handled()
			elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
				map_overlay.zoom_by(0.80)
				_update_camera()
				get_viewport().set_input_as_handled()
		else:
			get_viewport().set_input_as_handled()
		return

	if event is InputEventMouseMotion and Input.is_mouse_button_pressed(MOUSE_BUTTON_MIDDLE):
		camera_yaw -= event.relative.x * 0.006
		camera_elevation = clamp(
			camera_elevation - event.relative.y * 0.006,
			deg_to_rad(18.0),
			deg_to_rad(76.0)
		)
		_update_camera()
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.pressed:
		match event.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				camera_distance = max(CAMERA_MIN_DISTANCE, camera_distance - 0.5)
				_update_camera()
				get_viewport().set_input_as_handled()
			MOUSE_BUTTON_WHEEL_DOWN:
				camera_distance = min(CAMERA_MAX_DISTANCE, camera_distance + 0.5)
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
	if map_overlay != null and map_overlay.detailed_world_visible():
		_update_detailed_map_camera()
		return

	var target: Vector3 = player.global_position + Vector3.UP * CAMERA_TARGET_HEIGHT
	var horizontal_distance := camera_distance * cos(camera_elevation)
	var offset := Vector3(
		horizontal_distance * sin(camera_yaw),
		camera_distance * sin(camera_elevation),
		horizontal_distance * cos(camera_yaw)
	)
	camera.global_position = target + offset
	camera.look_at(target, Vector3.UP)


func _update_detailed_map_camera() -> void:
	var target := Vector3(player.global_position.x, 0.0, player.global_position.z)
	var height: float = clamp(360.0 / maxf(map_overlay.pixels_per_tile(), 1.0), 18.0, 70.0)
	camera.global_position = target + Vector3(0.0, height, 0.02)
	camera.look_at(target, Vector3.FORWARD)


func _sync_world_around_camera(force: bool) -> void:
	var chunk_rect := _visible_chunk_rect_for(_camera_ground_tile_rect(), player.global_position)
	if not force and visible_chunk_rect_valid and visible_chunk_rect == chunk_rect:
		return

	var min_chunk := chunk_rect.position
	var max_chunk := chunk_rect.position + chunk_rect.size - Vector2i.ONE
	sim.ensure_generated_rect(
		min_chunk.x * chunk_size,
		min_chunk.y * chunk_size,
		(max_chunk.x + 1) * chunk_size - 1,
		(max_chunk.y + 1) * chunk_size - 1
	)
	environment.sync_chunks(sim, _chunks_in_rect(min_chunk, max_chunk))
	visible_chunk_rect = chunk_rect
	visible_chunk_rect_valid = true
	_update_status_label()
	_update_map_overlays()


func _create_map_overlays() -> void:
	minimap = MapOverlayScript.new()
	minimap.name = "Minimap"
	minimap.configure_minimap()
	$Hud.add_child(minimap)

	map_overlay = MapOverlayScript.new()
	map_overlay.name = "MapOverlay"
	map_overlay.configure_fullscreen()
	$Hud.add_child(map_overlay)


func _toggle_map_overlay() -> void:
	if map_overlay == null:
		return
	if map_overlay.is_fullscreen_open():
		map_overlay.set_fullscreen_open(false)
	else:
		map_overlay.center_on_player()
		map_overlay.set_fullscreen_open(true)
	_update_map_overlays()
	_update_camera()


func _update_map_overlays() -> void:
	if environment == null or minimap == null or map_overlay == null or sim == null or player == null:
		return
	var tiles: Array = environment.visible_tiles()
	var tile_rect: Rect2i = environment.visible_tile_rect()
	var buildings: Array = sim.buildings()
	minimap.visible = not map_overlay.is_fullscreen_open()
	minimap.set_world_snapshot(tiles, buildings, tile_rect, player.global_position)
	map_overlay.set_world_snapshot(tiles, buildings, tile_rect, player.global_position)


func _visible_chunk_rect_for(tile_rect: Rect2i, player_position: Vector3) -> Rect2i:
	var min_tile := tile_rect.position - Vector2i(CAMERA_GENERATION_MARGIN_TILES, CAMERA_GENERATION_MARGIN_TILES)
	var max_tile := tile_rect.position + tile_rect.size - Vector2i.ONE + Vector2i(CAMERA_GENERATION_MARGIN_TILES, CAMERA_GENERATION_MARGIN_TILES)
	var player_tile := _world_to_tile(player_position)
	min_tile = Vector2i(mini(min_tile.x, player_tile.x), mini(min_tile.y, player_tile.y))
	max_tile = Vector2i(maxi(max_tile.x, player_tile.x), maxi(max_tile.y, player_tile.y))

	var min_chunk := _tile_to_chunk(min_tile)
	var max_chunk := _tile_to_chunk(max_tile)
	return Rect2i(min_chunk, max_chunk - min_chunk + Vector2i.ONE)


func _camera_ground_tile_rect() -> Rect2i:
	var viewport_size := get_viewport().get_visible_rect().size
	if viewport_size.x <= 0.0 or viewport_size.y <= 0.0:
		return _fallback_ground_tile_rect()

	var ground_points: Array[Vector2] = []
	var corners := [
		Vector2.ZERO,
		Vector2(viewport_size.x, 0.0),
		viewport_size,
		Vector2(0.0, viewport_size.y),
	]
	for corner: Vector2 in corners:
		var origin := camera.project_ray_origin(corner)
		var direction := camera.project_ray_normal(corner)
		if absf(direction.y) < 0.0001:
			continue
		var distance := -origin.y / direction.y
		if distance < 0.0:
			continue
		var hit := origin + direction * distance
		ground_points.append(Vector2(hit.x, hit.z))

	if ground_points.is_empty():
		return _fallback_ground_tile_rect()

	var min_x := ground_points[0].x
	var max_x := ground_points[0].x
	var min_y := ground_points[0].y
	var max_y := ground_points[0].y
	for point: Vector2 in ground_points:
		min_x = minf(min_x, point.x)
		max_x = maxf(max_x, point.x)
		min_y = minf(min_y, point.y)
		max_y = maxf(max_y, point.y)

	var min_tile := Vector2i(int(floor(min_x - 0.5)), int(floor(min_y - 0.5)))
	var max_tile := Vector2i(int(ceil(max_x + 0.5)), int(ceil(max_y + 0.5)))
	return Rect2i(min_tile, max_tile - min_tile + Vector2i.ONE)


func _fallback_ground_tile_rect() -> Rect2i:
	var center := _world_to_tile(player.global_position)
	var radius := CAMERA_FOOTPRINT_FALLBACK_RADIUS
	return Rect2i(center - Vector2i(radius, radius), Vector2i(radius * 2 + 1, radius * 2 + 1))


func _world_to_tile(position: Vector3) -> Vector2i:
	return Vector2i(int(round(position.x)), int(round(position.z)))


func _tile_to_chunk(tile: Vector2i) -> Vector2i:
	return Vector2i(
		int(floor(float(tile.x) / float(chunk_size))),
		int(floor(float(tile.y) / float(chunk_size)))
	)


func _chunks_in_rect(min_chunk: Vector2i, max_chunk: Vector2i) -> Array:
	var chunks: Array[Vector2i] = []
	for chunk_y in range(min_chunk.y, max_chunk.y + 1):
		for chunk_x in range(min_chunk.x, max_chunk.x + 1):
			chunks.append(Vector2i(chunk_x, chunk_y))
	return chunks


func _create_player_zone_overlay() -> void:
	player_zone_overlay = MeshInstance3D.new()
	player_zone_overlay.name = "PlayerZoneOverlay"
	player_zone_overlay.mesh = _player_zone_mesh()
	player_zone_overlay.material_override = _transparent_material(Color(0.28, 0.78, 1.0, 0.18))
	add_child(player_zone_overlay)
	_update_player_zone_overlay()


func _update_player_zone_overlay() -> void:
	if player_zone_overlay == null:
		return
	player_zone_overlay.global_position = Vector3(player.global_position.x, PLAYER_ZONE_Y, player.global_position.z)


func _player_zone_mesh() -> ArrayMesh:
	var segments := 96
	var inner_radius := PLAYER_ZONE_RADIUS - 0.08
	var outer_radius := PLAYER_ZONE_RADIUS + 0.08
	var vertices := PackedVector3Array()
	var normals := PackedVector3Array()
	var colors := PackedColorArray()
	var indices := PackedInt32Array()

	for index in range(segments):
		var angle := TAU * float(index) / float(segments)
		var direction := Vector3(cos(angle), 0.0, sin(angle))
		vertices.append(direction * inner_radius)
		vertices.append(direction * outer_radius)
		normals.append(Vector3.UP)
		normals.append(Vector3.UP)
		colors.append(Color(0.28, 0.78, 1.0, 0.10))
		colors.append(Color(0.28, 0.78, 1.0, 0.34))

	for index in range(segments):
		var next_index := (index + 1) % segments
		var inner_a := index * 2
		var outer_a := inner_a + 1
		var inner_b := next_index * 2
		var outer_b := inner_b + 1
		indices.append(inner_a)
		indices.append(outer_a)
		indices.append(outer_b)
		indices.append(inner_a)
		indices.append(outer_b)
		indices.append(inner_b)

	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_COLOR] = colors
	arrays[Mesh.ARRAY_INDEX] = indices

	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	return mesh


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


func _on_dev_console_command_submitted(line: String) -> void:
	var parts := line.split(" ", false)
	if parts.is_empty():
		return

	match str(parts[0]).to_lower():
		"help":
			dev_console.append_lines([
				"commands: help, clear, status, items, give <item> <amount>",
				"F1 toggles console. Up/Down navigate history. Tab completes item ids.",
			])
		"clear":
			dev_console.clear_scrollback()
		"status":
			dev_console.append_output(
				"tick=%d digest=%d buildings=%d selected=%s" % [
					sim.core_tick(),
					sim.digest(),
					sim.building_count(),
					"none" if selected_object.is_empty() else "%s #%d" % [str(selected_object.get("def_id", "")), selected_object_id],
				]
			)
		"items":
			dev_console.append_output("items: %s" % "  ".join(_item_ids()))
		"give":
			_execute_give_command(parts)
		_:
			dev_console.append_output("Unknown command '%s'. Use 'help' for commands." % str(parts[0]))


func _execute_give_command(parts: PackedStringArray) -> void:
	if parts.size() < 2:
		dev_console.append_output("Usage: give <item> <amount>")
		return
	var item_id := str(parts[1])
	var amount := 1
	if parts.size() >= 3:
		amount = max(1, int(parts[2]))
	if sim.give_item(item_id, amount):
		dev_console.append_output("Added %s x%d" % [item_id, amount])
		if inventory_window != null and inventory_window.is_open():
			_update_inventory_window()
		return
	dev_console.append_output("Could not add %s x%d" % [item_id, amount])


func _dev_console_completions() -> Array:
	var completions: Array = ["help", "clear", "status", "items", "give"]
	for item_id: String in _item_ids():
		completions.append(item_id)
	return completions


func _item_ids() -> Array[String]:
	var ids: Array[String] = []
	for raw_definition: Variant in ItemCatalogScript.definitions():
		var definition: Dictionary = raw_definition
		var id := str(definition.get("id", ""))
		if not id.is_empty():
			ids.append(id)
	return ids


func _toggle_inventory_window() -> void:
	if inventory_window.is_open():
		inventory_window.hide_window()
		return
	machine_window.hide_window()
	_update_inventory_window()
	inventory_window.show_inventory(
		sim.inventory_snapshot(),
		selected_object,
		_selected_object_ui_snapshot()
	)


func _update_inventory_window() -> void:
	if inventory_window == null:
		return
	inventory_window.update_inventory(
		sim.inventory_snapshot(),
		selected_object,
		_selected_object_ui_snapshot()
	)


func _on_inventory_slot_transfer_requested(from_ref: Dictionary, to_ref: Dictionary, amount: int) -> void:
	if sim.transfer_inventory_slot(from_ref, to_ref, amount):
		_update_inventory_window()


func _on_inventory_slot_action_requested(slot_ref: Dictionary, action: String) -> void:
	if sim.click_inventory_slot(slot_ref, action):
		_update_inventory_window()


func _selected_object_ui_snapshot() -> Dictionary:
	if selected_object.is_empty():
		return {}
	return sim.building_ui_snapshot(selected_object_id)


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
		build_ghost_root.visible = false
		build_preview_valid = false
		return

	var tile_variant: Variant = _mouse_ground_tile()
	if tile_variant == null:
		build_ghost_root.visible = false
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
		build_ghost_root.visible = false
		build_preview_valid = false
		return

	build_preview_valid = sim.can_place_building(
		selected_building_id,
		build_preview_tile.x,
		build_preview_tile.y,
		build_quarter_turns
	) and _footprint_allows_player(selected_building_id, footprint)
	_sync_ghost_tiles(footprint, GHOST_VALID_COLOR if build_preview_valid else GHOST_BLOCKED_COLOR)
	build_ghost_root.visible = true


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
		_sync_ghost_tiles(footprint, GHOST_BLOCKED_COLOR)
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
		_update_map_overlays()
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
	_update_map_overlays()


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


func _sync_ghost_tiles(footprint: Array, color: Color) -> void:
	_clear_children(build_ghost_root)
	for raw_tile: Variant in footprint:
		var tile: Dictionary = raw_tile
		var instance := MeshInstance3D.new()
		var mesh := PlaneMesh.new()
		mesh.size = Vector2(0.96, 0.96)
		instance.mesh = mesh
		instance.position = Vector3(float(tile["x"]), BUILD_GHOST_Y, float(tile["y"]))
		instance.material_override = _transparent_material(color)
		build_ghost_root.add_child(instance)


func _render_buildings_from_sim() -> void:
	BuildingRendererScript.render_from_sim(sim, buildings_root, building_tile_index, blocked_building_tiles)
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
		build_ghost_root.visible = false
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


func _transparent_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	return material


func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
