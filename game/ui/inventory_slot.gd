extends PanelContainer
class_name InventorySlot

signal transfer_requested(from_ref: Dictionary, to_ref: Dictionary, amount: int)
signal action_requested(slot_ref: Dictionary, action: String)

const ItemCatalogScript := preload("res://game/items/item_catalog.gd")
const PREVIEW_BG := Color(0.070, 0.075, 0.065, 0.92)
const PREVIEW_BORDER := Color(0.950, 0.840, 0.550, 0.95)

var slot_ref: Dictionary = {}
var item_id := ""
var amount := 0
var preview_texture: Texture2D
@onready var icon: TextureRect = get_node_or_null("Icon") as TextureRect
@onready var amount_label: Label = get_node_or_null("AmountLabel") as Label


func _ready() -> void:
	_apply_view_state()


func configure(ref: Dictionary, item: String, stack_amount: int, texture: Texture2D) -> void:
	slot_ref = ref.duplicate(true)
	item_id = item
	amount = stack_amount
	preview_texture = texture
	mouse_filter = Control.MOUSE_FILTER_STOP
	_apply_view_state()


func _gui_input(event: InputEvent) -> void:
	if slot_ref.is_empty():
		return
	if not (event is InputEventMouseButton) or event.pressed:
		return

	match event.button_index:
		MOUSE_BUTTON_LEFT:
			action_requested.emit(slot_ref, "split" if _ctrl_pressed() else "stack")
			accept_event()
		MOUSE_BUTTON_RIGHT:
			action_requested.emit(slot_ref, "one")
			accept_event()


func _get_drag_data(_at_position: Vector2) -> Variant:
	if item_id.is_empty() or amount <= 0 or slot_ref.is_empty():
		return null

	set_drag_preview(_drag_preview())
	return {
		"type": "inventory_slot",
		"source": slot_ref,
		"item": item_id,
		"amount": amount,
	}


func _can_drop_data(_at_position: Vector2, data: Variant) -> bool:
	return not slot_ref.is_empty() \
		and data is Dictionary \
		and str(data.get("type", "")) == "inventory_slot" \
		and data.get("source", {}) is Dictionary \
		and not _same_ref(data.get("source", {}), slot_ref)


func _drop_data(_at_position: Vector2, data: Variant) -> void:
	if not _can_drop_data(_at_position, data):
		return
	var source: Dictionary = data["source"]
	transfer_requested.emit(source, slot_ref, int(data.get("amount", 0)))


func _drag_preview() -> Control:
	var preview := PanelContainer.new()
	preview.custom_minimum_size = custom_minimum_size
	preview.add_theme_stylebox_override("panel", _stylebox(PREVIEW_BG, PREVIEW_BORDER))

	if preview_texture != null:
		var icon := TextureRect.new()
		icon.texture = preview_texture
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		icon.offset_left = 3.0
		icon.offset_top = 3.0
		icon.offset_right = -3.0
		icon.offset_bottom = -3.0
		preview.add_child(icon)

	return preview


func _apply_view_state() -> void:
	if icon == null:
		icon = get_node_or_null("Icon") as TextureRect
	if amount_label == null:
		amount_label = get_node_or_null("AmountLabel") as Label

	if icon != null:
		icon.texture = preview_texture
		icon.visible = not item_id.is_empty()
		icon.tooltip_text = ItemCatalogScript.display_name(item_id) if not item_id.is_empty() else ""
	if amount_label != null:
		amount_label.text = str(amount) if amount > 1 else ""
		amount_label.visible = amount > 1


func _same_ref(left: Dictionary, right: Dictionary) -> bool:
	if str(left.get("kind", "")) != str(right.get("kind", "")):
		return false
	match str(left.get("kind", "")):
		"character":
			return str(left.get("container", "")) == str(right.get("container", "")) \
				and int(left.get("slot", -1)) == int(right.get("slot", -2))
		"building":
			return int(left.get("building_id", -1)) == int(right.get("building_id", -2)) \
				and str(left.get("role", "")) == str(right.get("role", "")) \
				and int(left.get("slot", -1)) == int(right.get("slot", -2))
		"player":
			return int(left.get("slot", -1)) == int(right.get("slot", -2))
	return false


func _stylebox(bg: Color, border: Color) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = bg
	style.border_color = border
	style.border_width_left = 1
	style.border_width_top = 1
	style.border_width_right = 1
	style.border_width_bottom = 1
	return style


func _ctrl_pressed() -> bool:
	return Input.is_key_pressed(KEY_CTRL) \
		or Input.is_key_pressed(KEY_META)
