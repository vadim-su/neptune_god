@tool
extends VBoxContainer

var status_label: Label
var cancel_button: Button

func _ready() -> void:
	status_label = Label.new()
	status_label.text = "MCP: Disconnected"
	add_child(status_label)

	cancel_button = Button.new()
	cancel_button.text = "Cancel Operation"
	cancel_button.disabled = true
	cancel_button.pressed.connect(_on_cancel_pressed)
	add_child(cancel_button)

func update_status(text: String) -> void:
	if status_label:
		status_label.text = text

func set_operation_active(active: bool) -> void:
	if cancel_button:
		cancel_button.disabled = not active

func _on_cancel_pressed() -> void:
	var server = get_node_or_null("/root/EditorNode/MCPServer")
	if server:
		server.cancel_current_operation()
