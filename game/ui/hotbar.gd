extends Control

signal selected(entry_id: String)
signal assignment_requested(slot_index: int)

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const BuildingIconRendererScript := preload("res://game/ui/building_icon_renderer.gd")
const HOTBAR_SLOT_COUNT := 10
const SLOT_SIZE := Vector2(78.0, 78.0)
const SLOT_GAP := 8.0
const FRAME_PADDING := 10.0
const BOTTOM_MARGIN := 12.0

const PANEL_BG := Color(0.055, 0.065, 0.070, 0.88)
const SLOT_BG := Color(0.055, 0.065, 0.070, 0.88)
const SELECTED_SLOT_BG := Color(0.140, 0.180, 0.160, 0.94)
const PANEL_BORDER := Color(0.450, 0.480, 0.430, 0.55)
const ACCENT_COLOR := Color(0.560, 0.760, 0.420, 1.0)
const MUTED_TEXT := Color(0.620, 0.670, 0.620, 1.0)

const DEFAULT_ENTRIES := [
	{"id": "basic_miner", "label": "Basic miner"},
	{"id": "wooden_chest", "label": "Wooden chest"},
	{"id": "basic_belt", "label": "Basic belt"},
	{"id": "stone_furnace", "label": "Stone furnace"},
	{"id": "basic_inserter", "label": "Basic inserter"},
	{"id": "basic_assembler", "label": "Basic assembler"},
	{"id": "accelerated_belt", "label": "Accelerated belt"},
	{"id": "fast_belt", "label": "Fast belt"},
	{"id": "basic_splitter", "label": "Basic splitter"},
	{"id": "basic_underground_belt", "label": "Underground belt"},
]

var selected_slot := 0
var assigning_slot := -1
var slots: Array[Dictionary] = []
var _slot_buttons: Array[Button] = []
var _icon_rects: Array[TextureRect] = []
var _icon_renderer: Node


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	anchors_preset = Control.PRESET_FULL_RECT
	_hydrate_slots()
	_build_ui()
	_select_slot(0)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		var index := _hotbar_index_for_key(event.keycode)
		if index >= 0 and assigning_slot == -1:
			_select_slot(index)
			get_viewport().set_input_as_handled()


func selected_entry_id() -> String:
	if selected_slot < 0 or selected_slot >= slots.size():
		return ""
	return slots[selected_slot].get("id", "")


func selected_entry_label() -> String:
	if selected_slot < 0 or selected_slot >= slots.size():
		return ""
	return slots[selected_slot].get("label", "")


func _hydrate_slots() -> void:
	slots.clear()
	for index in HOTBAR_SLOT_COUNT:
		if index < DEFAULT_ENTRIES.size():
			var entry: Dictionary = DEFAULT_ENTRIES[index].duplicate()
			var id := str(entry.get("id", ""))
			if not id.is_empty():
				entry["label"] = BuildingCatalogScript.display_name(id)
			slots.append(entry)
		else:
			slots.append({})


func _build_ui() -> void:
	_icon_renderer = BuildingIconRendererScript.new()
	_icon_renderer.name = "BuildingIconRenderer"
	add_child(_icon_renderer)
	_icon_renderer.prepare_icons(_slot_entry_ids())

	var frame_size := Vector2(
		HOTBAR_SLOT_COUNT * SLOT_SIZE.x + (HOTBAR_SLOT_COUNT - 1) * SLOT_GAP + FRAME_PADDING * 2.0,
		SLOT_SIZE.y + FRAME_PADDING * 2.0
	)

	var panel := PanelContainer.new()
	panel.name = "GameplayHotbar"
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	panel.custom_minimum_size = frame_size
	panel.anchor_left = 0.5
	panel.anchor_right = 0.5
	panel.anchor_top = 1.0
	panel.anchor_bottom = 1.0
	panel.offset_left = -frame_size.x * 0.5
	panel.offset_right = frame_size.x * 0.5
	panel.offset_top = -frame_size.y - BOTTOM_MARGIN
	panel.offset_bottom = -BOTTOM_MARGIN
	panel.add_theme_stylebox_override("panel", _stylebox(PANEL_BG, PANEL_BORDER, 1, 0))
	add_child(panel)

	var margin := MarginContainer.new()
	margin.name = "Padding"
	margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
	margin.add_theme_constant_override("margin_left", 10)
	margin.add_theme_constant_override("margin_top", 10)
	margin.add_theme_constant_override("margin_right", 10)
	margin.add_theme_constant_override("margin_bottom", 10)
	panel.add_child(margin)

	var row := HBoxContainer.new()
	row.name = "Slots"
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_theme_constant_override("separation", int(SLOT_GAP))
	margin.add_child(row)

	for index in HOTBAR_SLOT_COUNT:
		_add_slot(row, index)


func _add_slot(parent: Control, index: int) -> void:
	var button := Button.new()
	button.name = "Slot%d" % (index + 1)
	button.custom_minimum_size = SLOT_SIZE
	button.focus_mode = Control.FOCUS_NONE
	button.tooltip_text = _slot_tooltip(index)
	button.text = ""
	button.mouse_filter = Control.MOUSE_FILTER_STOP
	button.pressed.connect(_on_slot_pressed.bind(index))
	button.gui_input.connect(_on_slot_gui_input.bind(index))
	parent.add_child(button)
	_slot_buttons.append(button)

	var icon := TextureRect.new()
	icon.name = "Icon"
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	icon.texture = _slot_texture(index)
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	icon.offset_left = 3.0
	icon.offset_top = 3.0
	icon.offset_right = -3.0
	icon.offset_bottom = -3.0
	button.add_child(icon)
	_icon_rects.append(icon)

	var hint := Label.new()
	hint.name = "KeyHint"
	hint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	hint.text = _slot_key_hint(index)
	hint.position = Vector2(4.0, 2.0)
	hint.add_theme_font_size_override("font_size", 13)
	hint.add_theme_color_override("font_color", MUTED_TEXT)
	button.add_child(hint)


func _on_slot_pressed(index: int) -> void:
	if Input.is_key_pressed(KEY_SHIFT):
		assigning_slot = index
		selected_slot = index
		_refresh_visuals()
		assignment_requested.emit(index)
		return

	_select_slot(index)


func _on_slot_gui_input(event: InputEvent, index: int) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_RIGHT:
		assigning_slot = index
		selected_slot = index
		_refresh_visuals()
		assignment_requested.emit(index)
		accept_event()


func _select_slot(index: int) -> void:
	if index < 0 or index >= HOTBAR_SLOT_COUNT:
		return

	selected_slot = index
	assigning_slot = -1
	_refresh_visuals()
	selected.emit(selected_entry_id())


func _refresh_visuals() -> void:
	for index in _slot_buttons.size():
		var selected := index == selected_slot
		var assigning := index == assigning_slot
		var border := ACCENT_COLOR if selected else PANEL_BORDER
		var border_width := 2 if selected else 1
		_slot_buttons[index].add_theme_stylebox_override(
			"normal",
			_stylebox(SELECTED_SLOT_BG if selected else SLOT_BG, border, border_width, 0)
		)
		_slot_buttons[index].add_theme_stylebox_override(
			"hover",
			_stylebox(Color(0.105, 0.125, 0.120, 0.95), border, border_width, 0)
		)
		_slot_buttons[index].add_theme_stylebox_override(
			"pressed",
			_stylebox(SELECTED_SLOT_BG, ACCENT_COLOR, 2, 0)
		)
		_icon_rects[index].modulate = Color(1.0, 1.0, 1.0, 0.42) if assigning else Color.WHITE


func _slot_texture(index: int) -> Texture2D:
	if _icon_renderer == null:
		return null
	var id := str(slots[index].get("id", ""))
	if id.is_empty():
		return null
	return _icon_renderer.texture_for(id)


func _slot_entry_ids() -> Array:
	var ids: Array = []
	for entry: Dictionary in slots:
		var id := str(entry.get("id", ""))
		if not id.is_empty() and not ids.has(id):
			ids.append(id)
	return ids


func _stylebox(bg: Color, border: Color, border_width: int, corner_radius: int) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = bg
	style.border_color = border
	style.border_width_left = border_width
	style.border_width_top = border_width
	style.border_width_right = border_width
	style.border_width_bottom = border_width
	style.corner_radius_top_left = corner_radius
	style.corner_radius_top_right = corner_radius
	style.corner_radius_bottom_left = corner_radius
	style.corner_radius_bottom_right = corner_radius
	return style


func _slot_tooltip(index: int) -> String:
	var label: String = slots[index].get("label", "Empty")
	return "%s [%s]" % [label, _slot_key_hint(index)]


func _slot_key_hint(index: int) -> String:
	return "0" if index == 9 else str(index + 1)


func _hotbar_index_for_key(keycode: int) -> int:
	match keycode:
		KEY_1:
			return 0
		KEY_2:
			return 1
		KEY_3:
			return 2
		KEY_4:
			return 3
		KEY_5:
			return 4
		KEY_6:
			return 5
		KEY_7:
			return 6
		KEY_8:
			return 7
		KEY_9:
			return 8
		KEY_0:
			return 9
		_:
			return -1
