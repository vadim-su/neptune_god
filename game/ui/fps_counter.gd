extends PanelContainer
class_name FpsCounter

@onready var label: Label = %FpsLabel


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	if label != null:
		label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	update_value()


func _process(_delta: float) -> void:
	update_value()


func update_value(frames_per_second := -1.0) -> void:
	if label == null:
		return
	var current_fps := frames_per_second
	if current_fps < 0.0:
		current_fps = Engine.get_frames_per_second()
	label.text = format_text(current_fps)


func keep_above_control(control: Control, offset := 20) -> void:
	if control == null:
		return
	z_index = control.z_index + offset


static func format_text(frames_per_second: float) -> String:
	return "FPS: %d" % int(round(frames_per_second))
