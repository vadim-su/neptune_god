extends Control
class_name DevConsole

signal command_submitted(line: String)

const MAX_SCROLLBACK_LINES := 240
const PANEL_BG := Color(0.020, 0.025, 0.025, 0.78)
const PANEL_BORDER := Color(0.560, 0.760, 0.420, 0.72)
const TEXT := Color(0.820, 0.840, 0.780)
const INPUT_TEXT := Color(0.960, 0.940, 0.780)

@onready var panel: PanelContainer = %ConsolePanel
@onready var scrollback_label: Label = %ScrollbackText
@onready var input_line: LineEdit = %InputLine

var _scrollback: Array[String] = []
var _history: Array[String] = []
var _history_cursor := -1
var _completions: Array[String] = []


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_STOP
	panel.add_theme_stylebox_override("panel", _stylebox(PANEL_BG, PANEL_BORDER, 1))
	input_line.focus_mode = Control.FOCUS_ALL
	input_line.keep_editing_on_text_submit = true
	input_line.text_submitted.connect(_on_text_submitted)
	input_line.gui_input.connect(_on_input_gui_input)
	input_line.add_theme_color_override("font_color", INPUT_TEXT)
	scrollback_label.add_theme_color_override("font_color", TEXT)
	if not Engine.is_editor_hint():
		visible = false


func _process(_delta: float) -> void:
	if visible and input_line != null and get_viewport().gui_get_focus_owner() != input_line:
		input_line.grab_focus()


func toggle_console() -> void:
	if visible:
		close_console()
	else:
		open_console()


func open_console() -> void:
	visible = true
	_history_cursor = -1
	_focus_input()


func close_console() -> void:
	visible = false
	_history_cursor = -1
	input_line.release_focus()


func is_open() -> bool:
	return visible


func set_completions(values: Array) -> void:
	_completions.clear()
	for raw_value: Variant in values:
		var value := str(raw_value)
		if not value.is_empty() and not _completions.has(value):
			_completions.append(value)
	_completions.sort()


func append_output(line: String) -> void:
	_scrollback.append(line)
	var overflow := _scrollback.size() - MAX_SCROLLBACK_LINES
	if overflow > 0:
		_scrollback = _scrollback.slice(overflow)
	_refresh_scrollback()


func append_lines(lines: Array) -> void:
	for line: Variant in lines:
		append_output(str(line))


func clear_scrollback() -> void:
	_scrollback.clear()
	_refresh_scrollback()


func _on_text_submitted(raw_line: String) -> void:
	var line := raw_line.strip_edges()
	input_line.text = ""
	_history_cursor = -1
	if line.is_empty():
		call_deferred("_focus_input")
		return
	_history.append(line)
	append_output("> %s" % line)
	command_submitted.emit(line)
	call_deferred("_focus_input")


func _on_input_gui_input(event: InputEvent) -> void:
	if not visible:
		return
	if event is InputEventKey and event.pressed and not event.echo:
		_handle_console_key(event)


func _unhandled_key_input(event: InputEvent) -> void:
	if not visible:
		return
	if event is InputEventKey and event.pressed and not event.echo:
		_handle_console_key(event)


func _handle_console_key(event: InputEventKey) -> void:
	match event.keycode:
		KEY_UP:
			_history_previous()
			accept_event()
		KEY_DOWN:
			_history_next()
			accept_event()
		KEY_TAB:
			_complete_input()
			accept_event()


func _history_previous() -> void:
	if _history.is_empty():
		return
	if _history_cursor == -1:
		_history_cursor = _history.size() - 1
	else:
		_history_cursor = max(0, _history_cursor - 1)
	_apply_history_cursor()


func _history_next() -> void:
	if _history_cursor == -1:
		return
	if _history_cursor + 1 >= _history.size():
		_history_cursor = -1
		input_line.text = ""
		return
	_history_cursor += 1
	_apply_history_cursor()


func _apply_history_cursor() -> void:
	input_line.text = _history[_history_cursor]
	input_line.caret_column = input_line.text.length()


func _complete_input() -> void:
	var text := input_line.text
	var prefix_start := text.rfind(" ") + 1
	var prefix := text.substr(prefix_start)
	if prefix.is_empty() and not text.ends_with(" "):
		return

	var matches: Array[String] = []
	for value: String in _completions:
		if value.begins_with(prefix):
			matches.append(value)
	if matches.is_empty():
		return
	if matches.size() == 1:
		input_line.text = text.substr(0, prefix_start) + matches[0]
		input_line.caret_column = input_line.text.length()
		_focus_input()
		return
	append_output("matches: %s" % "  ".join(matches))
	_focus_input()


func _focus_input() -> void:
	if visible and input_line != null:
		input_line.grab_focus()


func _refresh_scrollback() -> void:
	scrollback_label.text = "\n".join(_scrollback)


func _stylebox(bg: Color, border: Color, border_width: int) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = bg
	style.border_color = border
	style.border_width_left = 0
	style.border_width_top = 0
	style.border_width_right = 0
	style.border_width_bottom = border_width
	return style
