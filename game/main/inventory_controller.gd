extends RefCounted
class_name InventoryController

var inventory_window: Node
var machine_window: Node
var sim: Variant
var selected_object_snapshot: Callable


func setup(
	window: Node,
	machine_window_node: Node,
	simulation: Variant,
	selected_snapshot: Callable
) -> void:
	inventory_window = window
	machine_window = machine_window_node
	sim = simulation
	selected_object_snapshot = selected_snapshot
	if inventory_window != null:
		inventory_window.slot_transfer_requested.connect(_on_slot_transfer_requested)
		inventory_window.slot_action_requested.connect(_on_slot_action_requested)


func is_open() -> bool:
	return inventory_window != null and inventory_window.is_open()


func toggle() -> void:
	if inventory_window.is_open():
		inventory_window.hide_window()
		return
	if machine_window != null:
		machine_window.hide_window()
	update()
	inventory_window.show_inventory(
		sim.inventory_snapshot(),
		_selected_object(),
		_selected_object_ui_snapshot()
	)


func update() -> void:
	if inventory_window == null:
		return
	inventory_window.update_inventory(
		sim.inventory_snapshot(),
		_selected_object(),
		_selected_object_ui_snapshot()
	)


func _on_slot_transfer_requested(from_ref: Dictionary, to_ref: Dictionary, amount: int) -> void:
	if sim.transfer_inventory_slot(from_ref, to_ref, amount):
		update()


func _on_slot_action_requested(slot_ref: Dictionary, action: String) -> void:
	if sim.click_inventory_slot(slot_ref, action):
		update()


func _selected_object_ui_snapshot() -> Dictionary:
	var snapshot := _selected_snapshot()
	var selected_object: Dictionary = snapshot.get("object", {})
	if selected_object.is_empty():
		return {}
	return sim.building_ui_snapshot(int(snapshot.get("id", -1)))


func _selected_object() -> Dictionary:
	return _selected_snapshot().get("object", {})


func _selected_snapshot() -> Dictionary:
	if not selected_object_snapshot.is_valid():
		return {}
	return selected_object_snapshot.call()
