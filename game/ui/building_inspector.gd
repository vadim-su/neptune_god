extends RefCounted
class_name BuildingInspector

const BuildingCatalogScript := preload("res://game/buildings/building_catalog.gd")
const BuildingGeometryScript := preload("res://game/buildings/building_geometry.gd")
const PANEL_BG := Color(0.070, 0.075, 0.065, 0.94)
const PANEL_BORDER := Color(0.560, 0.760, 0.420, 0.72)

var panel: PanelContainer
var title: Label
var body: Label


func setup(parent: Node) -> void:
	panel = PanelContainer.new()
	panel.name = "BuildingInterface"
	panel.mouse_filter = Control.MOUSE_FILTER_STOP
	panel.visible = false
	panel.anchor_left = 1.0
	panel.anchor_right = 1.0
	panel.anchor_top = 0.0
	panel.anchor_bottom = 0.0
	panel.offset_left = -320.0
	panel.offset_top = 190.0
	panel.offset_right = -12.0
	panel.offset_bottom = 368.0
	panel.add_theme_stylebox_override("panel", _stylebox())
	parent.add_child(panel)

	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 12)
	margin.add_theme_constant_override("margin_top", 12)
	margin.add_theme_constant_override("margin_right", 12)
	margin.add_theme_constant_override("margin_bottom", 12)
	panel.add_child(margin)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 8)
	margin.add_child(column)

	title = Label.new()
	title.add_theme_font_size_override("font_size", 18)
	title.add_theme_color_override("font_color", Color(0.92, 0.94, 0.86))
	column.add_child(title)

	body = Label.new()
	body.add_theme_font_size_override("font_size", 14)
	body.add_theme_color_override("font_color", Color(0.78, 0.80, 0.74))
	body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(body)


func update(building: Dictionary) -> void:
	if building.is_empty() or panel == null:
		return

	var footprint: Array = building["footprint"]
	var origin := Vector2i(int(building["x"]), int(building["y"]))
	var def_id := str(building["def_id"])
	title.text = "%s #%d" % [BuildingCatalogScript.display_name(def_id), int(building["id"])]
	body.text = "Type: %s\nState: %s\nOrigin: %d, %d\nRotation: %d deg\nFootprint: %s" % [
		BuildingCatalogScript.ui_type(def_id),
		BuildingCatalogScript.state_label(def_id),
		origin.x,
		origin.y,
		int(building["quarter_turns"]) * 90,
		BuildingGeometryScript.footprint_size_text(footprint),
	]
	panel.visible = true


func hide() -> void:
	if panel != null:
		panel.visible = false


func _stylebox() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = PANEL_BG
	style.border_color = PANEL_BORDER
	style.border_width_left = 1
	style.border_width_top = 1
	style.border_width_right = 1
	style.border_width_bottom = 1
	return style
