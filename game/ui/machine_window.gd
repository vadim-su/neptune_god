extends PanelContainer
class_name MachineWindow

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")

signal recipe_selected(building_id: int, recipe_id: String)

const PANEL_BG := Color(0.070, 0.075, 0.065, 0.96)
const PANEL_BORDER := Color(0.560, 0.760, 0.420, 0.72)
const SLOT_BG := Color(0.105, 0.112, 0.098, 0.98)
const SLOT_BORDER := Color(0.310, 0.350, 0.265, 0.95)
const ACTIVE_RECIPE_BG := Color(0.160, 0.220, 0.140, 0.96)
const TEXT := Color(0.820, 0.840, 0.780)
const TITLE := Color(0.920, 0.940, 0.860)
const BAR_BG := Color(0.050, 0.055, 0.050, 0.96)
const PROCESS_FILL := Color(0.780, 0.640, 0.250, 1.0)
const FUEL_FILL := Color(0.900, 0.330, 0.160, 1.0)
const WINDOW_COMPACT_HEIGHT := 320.0
const WINDOW_RECIPE_HEIGHT := 430.0

@onready var window_title: Label = %WindowTitle
@onready var header: Control = %Header
@onready var close_button: Button = %CloseButton
@onready var subtitle: Label = %Subtitle
@onready var content: VBoxContainer = %Content

var selected_building_id := -1
var _dragging := false
var _drag_offset := Vector2.ZERO


func _ready() -> void:
	add_theme_stylebox_override("panel", _panel_stylebox())
	header.gui_input.connect(_on_header_gui_input)
	close_button.pressed.connect(hide_window)
	visible = false


func _input(event: InputEvent) -> void:
	if not _dragging:
		return

	if event is InputEventMouseMotion:
		global_position = get_viewport().get_mouse_position() - _drag_offset
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and not event.pressed:
		_dragging = false
		get_viewport().set_input_as_handled()


func update(building: Dictionary, snapshot: Dictionary) -> void:
	if building.is_empty() or snapshot.is_empty() or str(snapshot.get("ui_kind", "")) != "machine":
		hide_window()
		return

	selected_building_id = int(building["id"])
	visible = true
	offset_bottom = offset_top + (
		WINDOW_RECIPE_HEIGHT if bool(snapshot.get("recipe_grid_visible", false)) else WINDOW_COMPACT_HEIGHT
	)
	window_title.text = BuildingCatalogScript.display_name(str(building["def_id"]))
	subtitle.text = _subtitle_text(snapshot)
	_rebuild_content(snapshot)


func hide_window() -> void:
	selected_building_id = -1
	_dragging = false
	visible = false


func _on_header_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		_dragging = true
		_drag_offset = get_viewport().get_mouse_position() - global_position
		get_viewport().set_input_as_handled()


func _rebuild_content(snapshot: Dictionary) -> void:
	_clear_children(content)

	var recipe_grid_visible := bool(snapshot.get("recipe_grid_visible", false))
	var active_recipe := str(snapshot.get("active_recipe", ""))
	var recipes: Array = snapshot.get("recipes", [])
	if bool(snapshot.get("recipe_selector_visible", false)):
		_add_recipe_controls(recipe_grid_visible, active_recipe, recipes)
	if recipe_grid_visible:
		_add_recipe_grid(content, active_recipe, recipes)
		return

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)
	row.alignment = BoxContainer.ALIGNMENT_BEGIN
	content.add_child(row)

	_add_inventory_group(row, "Input", snapshot)
	_add_progress_bar(row, "Process", float(snapshot.get("process_progress", 0.0)), PROCESS_FILL, true)
	_add_inventory_group(row, "Output", snapshot)

	var fuel_row := HBoxContainer.new()
	fuel_row.add_theme_constant_override("separation", 12)
	content.add_child(fuel_row)
	_add_inventory_group(fuel_row, "Fuel", snapshot)
	_add_progress_bar(fuel_row, "Fuel", float(snapshot.get("fuel_progress", 0.0)), FUEL_FILL, false)


func _add_recipe_controls(recipe_grid_visible: bool, active_recipe: String, recipes: Array) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)
	row.alignment = BoxContainer.ALIGNMENT_BEGIN
	content.add_child(row)

	var heading := Label.new()
	heading.text = "Recipes"
	heading.add_theme_font_size_override("font_size", 14)
	heading.add_theme_color_override("font_color", TITLE)
	row.add_child(heading)

	if not recipe_grid_visible:
		var button := Button.new()
		button.text = _active_recipe_label(active_recipe, recipes)
		button.custom_minimum_size = Vector2(118, 28)
		button.pressed.connect(func() -> void:
			recipe_selected.emit(selected_building_id, "")
		)
		row.add_child(button)


func _add_recipe_grid(parent: Control, active_recipe: String, recipes: Array) -> void:
	var grid := GridContainer.new()
	grid.columns = clamp(recipes.size(), 2, 3)
	grid.custom_minimum_size = Vector2(104 * grid.columns + 6 * (grid.columns - 1), 40)
	grid.add_theme_constant_override("h_separation", 6)
	grid.add_theme_constant_override("v_separation", 6)
	parent.add_child(grid)

	for raw_recipe: Variant in recipes:
		var recipe: Dictionary = raw_recipe
		var button := Button.new()
		button.custom_minimum_size = Vector2(104, 40)
		button.text = _recipe_card_text(recipe)
		if str(recipe.get("id", "")) == active_recipe:
			button.add_theme_stylebox_override("normal", _flat_stylebox(ACTIVE_RECIPE_BG, SLOT_BORDER))
		var recipe_id := str(recipe.get("id", ""))
		button.pressed.connect(func() -> void:
			recipe_selected.emit(selected_building_id, recipe_id)
		)
		grid.add_child(button)


func _add_inventory_group(parent: Control, role: String, snapshot: Dictionary) -> void:
	var slots := _slots_for_role(role, snapshot)
	if slots.is_empty():
		return

	var group := HBoxContainer.new()
	group.add_theme_constant_override("separation", 4)
	parent.add_child(group)

	for raw_slot: Variant in slots:
		var slot: Dictionary = raw_slot
		group.add_child(_slot_view(slot))


func _slot_view(slot: Dictionary) -> Control:
	var panel_slot := PanelContainer.new()
	panel_slot.custom_minimum_size = Vector2(34, 34)
	panel_slot.add_theme_stylebox_override("panel", _flat_stylebox(SLOT_BG, SLOT_BORDER))

	var label := Label.new()
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.add_theme_font_size_override("font_size", 10)
	label.add_theme_color_override("font_color", TEXT)
	var item := str(slot.get("item", ""))
	var amount := int(slot.get("amount", 0))
	label.text = "" if item.is_empty() else "%s\n%d" % [_short_item_label(item), amount]
	panel_slot.add_child(label)
	return panel_slot


func _add_progress_bar(parent: Control, label_text: String, value: float, fill_color: Color, show_percent: bool) -> void:
	var box := VBoxContainer.new()
	box.custom_minimum_size = Vector2(220, 34)
	box.add_theme_constant_override("separation", 3)
	parent.add_child(box)

	var label := Label.new()
	label.text = "%s %d%%" % [label_text, int(round(clamp(value, 0.0, 1.0) * 100.0))] if show_percent else label_text
	label.add_theme_font_size_override("font_size", 12)
	label.add_theme_color_override("font_color", TEXT)
	box.add_child(label)

	var track := PanelContainer.new()
	track.custom_minimum_size = Vector2(220, 12)
	track.add_theme_stylebox_override("panel", _flat_stylebox(BAR_BG, SLOT_BORDER))
	box.add_child(track)

	var fill := ColorRect.new()
	fill.color = fill_color
	fill.anchor_left = 0.0
	fill.anchor_right = clamp(value, 0.0, 1.0)
	fill.anchor_top = 0.0
	fill.anchor_bottom = 1.0
	track.add_child(fill)


func _slots_for_role(role: String, snapshot: Dictionary) -> Array:
	var inventories: Array = snapshot.get("inventories", [])
	for raw_inventory: Variant in inventories:
		var inventory: Dictionary = raw_inventory
		if str(inventory.get("role", "")) == role:
			return inventory.get("slots", [])
	return []


func _subtitle_text(snapshot: Dictionary) -> String:
	var active_recipe := str(snapshot.get("active_recipe", ""))
	var status := str(snapshot.get("status", "idle"))
	if active_recipe.is_empty():
		return status
	return "%s - %s" % [status, _recipe_label_by_id(active_recipe, snapshot.get("recipes", []))]


func _active_recipe_label(active_recipe: String, recipes: Array) -> String:
	if active_recipe.is_empty():
		return "Recipes"
	return _recipe_label_by_id(active_recipe, recipes)


func _recipe_label_by_id(recipe_id: String, recipes: Array) -> String:
	for raw_recipe: Variant in recipes:
		var recipe: Dictionary = raw_recipe
		if str(recipe.get("id", "")) == recipe_id:
			return str(recipe.get("label", recipe_id))
	return recipe_id.replace("_", " ").capitalize()


func _recipe_card_text(recipe: Dictionary) -> String:
	var outputs: Array = recipe.get("outputs", [])
	var suffix := ""
	if not outputs.is_empty():
		var output: Dictionary = outputs[0]
		suffix = "\n%s x%d" % [_short_item_label(str(output.get("item", ""))), int(output.get("amount", 0))]
	return "%s%s" % [str(recipe.get("label", "")), suffix]


func _short_item_label(item: String) -> String:
	var words := item.split("_")
	var label := ""
	for word in words:
		if not word.is_empty():
			label += word.substr(0, 1).to_upper()
	return label if not label.is_empty() else item.substr(0, min(item.length(), 3)).to_upper()


func _panel_stylebox() -> StyleBoxFlat:
	return _flat_stylebox(PANEL_BG, PANEL_BORDER)


func _flat_stylebox(bg: Color, border: Color) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = bg
	style.border_color = border
	style.border_width_left = 1
	style.border_width_top = 1
	style.border_width_right = 1
	style.border_width_bottom = 1
	return style


func _clear_children(node: Node) -> void:
	for child: Node in node.get_children():
		node.remove_child(child)
		child.queue_free()
