extends PanelContainer
class_name GameWindow

signal close_requested

@export var draggable := true
@export var header_path: NodePath
@export var close_button_path: NodePath
@export var title_label_path: NodePath

var _header: Control
var _close_button: Button
var _title_label: Label
var _dragging := false
var _drag_offset := Vector2.ZERO


func _ready() -> void:
	_resolve_window_nodes()
	if _header != null:
		_header.mouse_filter = Control.MOUSE_FILTER_STOP
		_header.gui_input.connect(_on_header_gui_input)
	if _close_button != null:
		_close_button.pressed.connect(func() -> void:
			close_requested.emit()
		)


func _input(event: InputEvent) -> void:
	if not _dragging:
		return

	if event is InputEventMouseMotion:
		global_position = get_viewport().get_mouse_position() - _drag_offset
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and not event.pressed:
		_dragging = false
		get_viewport().set_input_as_handled()


func set_window_title(value: String) -> void:
	if _title_label != null:
		_title_label.text = value


func stop_dragging() -> void:
	_dragging = false


func _resolve_window_nodes() -> void:
	_header = _node_from_path_or_name(header_path, "Header") as Control
	_close_button = _node_from_path_or_name(close_button_path, "CloseButton") as Button
	_title_label = _node_from_path_or_name(title_label_path, "WindowTitle") as Label


func _node_from_path_or_name(path: NodePath, fallback_name: String) -> Node:
	if not path.is_empty() and has_node(path):
		return get_node(path)
	return find_child(fallback_name, true, false)


func _on_header_gui_input(event: InputEvent) -> void:
	if not draggable:
		return
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		_dragging = true
		_drag_offset = get_viewport().get_mouse_position() - global_position
		get_viewport().set_input_as_handled()
