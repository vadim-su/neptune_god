@tool
extends EditorPlugin

const CARGO_EXECUTABLE = "cargo"


func _build() -> bool:
	var manifest_path := ProjectSettings.globalize_path("res://Cargo.toml")
	var args := PackedStringArray(["build", "--release", "--manifest-path", manifest_path])
	var output: Array = []

	print("Running cargo build --release before starting the project...")
	var exit_code := OS.execute(
		CARGO_EXECUTABLE,
		args,
		output,
		true,
		false
	)

	for line in output:
		print(line)

	if exit_code != 0:
		push_error("cargo build --release failed with exit code %d" % exit_code)
		return false

	print("cargo build --release finished")
	return true
