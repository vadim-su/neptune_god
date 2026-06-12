extends Control
class_name CatalogSelector

signal entry_selected(owner_id: String, entry: Dictionary)
signal closed(owner_id: String)

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const BuildingIconRendererScript := preload("res://game/ui/building_icon_renderer.gd")

const PANEL_BG := Color(0.070, 0.075, 0.065, 0.96)
const PANEL_BORDER := Color(0.560, 0.760, 0.420, 0.72)
const SLOT_BG := Color(0.105, 0.112, 0.098, 0.98)
const SLOT_BORDER := Color(0.310, 0.350, 0.265, 0.95)
const ACTIVE_BG := Color(0.140, 0.180, 0.160, 0.94)
const TEXT := Color(0.820, 0.840, 0.780)
const MUTED_TEXT := Color(0.620, 0.670, 0.620, 1.0)
const TITLE := Color(0.920, 0.940, 0.860)
const WINDOW_WIDTH := 910.0
const WINDOW_HEIGHT := 620.0
const BOTTOM_MARGIN := 112.0
const CARD_SIZE := Vector2(204.0, 54.0)

enum GroupingMode { CATEGORY, USAGE, NAME }

@onready var panel: PanelContainer = %Panel
@onready var window_title: Label = %WindowTitle
@onready var close_button: Button = %CloseButton
@onready var search_field: LineEdit = %SearchField
@onready var category_button: Button = %CategoryButton
@onready var usage_button: Button = %UsageButton
@onready var name_button: Button = %NameButton
@onready var groups_container: VBoxContainer = %Groups

var owner_id := ""
var entries: Array[Dictionary] = []
var grouping_mode := GroupingMode.CATEGORY
var collapsed_groups := {}
var _icon_renderer: Node


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_STOP
	anchors_preset = Control.PRESET_FULL_RECT
	panel.add_theme_stylebox_override("panel", _stylebox(PANEL_BG, PANEL_BORDER, 1, 0))
	close_button.pressed.connect(close_selector)
	search_field.text_changed.connect(_on_search_changed)
	category_button.pressed.connect(_set_grouping_mode.bind(GroupingMode.CATEGORY))
	usage_button.pressed.connect(_set_grouping_mode.bind(GroupingMode.USAGE))
	name_button.pressed.connect(_set_grouping_mode.bind(GroupingMode.NAME))
	_icon_renderer = BuildingIconRendererScript.new()
	_icon_renderer.name = "BuildingIconRenderer"
	add_child(_icon_renderer)
	if not Engine.is_editor_hint():
		visible = false


func _unhandled_input(event: InputEvent) -> void:
	if not visible:
		return
	if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_ESCAPE:
		close_selector()
		get_viewport().set_input_as_handled()


func open_selector(new_owner_id: String, new_entries: Array, title: String = "Catalog") -> void:
	owner_id = new_owner_id
	entries.clear()
	for raw_entry: Variant in new_entries:
		if raw_entry is Dictionary:
			entries.append(_normalized_entry(raw_entry))
	window_title.text = title
	search_field.text = ""
	visible = true
	_position_panel()
	_prepare_icons()
	_refresh()
	search_field.grab_focus()


func close_selector() -> void:
	if not visible:
		return
	var closing_owner := owner_id
	visible = false
	owner_id = ""
	closed.emit(closing_owner)


func is_open() -> bool:
	return visible


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED and panel != null and visible:
		_position_panel()


func _position_panel() -> void:
	var viewport_size := get_viewport_rect().size
	var height: float = min(WINDOW_HEIGHT, max(300.0, viewport_size.y - BOTTOM_MARGIN - 16.0))
	var width: float = min(WINDOW_WIDTH, max(360.0, viewport_size.x - 32.0))
	panel.custom_minimum_size = Vector2(width, height)
	panel.anchor_left = 0.5
	panel.anchor_right = 0.5
	panel.anchor_top = 1.0
	panel.anchor_bottom = 1.0
	panel.offset_left = -width * 0.5
	panel.offset_right = width * 0.5
	panel.offset_top = -height - BOTTOM_MARGIN
	panel.offset_bottom = -BOTTOM_MARGIN


func _normalized_entry(entry: Dictionary) -> Dictionary:
	var normalized := entry.duplicate(true)
	var id := str(normalized.get("id", ""))
	normalized["id"] = id
	if str(normalized.get("label", "")).is_empty():
		normalized["label"] = BuildingCatalogScript.display_name(id)
	if str(normalized.get("kind", "")).is_empty():
		normalized["kind"] = "building"
	if str(normalized.get("category", "")).is_empty() and str(normalized.get("kind", "")) == "building":
		normalized["category"] = BuildingCatalogScript.ui_type(id)
	if not normalized.has("usage_tags"):
		normalized["usage_tags"] = ["buildable"] if str(normalized.get("kind", "")) == "building" else []
	if str(normalized.get("search_text", "")).is_empty():
		normalized["search_text"] = "%s %s %s" % [
			id,
			str(normalized.get("label", "")),
			str(normalized.get("category", "")),
		]
	return normalized


func _prepare_icons() -> void:
	if _icon_renderer == null:
		return
	var ids: Array = []
	for entry: Dictionary in entries:
		if str(entry.get("kind", "")) != "building":
			continue
		var id := str(entry.get("id", ""))
		if not id.is_empty() and not ids.has(id):
			ids.append(id)
	_icon_renderer.prepare_icons(ids)


func _refresh() -> void:
	_clear_children(groups_container)
	_refresh_grouping_buttons()
	var filtered := _filtered_entries()
	var groups := _group_entries(filtered)
	if groups.is_empty():
		var empty := Label.new()
		empty.text = "No matches"
		empty.add_theme_font_size_override("font_size", 13)
		empty.add_theme_color_override("font_color", MUTED_TEXT)
		groups_container.add_child(empty)
		return

	for group: Dictionary in groups:
		_add_group(group)


func _filtered_entries() -> Array[Dictionary]:
	var query := search_field.text.strip_edges().to_lower()
	var result: Array[Dictionary] = []
	for entry: Dictionary in entries:
		if query.is_empty() or _entry_matches(entry, query):
			result.append(entry)
	return result


func _entry_matches(entry: Dictionary, query: String) -> bool:
	var haystack := "%s %s %s %s" % [
		str(entry.get("id", "")),
		str(entry.get("label", "")),
		str(entry.get("category", "")),
		str(entry.get("search_text", "")),
	]
	for raw_tag: Variant in entry.get("usage_tags", []):
		haystack += " %s" % str(raw_tag)
	return haystack.to_lower().contains(query)


func _group_entries(source_entries: Array[Dictionary]) -> Array[Dictionary]:
	var by_id := {}
	for entry: Dictionary in source_entries:
		var key := _group_key(entry)
		if not by_id.has(key["id"]):
			by_id[key["id"]] = {"id": key["id"], "title": key["title"], "entries": []}
		by_id[key["id"]]["entries"].append(entry)

	var groups: Array[Dictionary] = []
	for raw_group: Variant in by_id.values():
		groups.append(raw_group)
	groups.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		var a_id := str(a.get("id", ""))
		var b_id := str(b.get("id", ""))
		if a_id == "other":
			return false
		if b_id == "other":
			return true
		return a_id < b_id
	)
	return groups


func _group_key(entry: Dictionary) -> Dictionary:
	match grouping_mode:
		GroupingMode.USAGE:
			var tags: Array = entry.get("usage_tags", [])
			if tags.is_empty():
				return {"id": "other", "title": "Other"}
			var tag := str(tags[0])
			return {"id": _slug(tag), "title": _titleize(tag)}
		GroupingMode.NAME:
			var label := str(entry.get("label", ""))
			var first := "#"
			var first_character := label.substr(0, 1)
			if not first_character.is_empty() and first_character.is_valid_identifier():
				first = first_character.to_upper()
			return {"id": first.to_lower(), "title": first}
		_:
			var category := str(entry.get("category", ""))
			if category.strip_edges().is_empty():
				return {"id": "other", "title": "Other"}
			return {"id": _slug(category), "title": _titleize(category)}


func _add_group(group: Dictionary) -> void:
	var group_id := str(group.get("id", ""))
	var group_entries: Array = group.get("entries", [])
	var collapsed := collapsed_groups.has(_collapse_key(group_id)) and search_field.text.strip_edges().is_empty()

	var section := VBoxContainer.new()
	section.add_theme_constant_override("separation", 4)
	groups_container.add_child(section)

	var header := Button.new()
	header.focus_mode = Control.FOCUS_NONE
	header.alignment = HORIZONTAL_ALIGNMENT_LEFT
	header.text = "%s %s (%d)" % [">" if collapsed else "v", str(group.get("title", "")), group_entries.size()]
	header.add_theme_color_override("font_color", TITLE)
	header.add_theme_font_size_override("font_size", 14)
	header.add_theme_stylebox_override("normal", _stylebox(Color.TRANSPARENT, Color.TRANSPARENT, 0, 0))
	header.add_theme_stylebox_override("hover", _stylebox(Color(0.105, 0.125, 0.120, 0.70), Color.TRANSPARENT, 0, 0))
	header.pressed.connect(_toggle_group.bind(group_id))
	section.add_child(header)

	if collapsed:
		return

	var grid := GridContainer.new()
	var available_width := maxf(1.0, panel.custom_minimum_size.x - 56.0)
	grid.columns = clampi(int(floor((available_width + 8.0) / (CARD_SIZE.x + 8.0))), 1, 4)
	grid.add_theme_constant_override("h_separation", 8)
	grid.add_theme_constant_override("v_separation", 8)
	section.add_child(grid)

	for raw_entry: Variant in group_entries:
		var entry: Dictionary = raw_entry
		_add_entry_card(grid, entry)


func _add_entry_card(parent: Control, entry: Dictionary) -> void:
	var button := Button.new()
	button.custom_minimum_size = CARD_SIZE
	button.focus_mode = Control.FOCUS_NONE
	button.text = ""
	button.tooltip_text = str(entry.get("label", ""))
	button.add_theme_stylebox_override("normal", _stylebox(SLOT_BG, SLOT_BORDER, 1, 0))
	button.add_theme_stylebox_override("hover", _stylebox(ACTIVE_BG, PANEL_BORDER, 1, 0))
	button.pressed.connect(func() -> void:
		entry_selected.emit(owner_id, entry)
		close_selector()
	)
	parent.add_child(button)

	var row := HBoxContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 6.0
	row.offset_top = 6.0
	row.offset_right = -6.0
	row.offset_bottom = -6.0
	row.add_theme_constant_override("separation", 8)
	button.add_child(row)

	var icon := TextureRect.new()
	icon.custom_minimum_size = Vector2(34.0, 34.0)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.texture = _entry_texture(entry)
	row.add_child(icon)

	var label := Label.new()
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_font_size_override("font_size", 12)
	label.add_theme_color_override("font_color", TEXT)
	label.text = str(entry.get("label", ""))
	row.add_child(label)


func _entry_texture(entry: Dictionary) -> Texture2D:
	var texture: Variant = entry.get("icon_texture", null)
	if texture is Texture2D:
		return texture
	if str(entry.get("kind", "")) == "building" and _icon_renderer != null:
		return _icon_renderer.texture_for(str(entry.get("id", "")))
	return null


func _refresh_grouping_buttons() -> void:
	_style_grouping_button(category_button, grouping_mode == GroupingMode.CATEGORY)
	_style_grouping_button(usage_button, grouping_mode == GroupingMode.USAGE)
	_style_grouping_button(name_button, grouping_mode == GroupingMode.NAME)


func _style_grouping_button(button: Button, active: bool) -> void:
	button.add_theme_stylebox_override(
		"normal",
		_stylebox(ACTIVE_BG if active else SLOT_BG, PANEL_BORDER if active else SLOT_BORDER, 1, 0)
	)
	button.add_theme_stylebox_override("hover", _stylebox(ACTIVE_BG, PANEL_BORDER, 1, 0))
	button.add_theme_color_override("font_color", TITLE if active else TEXT)


func _set_grouping_mode(mode: int) -> void:
	grouping_mode = mode
	_refresh()


func _toggle_group(group_id: String) -> void:
	if not search_field.text.strip_edges().is_empty():
		return
	var key := _collapse_key(group_id)
	if collapsed_groups.has(key):
		collapsed_groups.erase(key)
	else:
		collapsed_groups[key] = true
	_refresh()


func _collapse_key(group_id: String) -> String:
	return "%d:%s" % [grouping_mode, group_id]


func _on_search_changed(_value: String) -> void:
	_refresh()


func _slug(value: String) -> String:
	var parts: Array[String] = []
	for part: String in value.strip_edges().to_lower().replace("-", "_").replace(" ", "_").split("_"):
		if not part.is_empty():
			parts.append(part)
	return "_".join(parts) if not parts.is_empty() else "other"


func _titleize(value: String) -> String:
	var parts: Array[String] = []
	for part: String in value.strip_edges().replace("-", " ").replace("_", " ").split(" "):
		if not part.is_empty():
			parts.append(part.substr(0, 1).to_upper() + part.substr(1))
	return " ".join(parts) if not parts.is_empty() else "Other"


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
