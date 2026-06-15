extends Control
class_name InventoryWindow

signal closed
signal slot_transfer_requested(from_ref: Dictionary, to_ref: Dictionary, amount: int)
signal slot_action_requested(slot_ref: Dictionary, action: String)

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const ItemIconRendererScript := preload("res://game/ui/item_icon_renderer.gd")
const InventorySlotScene := preload("res://game/ui/inventory_slot.tscn")

const PANEL_BG := Color(0.070, 0.075, 0.065, 0.96)
const PANEL_INNER_BG := Color(0.050, 0.055, 0.050, 0.92)
const PANEL_BORDER := Color(0.560, 0.760, 0.420, 0.72)
const GRID_BG := Color(0.035, 0.040, 0.036, 0.70)
const SLOT_BG := Color(0.105, 0.112, 0.098, 0.98)
const SLOT_BORDER := Color(0.310, 0.350, 0.265, 0.95)
const TEXT := Color(0.820, 0.840, 0.780)
const MUTED_TEXT := Color(0.620, 0.670, 0.620, 1.0)
const TITLE := Color(0.920, 0.940, 0.860)
const SLOT_SIZE := Vector2(38.0, 38.0)
const SLOT_GAP := 4
const CHARACTER_COLUMNS := 7
const OBJECT_COLUMNS := 5
const PLAYER_PANEL_WIDTH := 610.0
const OBJECT_PANEL_WIDTH := 270.0

const EQUIPMENT_SLOTS := [
	{"id": "legs", "label": "Legs"},
	{"id": "waist", "label": "Waist"},
	{"id": "back", "label": "Back"},
]

const FALLBACK_SECTIONS := [
	{"id": "tool_belt", "name": "Tool belt", "slots": 6},
	{"id": "left_pocket", "name": "Left pocket", "slots": 4},
	{"id": "right_pocket", "name": "Right pocket", "slots": 4},
	{"id": "backpack_main", "name": "Backpack", "slots": 28},
]

const OBJECT_ROLE_ORDER := ["Input", "Fuel", "Output", "Storage", "Hand"]

@onready var player_panel = %PlayerPanel
@onready var status_label: Label = %StatusLabel
@onready var equipment_list: VBoxContainer = %EquipmentList
@onready var sections_container: VBoxContainer = %Sections
@onready var object_panel = %ObjectPanel
@onready var object_title: Label = %ObjectTitle
@onready var object_content: VBoxContainer = %ObjectContent
@onready var cursor_stack: PanelContainer = %CursorStack
@onready var cursor_icon: TextureRect = %CursorIcon
@onready var cursor_amount: Label = %CursorAmount

var _item_icon_renderer: Node


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	anchors_preset = Control.PRESET_FULL_RECT
	player_panel.add_theme_stylebox_override("panel", _stylebox(PANEL_BG, PANEL_BORDER, 1, 0))
	object_panel.add_theme_stylebox_override("panel", _stylebox(PANEL_BG, PANEL_BORDER, 1, 0))
	cursor_stack.add_theme_stylebox_override("panel", _stylebox(SLOT_BG, Color(0.95, 0.84, 0.55, 0.95), 1, 0))
	player_panel.close_requested.connect(hide_window)
	object_panel.close_requested.connect(hide_window)
	_item_icon_renderer = ItemIconRendererScript.new()
	_item_icon_renderer.name = "ItemIconRenderer"
	add_child(_item_icon_renderer)
	if not Engine.is_editor_hint():
		visible = false
		cursor_stack.visible = false


func _process(_delta: float) -> void:
	if cursor_stack.visible:
		cursor_stack.global_position = get_viewport().get_mouse_position() + Vector2(12.0, 12.0)


func show_inventory(snapshot: Dictionary, selected_building: Dictionary = {}, object_snapshot: Dictionary = {}) -> void:
	visible = true
	update_inventory(snapshot, selected_building, object_snapshot)


func update_inventory(snapshot: Dictionary, selected_building: Dictionary = {}, object_snapshot: Dictionary = {}) -> void:
	if not visible:
		return
	_prepare_icons(snapshot, object_snapshot)
	_rebuild_character(snapshot)
	_rebuild_object(selected_building, object_snapshot)
	_update_cursor(snapshot)


func hide_window() -> void:
	if not visible:
		return
	visible = false
	cursor_stack.visible = false
	closed.emit()


func toggle(snapshot: Dictionary, selected_building: Dictionary = {}, object_snapshot: Dictionary = {}) -> void:
	if visible:
		hide_window()
	else:
		show_inventory(snapshot, selected_building, object_snapshot)


func is_open() -> bool:
	return visible


func _rebuild_character(snapshot: Dictionary) -> void:
	_clear_children(equipment_list)
	_clear_children(sections_container)
	_add_equipment_rows(snapshot)

	var sections := _sections_for_snapshot(snapshot)
	var total_used := 0
	var total_slots := 0
	for section: Dictionary in sections:
		total_used += int(section.get("used_slots", 0))
		total_slots += int(section.get("total_slots", 0))
	status_label.text = "Status: ready  Slots: %d/%d" % [total_used, total_slots]

	for section: Dictionary in sections:
		_add_container_section(section)


func _add_equipment_rows(snapshot: Dictionary) -> void:
	var title := Label.new()
	title.text = "Equipment"
	title.add_theme_font_size_override("font_size", 15)
	title.add_theme_color_override("font_color", TITLE)
	equipment_list.add_child(title)

	var equipment := _equipment_map(snapshot)
	for raw_slot: Dictionary in EQUIPMENT_SLOTS:
		var slot_id := str(raw_slot["id"])
		var item := str(equipment.get(slot_id, ""))
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 6)
		equipment_list.add_child(row)

		var slot_label := Label.new()
		slot_label.custom_minimum_size = Vector2(48.0, SLOT_SIZE.y)
		slot_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		slot_label.add_theme_font_size_override("font_size", 11)
		slot_label.add_theme_color_override("font_color", TEXT)
		slot_label.text = str(raw_slot["label"])
		row.add_child(slot_label)

		row.add_child(_slot_view({"item": item, "amount": 1 if not item.is_empty() else 0}))

		var item_label := Label.new()
		item_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		item_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		item_label.add_theme_font_size_override("font_size", 11)
		item_label.add_theme_color_override("font_color", TEXT)
		item_label.text = ItemCatalogScript.display_name(item) if not item.is_empty() else "empty"
		row.add_child(item_label)


func _add_container_section(section: Dictionary) -> void:
	var section_box := VBoxContainer.new()
	section_box.add_theme_constant_override("separation", 4)
	sections_container.add_child(section_box)

	var title := Label.new()
	title.text = str(section.get("name", "Container"))
	title.add_theme_font_size_override("font_size", 15)
	title.add_theme_color_override("font_color", TITLE)
	section_box.add_child(title)

	var capacity := Label.new()
	capacity.text = _capacity_text(section)
	capacity.add_theme_font_size_override("font_size", 10)
	capacity.add_theme_color_override("font_color", MUTED_TEXT)
	section_box.add_child(capacity)

	var grid_panel := PanelContainer.new()
	grid_panel.add_theme_stylebox_override("panel", _stylebox(GRID_BG, Color.TRANSPARENT, 0, 0))
	section_box.add_child(grid_panel)

	var grid := GridContainer.new()
	grid.columns = CHARACTER_COLUMNS
	grid.add_theme_constant_override("h_separation", SLOT_GAP)
	grid.add_theme_constant_override("v_separation", SLOT_GAP)
	grid_panel.add_child(grid)

	var slots: Array = section.get("slots", [])
	for index in range(slots.size()):
		var slot: Dictionary = slots[index]
		grid.add_child(_slot_view(slot, _slot_ref_for_section(section, index)))


func _rebuild_object(selected_building: Dictionary, object_snapshot: Dictionary) -> void:
	_clear_children(object_content)
	var inventories: Array = object_snapshot.get("inventories", [])
	object_panel.visible = not selected_building.is_empty() and not inventories.is_empty()
	if not object_panel.visible:
		return

	var def_id := str(selected_building.get("def_id", ""))
	object_title.text = BuildingCatalogScript.display_name(def_id)
	for role: String in OBJECT_ROLE_ORDER:
		var slots := _slots_for_role(role, inventories)
		if slots.is_empty():
			continue
		_add_object_role_section(int(selected_building.get("id", -1)), role, slots)


func _add_object_role_section(building_id: int, role: String, slots: Array) -> void:
	var section_box := VBoxContainer.new()
	section_box.add_theme_constant_override("separation", 4)
	object_content.add_child(section_box)

	var title := Label.new()
	title.text = role
	title.add_theme_font_size_override("font_size", 15)
	title.add_theme_color_override("font_color", TITLE)
	section_box.add_child(title)

	var grid_panel := PanelContainer.new()
	grid_panel.add_theme_stylebox_override("panel", _stylebox(GRID_BG, Color.TRANSPARENT, 0, 0))
	section_box.add_child(grid_panel)

	var grid := GridContainer.new()
	grid.columns = OBJECT_COLUMNS
	grid.add_theme_constant_override("h_separation", SLOT_GAP)
	grid.add_theme_constant_override("v_separation", SLOT_GAP)
	grid_panel.add_child(grid)

	for index in range(slots.size()):
		var slot: Dictionary = slots[index]
		grid.add_child(_slot_view(slot, {
			"kind": "building",
			"building_id": building_id,
			"role": role,
			"slot": index,
		}))


func _slot_view(slot: Dictionary, slot_ref: Dictionary = {}) -> Control:
	var panel_slot := InventorySlotScene.instantiate() as InventorySlot
	panel_slot.custom_minimum_size = SLOT_SIZE

	var item := str(slot.get("item", ""))
	var amount := int(slot.get("amount", 0))
	var texture: Texture2D = null
	if not item.is_empty() and _item_icon_renderer != null:
		texture = _item_icon_renderer.texture_for(item)
	panel_slot.configure(slot_ref, item, amount, texture)
	panel_slot.transfer_requested.connect(_on_slot_transfer_requested)
	panel_slot.action_requested.connect(_on_slot_action_requested)

	return panel_slot


func _on_slot_transfer_requested(from_ref: Dictionary, to_ref: Dictionary, amount: int) -> void:
	slot_transfer_requested.emit(from_ref, to_ref, amount)


func _on_slot_action_requested(slot_ref: Dictionary, action: String) -> void:
	slot_action_requested.emit(slot_ref, action)


func _update_cursor(snapshot: Dictionary) -> void:
	var cursor: Dictionary = snapshot.get("cursor", {})
	var item := str(cursor.get("item", ""))
	var amount := int(cursor.get("amount", 0))
	cursor_stack.visible = not item.is_empty() and amount > 0
	if not cursor_stack.visible:
		return
	cursor_icon.texture = _item_icon_renderer.texture_for(item) if _item_icon_renderer != null else null
	cursor_amount.text = str(amount) if amount > 1 else ""


func _sections_for_snapshot(snapshot: Dictionary) -> Array[Dictionary]:
	var sections: Array = snapshot.get("sections", [])
	if not sections.is_empty():
		var normalized: Array[Dictionary] = []
		for raw_section: Variant in sections:
			if raw_section is Dictionary:
				normalized.append(raw_section)
		return normalized
	return _fallback_sections(snapshot.get("player_slots", []))


func _fallback_sections(player_slots: Array) -> Array[Dictionary]:
	var sections: Array[Dictionary] = []
	var offset := 0
	for raw_section: Dictionary in FALLBACK_SECTIONS:
		var slot_count := int(raw_section["slots"])
		sections.append(_section_from_player_slots(raw_section, player_slots, offset, slot_count))
		offset += slot_count
	if player_slots.size() > offset:
		sections.append(_section_from_player_slots(
			{"id": "inventory", "name": "Inventory"},
			player_slots,
			offset,
			player_slots.size() - offset
		))
	return sections


func _section_from_player_slots(section_def: Dictionary, player_slots: Array, offset: int, slot_count: int) -> Dictionary:
	var slots: Array = []
	var used_slots := 0
	for index in range(slot_count):
		var source_index := offset + index
		var slot := {"item": "", "amount": 0}
		if source_index < player_slots.size() and player_slots[source_index] is Dictionary:
			slot = player_slots[source_index]
		if not str(slot.get("item", "")).is_empty():
			used_slots += 1
		slots.append(slot)
	return {
		"id": str(section_def.get("id", "")),
		"name": str(section_def.get("name", "Inventory")),
		"source_kind": "player",
		"source_offset": offset,
		"slots": slots,
		"used_slots": used_slots,
		"total_slots": slot_count,
		"total_weight_grams": 0,
		"max_weight_grams": 0,
		"total_bulk_units": 0,
		"max_bulk_units": 0,
	}


func _slot_ref_for_section(section: Dictionary, slot_index: int) -> Dictionary:
	if str(section.get("source_kind", "")) == "player":
		return {
			"kind": "player",
			"slot": int(section.get("source_offset", 0)) + slot_index,
		}
	return {
		"kind": "character",
		"container": str(section.get("id", "")),
		"slot": slot_index,
	}


func _equipment_map(snapshot: Dictionary) -> Dictionary:
	var result := {}
	for raw_entry: Variant in snapshot.get("equipment", []):
		if not raw_entry is Dictionary:
			continue
		var entry: Dictionary = raw_entry
		result[str(entry.get("slot", ""))] = str(entry.get("item", ""))
	return result


func _slots_for_role(role: String, inventories: Array) -> Array:
	for raw_inventory: Variant in inventories:
		if not raw_inventory is Dictionary:
			continue
		var inventory: Dictionary = raw_inventory
		if str(inventory.get("role", "")) == role:
			return inventory.get("slots", [])
	return []


func _capacity_text(section: Dictionary) -> String:
	var slots_text := "Slots: %d/%d" % [
		int(section.get("used_slots", 0)),
		int(section.get("total_slots", 0)),
	]
	var bulk_text := _limit_text("Bulk", int(section.get("total_bulk_units", 0)), int(section.get("max_bulk_units", 0)))
	var weight_text := _weight_text(int(section.get("total_weight_grams", 0)), int(section.get("max_weight_grams", 0)))
	return "%s  %s  %s" % [slots_text, bulk_text, weight_text]


func _limit_text(label: String, value: int, max_value: int) -> String:
	if max_value <= 0:
		return "%s: %d" % [label, value]
	return "%s: %d/%d" % [label, value, max_value]


func _weight_text(value_grams: int, max_grams: int) -> String:
	var value := float(value_grams) / 1000.0
	if max_grams <= 0:
		return "Weight: %.1f kg" % value
	return "Weight: %.1f/%.1f kg" % [value, float(max_grams) / 1000.0]


func _prepare_icons(snapshot: Dictionary, object_snapshot: Dictionary) -> void:
	if _item_icon_renderer == null:
		return
	var ids: Array = []
	for raw_slot: Variant in snapshot.get("player_slots", []):
		if raw_slot is Dictionary:
			_add_item_id(ids, str(raw_slot.get("item", "")))
	for raw_section: Variant in snapshot.get("sections", []):
		if not raw_section is Dictionary:
			continue
		for raw_slot: Variant in raw_section.get("slots", []):
			if raw_slot is Dictionary:
				_add_item_id(ids, str(raw_slot.get("item", "")))
	for raw_entry: Variant in snapshot.get("equipment", []):
		if raw_entry is Dictionary:
			_add_item_id(ids, str(raw_entry.get("item", "")))
	var cursor: Dictionary = snapshot.get("cursor", {})
	_add_item_id(ids, str(cursor.get("item", "")))
	for raw_inventory: Variant in object_snapshot.get("inventories", []):
		if not raw_inventory is Dictionary:
			continue
		for raw_slot: Variant in raw_inventory.get("slots", []):
			if raw_slot is Dictionary:
				_add_item_id(ids, str(raw_slot.get("item", "")))
	_item_icon_renderer.prepare_icons(ids)


func _add_item_id(ids: Array, item_id: String) -> void:
	if not item_id.is_empty() and not ids.has(item_id):
		ids.append(item_id)


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


func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
