extends Node3D

const ItemCatalogScript := preload("res://game/items/item_catalog.gd")

signal chunks_changed

const TILE_SIZE := 1.0
const TERRAIN_Y := 0.0
const RESOURCE_Y := 0.035

const TERRAIN_COLORS := {
	"ground": Color(0.18, 0.26, 0.18),
	"stone": Color(0.32, 0.33, 0.31),
	"water": Color(0.10, 0.24, 0.34),
}

const RESOURCE_COLORS := {
	"iron_ore": Color(0.56, 0.49, 0.40),
	"copper_ore": Color(0.78, 0.39, 0.18),
	"coal": Color(0.05, 0.05, 0.05),
}

const TERRAIN_TEXTURES := {
	"ground": preload("res://assets/images/ground_tile.png"),
	"stone": preload("res://assets/images/terrain_stone.png"),
	"water": preload("res://assets/images/terrain_water.png"),
}

const RESOURCE_TEXTURES := {
	"iron_ore": preload("res://assets/images/resource_iron_ore.png"),
	"copper_ore": preload("res://assets/images/resource_copper_ore.png"),
	"coal": preload("res://assets/images/resource_coal.png"),
}

const TERRAIN_BLEND_SHADER := preload("res://game/world/terrain_blend.gdshader")
const CHUNK_BLEND_MARGIN := 1
const MAX_BACKGROUND_CHUNK_JOBS := 1
const MAX_READY_CHUNK_RESULTS_DEQUEUED_PER_FRAME := 1
const MAX_CHUNK_APPLY_STAGES_PER_FRAME := 1
const TERRAIN_RENDER_BATCH_SIZE_TILES := 16
const RESOURCE_RENDER_BATCH_SIZE_INSTANCES := 128

enum ChunkApplyStage { STORE_TILES, TERRAIN_ROOT, TERRAIN_BATCH, RESOURCE_BATCH, FINALIZE }


class WorldChunkRenderTask:
	extends RefCounted

	const TASK_TERRAIN_Y := 0.0
	const TASK_RESOURCE_Y := 0.035
	const TASK_TERRAIN_RENDER_BATCH_SIZE_TILES := 16
	const TASK_RESOURCE_RENDER_BATCH_SIZE_INSTANCES := 128

	var epoch := 0
	var chunk := Vector2i.ZERO
	var tiles: Array = []
	var render_result: Dictionary = {}

	func _init(next_chunk: Vector2i, next_tiles: Array, next_epoch: int) -> void:
		chunk = next_chunk
		tiles = next_tiles
		epoch = next_epoch

	func run() -> void:
		render_result = {
			"epoch": epoch,
			"chunk": chunk,
			"tiles": tiles,
			"tile_bounds": tile_bounds(tiles),
			"tile_signature": tile_signature(tiles),
			"terrain": terrain_chunk_render_data(tiles),
			"resources": resource_chunk_render_data(tiles),
		}

	func result() -> Dictionary:
		return render_result

	static func terrain_chunk_render_data(source_tiles: Array) -> Dictionary:
		var terrain_by_pos := {}
		for raw_tile: Variant in source_tiles:
			var tile: Dictionary = raw_tile
			terrain_by_pos[Vector2i(tile["x"], tile["y"])] = tile["terrain"]

		var tiles_by_batch := {}
		for raw_tile: Variant in source_tiles:
			var tile: Dictionary = raw_tile
			if not bool(tile.get("render", true)):
				continue
			var pos := Vector2i(tile["x"], tile["y"])
			var batch_key := Vector2i(
				int(floor(float(pos.x) / float(TASK_TERRAIN_RENDER_BATCH_SIZE_TILES))),
				int(floor(float(pos.y) / float(TASK_TERRAIN_RENDER_BATCH_SIZE_TILES)))
			)
			if not tiles_by_batch.has(batch_key):
				tiles_by_batch[batch_key] = []
			tiles_by_batch[batch_key].append(tile)

		var batch_keys: Array = tiles_by_batch.keys()
		batch_keys.sort_custom(func(left: Vector2i, right: Vector2i) -> bool:
			if left.y == right.y:
				return left.x < right.x
			return left.y < right.y
		)

		var batches: Array[Dictionary] = []
		for batch_key: Vector2i in batch_keys:
			batches.append(terrain_chunk_render_batch_data(tiles_by_batch[batch_key], terrain_by_pos))
		return {
			"batches": batches,
		}

	static func terrain_chunk_render_batch_data(source_tiles: Array, terrain_by_pos: Dictionary) -> Dictionary:
		var vertices := PackedVector3Array()
		var normals := PackedVector3Array()
		var uvs := PackedVector2Array()
		var colors := PackedColorArray()
		var indices := PackedInt32Array()

		for raw_tile: Variant in source_tiles:
			var tile: Dictionary = raw_tile
			if not bool(tile.get("render", true)):
				continue
			add_blended_tile_geometry(
				vertices,
				normals,
				uvs,
				colors,
				indices,
				Vector2i(tile["x"], tile["y"]),
				terrain_by_pos
			)

		return {
			"vertices": vertices,
			"normals": normals,
			"uvs": uvs,
			"colors": colors,
			"indices": indices,
		}

	static func add_blended_tile_geometry(
		vertices: PackedVector3Array,
		normals: PackedVector3Array,
		uvs: PackedVector2Array,
		colors: PackedColorArray,
		indices: PackedInt32Array,
		pos: Vector2i,
		terrain_by_pos: Dictionary
	) -> void:
		var base_index := vertices.size()
		var offsets := [-0.5, 0.0, 0.5]

		for z_offset: float in offsets:
			for x_offset: float in offsets:
				var world_x := float(pos.x) + x_offset
				var world_z := float(pos.y) + z_offset
				vertices.append(Vector3(world_x, TASK_TERRAIN_Y, world_z))
				normals.append(Vector3.UP)
				uvs.append(Vector2(world_x + 0.5, world_z + 0.5))
				colors.append(terrain_blend_weight(pos, x_offset, z_offset, terrain_by_pos))

		var quads := [
			[0, 1, 4, 3],
			[1, 2, 5, 4],
			[3, 4, 7, 6],
			[4, 5, 8, 7],
		]
		for quad: Array in quads:
			indices.append(base_index + quad[0])
			indices.append(base_index + quad[1])
			indices.append(base_index + quad[2])
			indices.append(base_index + quad[0])
			indices.append(base_index + quad[2])
			indices.append(base_index + quad[3])

	static func terrain_blend_weight(pos: Vector2i, x_offset: float, z_offset: float, terrain_by_pos: Dictionary) -> Color:
		var samples: Array[Vector2i] = [pos]
		if x_offset < 0.0:
			samples.append(pos + Vector2i(-1, 0))
		elif x_offset > 0.0:
			samples.append(pos + Vector2i(1, 0))

		if z_offset < 0.0:
			samples.append(pos + Vector2i(0, -1))
		elif z_offset > 0.0:
			samples.append(pos + Vector2i(0, 1))

		if x_offset != 0.0 and z_offset != 0.0:
			samples.append(pos + Vector2i(signi(x_offset), signi(z_offset)))

		var weight := Vector3.ZERO
		for sample_pos: Vector2i in samples:
			weight += terrain_weight_vector(terrain_by_pos.get(sample_pos, terrain_by_pos.get(pos, "ground")))
		weight /= float(samples.size())
		return Color(weight.x, weight.y, weight.z, 1.0)

	static func terrain_weight_vector(terrain_id: String) -> Vector3:
		match terrain_id:
			"stone":
				return Vector3(0.0, 1.0, 0.0)
			"water":
				return Vector3(0.0, 0.0, 1.0)
			_:
				return Vector3(1.0, 0.0, 0.0)

	static func resource_chunk_render_data(source_tiles: Array) -> Dictionary:
		var positions_by_resource := {}
		for raw_tile: Variant in source_tiles:
			var tile: Dictionary = raw_tile
			if not bool(tile.get("render", true)):
				continue
			var resource_id := str(tile["resource"])
			if resource_id.is_empty():
				continue
			if not positions_by_resource.has(resource_id):
				positions_by_resource[resource_id] = []
			positions_by_resource[resource_id].append(Vector3(float(tile["x"]), TASK_RESOURCE_Y, float(tile["y"])))
		var resource_ids: Array = positions_by_resource.keys()
		resource_ids.sort()
		var batches: Array[Dictionary] = []
		for resource_id: String in resource_ids:
			var positions: Array = positions_by_resource[resource_id]
			var batch_start := 0
			while batch_start < positions.size():
				var batch_positions: Array = []
				var batch_end: int = mini(batch_start + TASK_RESOURCE_RENDER_BATCH_SIZE_INSTANCES, positions.size())
				for index in range(batch_start, batch_end):
					batch_positions.append(positions[index])
				batches.append({
					"resource": resource_id,
					"positions": batch_positions,
				})
				batch_start = batch_end
		return {
			"batches": batches,
		}

	static func tile_signature(source_tiles: Array) -> String:
		var hash_value := hash(source_tiles.size())
		for raw_tile: Variant in source_tiles:
			var tile: Dictionary = raw_tile
			hash_value = hash([
				hash_value,
				int(tile.get("x", 0)),
				int(tile.get("y", 0)),
				str(tile.get("terrain", "")),
				str(tile.get("resource", "")),
				int(tile.get("amount", 0)),
				bool(tile.get("render", true)),
			])
		return str(hash_value)

	static func tile_bounds(source_tiles: Array) -> Rect2i:
		var bounds_initialized := false
		var min_x := 0
		var max_x := 0
		var min_y := 0
		var max_y := 0
		for raw_tile: Variant in source_tiles:
			var tile: Dictionary = raw_tile
			if not bool(tile.get("render", true)):
				continue
			var tile_x := int(tile["x"])
			var tile_y := int(tile["y"])
			if not bounds_initialized:
				min_x = tile_x
				max_x = tile_x
				min_y = tile_y
				max_y = tile_y
				bounds_initialized = true
				continue
			min_x = mini(min_x, tile_x)
			max_x = maxi(max_x, tile_x)
			min_y = mini(min_y, tile_y)
			max_y = maxi(max_y, tile_y)
		if not bounds_initialized:
			return Rect2i(Vector2i.ZERO, Vector2i.ONE)
		return Rect2i(Vector2i(min_x, min_y), Vector2i(max_x - min_x + 1, max_y - min_y + 1))


var _map_root: Node3D
var _terrain_chunks := {}
var _resource_chunks := {}
var _tiles_by_chunk := {}
var _tile_bounds_by_chunk := {}
var _tile_signature_by_chunk := {}
var _visible_chunks := {}
var _explored_chunks := {}
var _pending_chunks: Array[Vector2i] = []
var _pending_chunk_read_index := 0
var _pending_chunk_lookup := {}
var _loading_tile_jobs := {}
var _loading_chunk_tasks := {}
var _pending_chunk_render_tasks: Array[Dictionary] = []
var _pending_chunk_render_task_read_index := 0
var _ready_chunk_results: Array[Dictionary] = []
var _ready_chunk_result_read_index := 0
var _pending_chunk_applies: Array[Dictionary] = []
var _pending_chunk_apply_read_index := 0
var _chunk_result_mutex := Mutex.new()
var _chunk_loading_epoch := 0
var _chunk_tile_provider: Variant
var _terrain_material: ShaderMaterial
var _resource_material_cache := {}
var _resource_plane_mesh: PlaneMesh
var _chunk_visibility_update_count := 0
var _pending_chunk_refresh_count := 0
var _chunk_snapshot_revision := 0
var _chunk_snapshot_grid_size_cache := Vector2i.ZERO
var _visible_chunk_snapshot_cache_revision := -1
var _visible_chunk_snapshot_cache: Dictionary = {}
var _explored_chunk_snapshot_cache_revision := -1
var _explored_chunk_snapshot_cache: Dictionary = {}
var _visible_chunk_snapshot_rect_cache := {}
var _explored_chunk_snapshot_rect_cache := {}


func build_from_sim(sim: NeptuneSim) -> void:
	clear_generated_map()

	var tiles: Array = sim.map_tiles()
	_tiles_by_chunk[Vector2i.ZERO] = tiles
	_tile_bounds_by_chunk[Vector2i.ZERO] = WorldChunkRenderTask.tile_bounds(tiles)
	_remember_chunk_snapshot_grid_size(_tile_bounds_by_chunk[Vector2i.ZERO])
	_tile_signature_by_chunk[Vector2i.ZERO] = WorldChunkRenderTask.tile_signature(tiles)
	_terrain_chunks[Vector2i.ZERO] = _add_terrain_chunk(Vector2i.ZERO, tiles)
	_resource_chunks[Vector2i.ZERO] = _add_resource_chunk(Vector2i.ZERO, tiles)
	_bump_chunk_snapshot_revision()


func clear_generated_map() -> void:
	_wait_for_loading_tasks()
	if _map_root != null:
		_map_root.queue_free()

	_map_root = Node3D.new()
	_map_root.name = "GeneratedMap"
	add_child(_map_root)
	_terrain_chunks.clear()
	_resource_chunks.clear()
	_tiles_by_chunk.clear()
	_tile_bounds_by_chunk.clear()
	_chunk_snapshot_grid_size_cache = Vector2i.ZERO
	_tile_signature_by_chunk.clear()
	_visible_chunks.clear()
	_explored_chunks.clear()
	_pending_chunks.clear()
	_pending_chunk_read_index = 0
	_pending_chunk_lookup.clear()
	_discard_loading_tile_jobs()
	_pending_chunk_render_tasks.clear()
	_pending_chunk_render_task_read_index = 0
	_ready_chunk_results.clear()
	_ready_chunk_result_read_index = 0
	_pending_chunk_applies.clear()
	_pending_chunk_apply_read_index = 0
	_chunk_tile_provider = null
	_terrain_material = null
	_resource_material_cache.clear()
	_resource_plane_mesh = null
	_chunk_loading_epoch += 1
	_bump_chunk_snapshot_revision()


func sync_chunks(tile_provider: Variant, visible_chunks: Array, preload_chunks: Array = []) -> void:
	if _map_root == null:
		clear_generated_map()
	_chunk_tile_provider = tile_provider

	var visible_lookup := {}
	var explored_changed := false
	for raw_chunk: Variant in visible_chunks:
		var chunk: Vector2i = raw_chunk
		visible_lookup[chunk] = true
		if not _explored_chunks.has(chunk):
			explored_changed = true
		_explored_chunks[chunk] = true

	_set_visible_chunks(visible_lookup)
	if explored_changed:
		_bump_chunk_snapshot_revision()
	_refresh_pending_chunks(_ordered_unique_chunks(visible_chunks, preload_chunks))
	_start_background_chunk_jobs(_chunk_tile_provider)


func _process(_delta: float) -> void:
	_collect_completed_tile_jobs()
	_start_pending_chunk_render_tasks()
	_collect_completed_chunk_tasks()
	_queue_ready_chunk_applies(MAX_READY_CHUNK_RESULTS_DEQUEUED_PER_FRAME)
	_apply_pending_chunk_stages(MAX_CHUNK_APPLY_STAGES_PER_FRAME)
	if _chunk_tile_provider != null:
		_start_background_chunk_jobs(_chunk_tile_provider)


func _exit_tree() -> void:
	_wait_for_loading_tasks()


func _ordered_unique_chunks(visible_chunks: Array, preload_chunks: Array) -> Array[Vector2i]:
	var ordered_chunks: Array[Vector2i] = []
	var added := {}
	for source: Array in [visible_chunks, preload_chunks if not preload_chunks.is_empty() else visible_chunks]:
		for raw_chunk: Variant in source:
			var chunk: Vector2i = raw_chunk
			if added.has(chunk):
				continue
			added[chunk] = true
			ordered_chunks.append(chunk)
	return ordered_chunks


func _refresh_pending_chunks(chunks_to_load: Array[Vector2i]) -> void:
	_pending_chunk_refresh_count += 1
	_pending_chunks.clear()
	_pending_chunk_read_index = 0
	_pending_chunk_lookup.clear()
	for chunk: Vector2i in chunks_to_load:
		if _terrain_chunks.has(chunk) or _is_chunk_loading(chunk):
			continue
		_pending_chunks.append(chunk)
		_pending_chunk_lookup[chunk] = true


func _set_visible_chunks(visible_lookup: Dictionary) -> void:
	var previous_visible := _visible_chunks
	_visible_chunks = visible_lookup
	var visibility_changed := not _chunk_lookup_equal(previous_visible, _visible_chunks)
	for chunk: Vector2i in previous_visible.keys():
		if not _visible_chunks.has(chunk):
			_set_chunk_nodes_visible(chunk, false)
	for chunk: Vector2i in _visible_chunks.keys():
		if not previous_visible.has(chunk):
			_set_chunk_nodes_visible(chunk, true)
	if visibility_changed:
		_bump_chunk_snapshot_revision()


func _update_chunk_visibility() -> void:
	for chunk: Vector2i in _terrain_chunks.keys():
		_set_chunk_nodes_visible(chunk, _visible_chunks.has(chunk))


func _set_chunk_nodes_visible(chunk: Vector2i, is_visible: bool) -> void:
	_chunk_visibility_update_count += 1
	if _terrain_chunks.has(chunk):
		var terrain_node := _terrain_chunks[chunk] as Node3D
		terrain_node.visible = is_visible
	if _resource_chunks.has(chunk):
		var resource_node := _resource_chunks[chunk] as Node3D
		resource_node.visible = is_visible


func _start_background_chunk_jobs(tile_provider: Variant) -> void:
	if not tile_provider.has_method("start_chunk_tiles_job"):
		push_error("Chunk tile provider must support async start_chunk_tiles_job()")
		return
	_normalize_pending_chunk_queue()
	while _active_background_job_count() < MAX_BACKGROUND_CHUNK_JOBS and _pending_chunk_read_index < _pending_chunks.size():
		var chunk := _pending_chunks[_pending_chunk_read_index] as Vector2i
		_pending_chunk_read_index += 1
		_pending_chunk_lookup.erase(chunk)
		var job_id: int = tile_provider.start_chunk_tiles_job(chunk.x, chunk.y, CHUNK_BLEND_MARGIN)
		if job_id < 0:
			push_error("Failed to start async chunk tile job for %d,%d" % [chunk.x, chunk.y])
			continue
		_loading_tile_jobs[job_id] = chunk
	_compact_pending_chunk_queue()


func _active_background_job_count() -> int:
	return _loading_tile_jobs.size() + _loading_chunk_tasks.size()


func _normalize_pending_chunk_queue() -> void:
	if _pending_chunks.is_empty() and _pending_chunk_read_index != 0:
		_pending_chunk_read_index = 0
	elif _pending_chunk_read_index > _pending_chunks.size():
		_pending_chunk_read_index = _pending_chunks.size()


func _compact_pending_chunk_queue() -> void:
	if _pending_chunk_read_index <= 0:
		return
	if _pending_chunk_read_index >= _pending_chunks.size():
		_pending_chunks.clear()
		_pending_chunk_read_index = 0
	elif _pending_chunk_read_index >= 32:
		_pending_chunks = _pending_chunks.slice(_pending_chunk_read_index)
		_pending_chunk_read_index = 0


func _collect_completed_tile_jobs() -> void:
	if _chunk_tile_provider == null or not _chunk_tile_provider.has_method("is_chunk_tiles_job_ready"):
		return
	for job_id: int in _loading_tile_jobs.keys():
		if not _chunk_tile_provider.is_chunk_tiles_job_ready(job_id):
			continue
		var chunk: Vector2i = _loading_tile_jobs[job_id]
		var tiles: Array = _chunk_tile_provider.take_chunk_tiles_job(job_id)
		_loading_tile_jobs.erase(job_id)
		_enqueue_chunk_render_task(chunk, tiles, "Prepare world chunk render data %d,%d" % [chunk.x, chunk.y])


func _enqueue_chunk_render_task(chunk: Vector2i, tiles: Array, description: String) -> void:
	_pending_chunk_render_tasks.append({
		"chunk": chunk,
		"tiles": tiles,
		"description": description,
		"epoch": _chunk_loading_epoch,
	})


func _start_pending_chunk_render_tasks() -> void:
	_normalize_pending_chunk_render_task_queue()
	while _active_background_job_count() < MAX_BACKGROUND_CHUNK_JOBS and _pending_chunk_render_task_read_index < _pending_chunk_render_tasks.size():
		var pending: Dictionary = _pending_chunk_render_tasks[_pending_chunk_render_task_read_index]
		if int(pending.get("epoch", -1)) != _chunk_loading_epoch:
			_pending_chunk_render_task_read_index += 1
			continue
		var started := _start_chunk_render_task(
			pending["chunk"],
			pending["tiles"],
			str(pending.get("description", "Prepare world chunk render data"))
		)
		if not started:
			break
		_pending_chunk_render_task_read_index += 1
	_compact_pending_chunk_render_task_queue()


func _start_chunk_render_task(chunk: Vector2i, tiles: Array, description: String) -> bool:
	var task := _chunk_render_task(chunk, tiles, _chunk_loading_epoch)
	var task_id := WorkerThreadPool.add_task(Callable(task, "run"), false, description)
	if task_id < 0:
		push_error("Failed to start world chunk render task for %d,%d" % [chunk.x, chunk.y])
		return false
	_loading_chunk_tasks[task_id] = task
	return true


func _chunk_render_task(chunk: Vector2i, tiles: Array, epoch: int) -> WorldChunkRenderTask:
	return WorldChunkRenderTask.new(chunk, tiles.duplicate(false), epoch)


func _normalize_pending_chunk_render_task_queue() -> void:
	if _pending_chunk_render_tasks.is_empty() and _pending_chunk_render_task_read_index != 0:
		_pending_chunk_render_task_read_index = 0
	elif _pending_chunk_render_task_read_index > _pending_chunk_render_tasks.size():
		_pending_chunk_render_task_read_index = _pending_chunk_render_tasks.size()


func _compact_pending_chunk_render_task_queue() -> void:
	if _pending_chunk_render_task_read_index <= 0:
		return
	if _pending_chunk_render_task_read_index >= _pending_chunk_render_tasks.size():
		_pending_chunk_render_tasks.clear()
		_pending_chunk_render_task_read_index = 0
	elif _pending_chunk_render_task_read_index >= 32:
		_pending_chunk_render_tasks = _pending_chunk_render_tasks.slice(_pending_chunk_render_task_read_index)
		_pending_chunk_render_task_read_index = 0


func _append_ready_chunk_result(result: Dictionary) -> void:
	_chunk_result_mutex.lock()
	_ready_chunk_results.append(result)
	_chunk_result_mutex.unlock()


func _collect_completed_chunk_tasks() -> void:
	for task_id: int in _loading_chunk_tasks.keys():
		if WorkerThreadPool.is_task_completed(task_id):
			var task: WorldChunkRenderTask = _loading_chunk_tasks[task_id]
			WorkerThreadPool.wait_for_task_completion(task_id)
			_loading_chunk_tasks.erase(task_id)
			_append_ready_chunk_result(task.result())


func _queue_ready_chunk_applies(limit: int) -> void:
	var ready_results: Array[Dictionary] = []
	_chunk_result_mutex.lock()
	while _ready_chunk_result_read_index < _ready_chunk_results.size() and ready_results.size() < limit:
		ready_results.append(_ready_chunk_results[_ready_chunk_result_read_index])
		_ready_chunk_result_read_index += 1
	_compact_ready_chunk_results()
	_chunk_result_mutex.unlock()

	for result: Dictionary in ready_results:
		result["stage"] = ChunkApplyStage.STORE_TILES
		result["terrain_index"] = 0
		result["resource_index"] = 0
		_pending_chunk_applies.append(result)


func _compact_ready_chunk_results() -> void:
	if _ready_chunk_result_read_index <= 0:
		return
	if _ready_chunk_result_read_index >= _ready_chunk_results.size():
		_ready_chunk_results.clear()
		_ready_chunk_result_read_index = 0
	elif _ready_chunk_result_read_index >= 32:
		_ready_chunk_results = _ready_chunk_results.slice(_ready_chunk_result_read_index)
		_ready_chunk_result_read_index = 0


func _apply_ready_chunk_results(limit: int) -> void:
	_queue_ready_chunk_applies(limit)
	_apply_pending_chunk_stages(limit)


func _apply_pending_chunk_stages(limit: int) -> void:
	var applied_snapshot_chunk := false
	var stages_applied := 0
	_normalize_pending_chunk_apply_queue()
	while stages_applied < limit and _pending_chunk_apply_read_index < _pending_chunk_applies.size():
		var result: Dictionary = _pending_chunk_applies[_pending_chunk_apply_read_index]
		if int(result["epoch"]) != _chunk_loading_epoch:
			_pending_chunk_apply_read_index += 1
			continue
		var chunk: Vector2i = result["chunk"]

		var stage := int(result.get("stage", ChunkApplyStage.STORE_TILES))
		if stage == ChunkApplyStage.STORE_TILES and _terrain_chunks.has(chunk):
			_pending_chunk_apply_read_index += 1
			continue
		match stage:
			ChunkApplyStage.STORE_TILES:
				var tiles: Array = result["tiles"]
				_tiles_by_chunk[chunk] = tiles
				_tile_bounds_by_chunk[chunk] = result.get("tile_bounds", WorldChunkRenderTask.tile_bounds(tiles))
				_remember_chunk_snapshot_grid_size(_tile_bounds_by_chunk[chunk])
				_tile_signature_by_chunk[chunk] = str(result.get("tile_signature", WorldChunkRenderTask.tile_signature(tiles)))
				var snapshot_relevant := _chunk_affects_snapshots(chunk)
				result["snapshot_relevant"] = snapshot_relevant
				if snapshot_relevant:
					_bump_chunk_snapshot_revision()
				result["stage"] = ChunkApplyStage.TERRAIN_ROOT
				_pending_chunk_applies[_pending_chunk_apply_read_index] = result
			ChunkApplyStage.TERRAIN_ROOT:
				_terrain_chunks[chunk] = _add_empty_terrain_chunk(chunk)
				(_terrain_chunks[chunk] as Node3D).visible = _visible_chunks.has(chunk)
				result["terrain_index"] = 0
				result["stage"] = ChunkApplyStage.TERRAIN_BATCH
				_pending_chunk_applies[_pending_chunk_apply_read_index] = result
			ChunkApplyStage.TERRAIN_BATCH:
				var terrain_batches: Array = _terrain_render_batches(result["terrain"])
				var terrain_index := int(result.get("terrain_index", 0))
				if terrain_index < terrain_batches.size():
					_add_terrain_batch_instance(
						_terrain_chunks[chunk] as Node3D,
						chunk,
						terrain_index,
						terrain_batches[terrain_index]
					)
					result["terrain_index"] = terrain_index + 1
					_pending_chunk_applies[_pending_chunk_apply_read_index] = result
				else:
					var resource_batches: Array = _resource_render_batches(result["resources"])
					_resource_chunks[chunk] = _add_empty_resource_chunk(chunk)
					result["resource_batches"] = resource_batches
					result["resource_index"] = 0
					result["stage"] = ChunkApplyStage.RESOURCE_BATCH
					_pending_chunk_applies[_pending_chunk_apply_read_index] = result
			ChunkApplyStage.RESOURCE_BATCH:
				var resource_batches: Array = result.get("resource_batches", [])
				var resource_index := int(result.get("resource_index", 0))
				if resource_index < resource_batches.size():
					var resource_batch: Dictionary = resource_batches[resource_index]
					_add_resource_instances(
						_resource_chunks[chunk] as Node3D,
						str(resource_batch.get("resource", "")),
						resource_batch.get("positions", [])
					)
					result["resource_index"] = resource_index + 1
					_pending_chunk_applies[_pending_chunk_apply_read_index] = result
				else:
					_finalize_pending_chunk_apply(chunk)
					_pending_chunk_apply_read_index += 1
					applied_snapshot_chunk = applied_snapshot_chunk or bool(result.get("snapshot_relevant", false))
			ChunkApplyStage.FINALIZE:
				_finalize_pending_chunk_apply(chunk)
				_pending_chunk_apply_read_index += 1
				applied_snapshot_chunk = applied_snapshot_chunk or bool(result.get("snapshot_relevant", false))
		stages_applied += 1

	if applied_snapshot_chunk:
		chunks_changed.emit()
	_compact_pending_chunk_apply_queue()


func _finalize_pending_chunk_apply(chunk: Vector2i) -> void:
	_set_chunk_nodes_visible(chunk, _visible_chunks.has(chunk))


func _chunk_affects_snapshots(chunk: Vector2i) -> bool:
	return _visible_chunks.has(chunk) or _explored_chunks.has(chunk)


func _normalize_pending_chunk_apply_queue() -> void:
	if _pending_chunk_applies.is_empty() and _pending_chunk_apply_read_index != 0:
		_pending_chunk_apply_read_index = 0
	elif _pending_chunk_apply_read_index > _pending_chunk_applies.size():
		_pending_chunk_apply_read_index = _pending_chunk_applies.size()


func _compact_pending_chunk_apply_queue() -> void:
	if _pending_chunk_apply_read_index <= 0:
		return
	if _pending_chunk_apply_read_index >= _pending_chunk_applies.size():
		_pending_chunk_applies.clear()
		_pending_chunk_apply_read_index = 0
	elif _pending_chunk_apply_read_index >= 32:
		_pending_chunk_applies = _pending_chunk_applies.slice(_pending_chunk_apply_read_index)
		_pending_chunk_apply_read_index = 0


func _is_chunk_loading(chunk: Vector2i) -> bool:
	for loading_chunk: Vector2i in _loading_tile_jobs.values():
		if loading_chunk == chunk:
			return true
	for loading_task: WorldChunkRenderTask in _loading_chunk_tasks.values():
		if loading_task.chunk == chunk:
			return true
	for index in range(_pending_chunk_render_task_read_index, _pending_chunk_render_tasks.size()):
		var pending_render: Dictionary = _pending_chunk_render_tasks[index]
		if pending_render.get("chunk", Vector2i.ZERO) == chunk:
			return true
	for index in range(_pending_chunk_apply_read_index, _pending_chunk_applies.size()):
		var apply_result: Dictionary = _pending_chunk_applies[index]
		if apply_result.get("chunk", Vector2i.ZERO) == chunk:
			return true
	return false


func _wait_for_loading_tasks() -> void:
	_discard_loading_tile_jobs()
	for task_id: int in _loading_chunk_tasks.keys():
		WorkerThreadPool.wait_for_task_completion(task_id)
	_loading_chunk_tasks.clear()
	_pending_chunk_render_tasks.clear()
	_pending_chunk_render_task_read_index = 0


func _discard_loading_tile_jobs() -> void:
	if _chunk_tile_provider != null and _chunk_tile_provider.has_method("discard_chunk_tiles_job"):
		for job_id: int in _loading_tile_jobs.keys():
			_chunk_tile_provider.discard_chunk_tiles_job(job_id)
	_loading_tile_jobs.clear()


func visible_chunk_snapshot() -> Dictionary:
	if _visible_chunk_snapshot_cache_revision == _chunk_snapshot_revision:
		return _visible_chunk_snapshot_cache
	_visible_chunk_snapshot_cache = _chunk_snapshot_for(_visible_chunks)
	_visible_chunk_snapshot_cache_revision = _chunk_snapshot_revision
	return _visible_chunk_snapshot_cache


func explored_chunk_snapshot() -> Dictionary:
	if _explored_chunk_snapshot_cache_revision == _chunk_snapshot_revision:
		return _explored_chunk_snapshot_cache
	_explored_chunk_snapshot_cache = _chunk_snapshot_for(_explored_chunks)
	_explored_chunk_snapshot_cache_revision = _chunk_snapshot_revision
	return _explored_chunk_snapshot_cache


func visible_chunk_snapshot_for_rect(tile_rect: Rect2i) -> Dictionary:
	return _cached_chunk_snapshot_for_rect(_visible_chunks, tile_rect, _visible_chunk_snapshot_rect_cache)


func explored_chunk_snapshot_for_rect(tile_rect: Rect2i) -> Dictionary:
	return _cached_chunk_snapshot_for_rect(_explored_chunks, tile_rect, _explored_chunk_snapshot_rect_cache)


func chunk_snapshot_grid_size() -> Vector2i:
	return _chunk_snapshot_grid_size()


func visible_tile_rect() -> Rect2i:
	return visible_chunk_snapshot()["rect"]


func _cached_chunk_snapshot_for_rect(chunk_lookup: Dictionary, tile_rect: Rect2i, cache: Dictionary) -> Dictionary:
	var cache_key := _chunk_snapshot_rect_cache_key(tile_rect)
	if cache.has(cache_key):
		return cache[cache_key]
	var snapshot := _chunk_snapshot_for_rect(chunk_lookup, tile_rect)
	cache[cache_key] = snapshot
	return snapshot


func _chunk_snapshot_rect_cache_key(tile_rect: Rect2i) -> String:
	return "%d:%d:%d:%d:%d" % [
		_chunk_snapshot_revision,
		tile_rect.position.x,
		tile_rect.position.y,
		tile_rect.size.x,
		tile_rect.size.y,
	]


func _bump_chunk_snapshot_revision() -> void:
	_chunk_snapshot_revision += 1
	_visible_chunk_snapshot_cache_revision = -1
	_visible_chunk_snapshot_cache = {}
	_explored_chunk_snapshot_cache_revision = -1
	_explored_chunk_snapshot_cache = {}
	_visible_chunk_snapshot_rect_cache.clear()
	_explored_chunk_snapshot_rect_cache.clear()


func _chunk_lookup_equal(left: Dictionary, right: Dictionary) -> bool:
	if left.size() != right.size():
		return false
	for chunk: Vector2i in left.keys():
		if not right.has(chunk):
			return false
	return true


func _chunk_snapshot_for(chunk_lookup: Dictionary) -> Dictionary:
	var chunks: Array[Dictionary] = []
	var bounds := Rect2i()
	var bounds_initialized := false
	var snapshot_key_hash := hash(0)
	for chunk: Vector2i in chunk_lookup.keys():
		if not _tiles_by_chunk.has(chunk):
			continue
		var chunk_bounds: Rect2i = _tile_bounds_by_chunk.get(chunk, Rect2i())
		if chunk_bounds.size.x <= 0 or chunk_bounds.size.y <= 0:
			continue
		var chunk_key := "%d:%d" % [chunk.x, chunk.y]
		var chunk_signature := str(_tile_signature_by_chunk.get(chunk, ""))
		chunks.append({
			"key": chunk_key,
			"chunk": chunk,
			"bounds": chunk_bounds,
			"signature": chunk_signature,
			"tiles": _tiles_by_chunk[chunk],
		})
		snapshot_key_hash = hash([snapshot_key_hash, chunk_key, chunk_bounds, chunk_signature])
		if not bounds_initialized:
			bounds = chunk_bounds
			bounds_initialized = true
		else:
			bounds = _rect_union(bounds, chunk_bounds)
	if not bounds_initialized:
		bounds = Rect2i(Vector2i.ZERO, Vector2i.ONE)
	return {
		"chunks": chunks,
		"rect": bounds,
		"key": str(hash([chunks.size(), snapshot_key_hash])),
	}


func _chunk_snapshot_for_rect(chunk_lookup: Dictionary, tile_rect: Rect2i) -> Dictionary:
	if tile_rect.size.x <= 0 or tile_rect.size.y <= 0:
		return {
			"chunks": [],
			"rect": Rect2i(Vector2i.ZERO, Vector2i.ONE),
			"key": str(hash([0])),
		}

	var grid_size := _chunk_snapshot_grid_size()
	if grid_size.x <= 0 or grid_size.y <= 0:
		return _chunk_snapshot_for(chunk_lookup)

	var min_chunk := _tile_to_snapshot_chunk(tile_rect.position, grid_size)
	var max_chunk := _tile_to_snapshot_chunk(tile_rect.position + tile_rect.size - Vector2i.ONE, grid_size)
	var scoped_lookup := {}
	for chunk_y in range(min_chunk.y, max_chunk.y + 1):
		for chunk_x in range(min_chunk.x, max_chunk.x + 1):
			var chunk := Vector2i(chunk_x, chunk_y)
			if chunk_lookup.has(chunk):
				scoped_lookup[chunk] = true
	var snapshot := _chunk_snapshot_for(scoped_lookup)
	if (snapshot["chunks"] as Array).is_empty():
		snapshot["rect"] = tile_rect
	return snapshot


func _chunk_snapshot_grid_size() -> Vector2i:
	if _chunk_snapshot_grid_size_cache.x > 0 and _chunk_snapshot_grid_size_cache.y > 0:
		return _chunk_snapshot_grid_size_cache
	for raw_bounds: Variant in _tile_bounds_by_chunk.values():
		var bounds: Rect2i = raw_bounds
		if bounds.size.x > 0 and bounds.size.y > 0:
			_remember_chunk_snapshot_grid_size(bounds)
			return _chunk_snapshot_grid_size_cache
	return Vector2i.ZERO


func _remember_chunk_snapshot_grid_size(bounds: Rect2i) -> void:
	if _chunk_snapshot_grid_size_cache.x > 0 and _chunk_snapshot_grid_size_cache.y > 0:
		return
	if bounds.size.x <= 0 or bounds.size.y <= 0:
		return
	_chunk_snapshot_grid_size_cache = bounds.size


func _tile_to_snapshot_chunk(tile: Vector2i, grid_size: Vector2i) -> Vector2i:
	return Vector2i(
		int(floor(float(tile.x) / float(maxi(grid_size.x, 1)))),
		int(floor(float(tile.y) / float(maxi(grid_size.y, 1))))
	)


func _render_tiles(tiles: Array) -> Array:
	var render_tiles: Array = []
	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		if bool(tile.get("render", true)):
			render_tiles.append(tile)
	return render_tiles


func _add_terrain_chunk(chunk: Vector2i, tiles: Array) -> Node3D:
	return _add_terrain_chunk_from_data(chunk, _terrain_chunk_render_data(tiles))


func _terrain_chunk_render_data(tiles: Array) -> Dictionary:
	return WorldChunkRenderTask.terrain_chunk_render_data(tiles)


func _terrain_chunk_render_batch_data(tiles: Array, terrain_by_pos: Dictionary) -> Dictionary:
	return WorldChunkRenderTask.terrain_chunk_render_batch_data(tiles, terrain_by_pos)


func _add_terrain_chunk_from_data(chunk: Vector2i, render_data: Dictionary) -> Node3D:
	var terrain_root := _add_empty_terrain_chunk(chunk)
	var batch_index := 0
	for batch: Dictionary in _terrain_render_batches(render_data):
		_add_terrain_batch_instance(terrain_root, chunk, batch_index, batch)
		batch_index += 1
	return terrain_root


func _add_empty_terrain_chunk(chunk: Vector2i) -> Node3D:
	var terrain_root := Node3D.new()
	terrain_root.name = "Terrain_%d_%d" % [chunk.x, chunk.y]
	_map_root.add_child(terrain_root)
	return terrain_root


func _terrain_render_batches(render_data: Dictionary) -> Array:
	if render_data.has("batches"):
		return render_data["batches"]
	return [render_data]


func _add_terrain_batch_instance(terrain_root: Node3D, chunk: Vector2i, batch_index: int, render_data: Dictionary) -> void:
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = render_data["vertices"]
	arrays[Mesh.ARRAY_NORMAL] = render_data["normals"]
	arrays[Mesh.ARRAY_TEX_UV] = render_data["uvs"]
	arrays[Mesh.ARRAY_COLOR] = render_data["colors"]
	arrays[Mesh.ARRAY_INDEX] = render_data["indices"]

	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)

	var instance := MeshInstance3D.new()
	instance.name = "Terrain_%d_%d_%d" % [chunk.x, chunk.y, batch_index]
	instance.mesh = mesh
	instance.material_override = _terrain_blend_material()
	terrain_root.add_child(instance)


func _add_blended_tile_geometry(
	vertices: PackedVector3Array,
	normals: PackedVector3Array,
	uvs: PackedVector2Array,
	colors: PackedColorArray,
	indices: PackedInt32Array,
	pos: Vector2i,
	terrain_by_pos: Dictionary
) -> void:
	WorldChunkRenderTask.add_blended_tile_geometry(
		vertices,
		normals,
		uvs,
		colors,
		indices,
		pos,
		terrain_by_pos
	)


func _terrain_blend_weight(pos: Vector2i, x_offset: float, z_offset: float, terrain_by_pos: Dictionary) -> Color:
	return WorldChunkRenderTask.terrain_blend_weight(pos, x_offset, z_offset, terrain_by_pos)


func _terrain_weight_vector(terrain_id: String) -> Vector3:
	return WorldChunkRenderTask.terrain_weight_vector(terrain_id)


func _terrain_blend_material() -> ShaderMaterial:
	if _terrain_material != null:
		return _terrain_material
	var material := ShaderMaterial.new()
	material.shader = TERRAIN_BLEND_SHADER
	material.set_shader_parameter("ground_texture", TERRAIN_TEXTURES["ground"])
	material.set_shader_parameter("stone_texture", TERRAIN_TEXTURES["stone"])
	material.set_shader_parameter("water_texture", TERRAIN_TEXTURES["water"])
	material.set_shader_parameter("terrain_scale", 0.18)
	material.set_shader_parameter("detail_scale", 0.47)
	material.set_shader_parameter("detail_strength", 0.28)
	_terrain_material = material
	return _terrain_material


func _resource_material(resource_id: String) -> StandardMaterial3D:
	if _resource_material_cache.has(resource_id):
		return _resource_material_cache[resource_id]
	var material := StandardMaterial3D.new()
	material.roughness = 0.86
	var texture: Texture2D = RESOURCE_TEXTURES.get(resource_id, null)
	if texture != null:
		material.albedo_color = Color.WHITE
		material.albedo_texture = texture
	else:
		material.albedo_color = _resource_color(resource_id)
	_resource_material_cache[resource_id] = material
	return material


func _resource_color(resource_id: String) -> Color:
	if RESOURCE_COLORS.has(resource_id):
		return RESOURCE_COLORS[resource_id]
	return ItemCatalogScript.color(resource_id)


func _add_resource_chunk(chunk: Vector2i, tiles: Array) -> Node3D:
	return _add_resource_chunk_from_data(chunk, _resource_chunk_render_data(tiles))


func _resource_chunk_render_data(tiles: Array) -> Dictionary:
	return WorldChunkRenderTask.resource_chunk_render_data(tiles)


func _add_resource_chunk_from_data(chunk: Vector2i, positions_by_resource: Dictionary) -> Node3D:
	var resource_root := _add_empty_resource_chunk(chunk)

	for resource_batch: Dictionary in _resource_render_batches(positions_by_resource):
		_add_resource_instances(
			resource_root,
			str(resource_batch.get("resource", "")),
			resource_batch.get("positions", [])
		)
	return resource_root


func _resource_render_batches(render_data: Dictionary) -> Array:
	if render_data.has("batches"):
		return render_data["batches"]
	var resource_ids: Array = render_data.keys()
	resource_ids.sort()
	var batches: Array[Dictionary] = []
	for resource_id: String in resource_ids:
		batches.append({
			"resource": resource_id,
			"positions": render_data[resource_id],
		})
	return batches


func _add_empty_resource_chunk(chunk: Vector2i) -> Node3D:
	var resource_root := Node3D.new()
	resource_root.name = "Resources_%d_%d" % [chunk.x, chunk.y]
	_map_root.add_child(resource_root)
	return resource_root


func _add_resource_instances(resource_root: Node3D, resource_id: String, positions: Array) -> void:
	if positions.is_empty():
		return

	var multimesh := MultiMesh.new()
	multimesh.transform_format = MultiMesh.TRANSFORM_3D
	multimesh.mesh = _resource_plane()
	multimesh.instance_count = positions.size()

	for index in positions.size():
		multimesh.set_instance_transform(index, Transform3D(Basis(), positions[index]))

	var instance := MultiMeshInstance3D.new()
	instance.name = "Resource_%s" % resource_id
	instance.multimesh = multimesh
	instance.material_override = _resource_material(resource_id)
	resource_root.add_child(instance)


func _resource_plane() -> PlaneMesh:
	if _resource_plane_mesh == null:
		_resource_plane_mesh = PlaneMesh.new()
		_resource_plane_mesh.size = Vector2(0.94, 0.94)
	return _resource_plane_mesh


func _bounds_for_tiles(tiles: Array) -> Rect2i:
	if tiles.is_empty():
		return Rect2i(Vector2i.ZERO, Vector2i.ONE)

	var first: Dictionary = tiles[0]
	var min_x: int = first["x"]
	var max_x: int = first["x"]
	var min_y: int = first["y"]
	var max_y: int = first["y"]

	for raw_tile: Variant in tiles:
		var tile: Dictionary = raw_tile
		min_x = mini(min_x, tile["x"])
		max_x = maxi(max_x, tile["x"])
		min_y = mini(min_y, tile["y"])
		max_y = maxi(max_y, tile["y"])

	return Rect2i(Vector2i(min_x, min_y), Vector2i(max_x - min_x + 1, max_y - min_y + 1))


func _rect_union(left: Rect2i, right: Rect2i) -> Rect2i:
	var min_pos := Vector2i(mini(left.position.x, right.position.x), mini(left.position.y, right.position.y))
	var left_end := left.position + left.size
	var right_end := right.position + right.size
	var max_pos := Vector2i(maxi(left_end.x, right_end.x), maxi(left_end.y, right_end.y))
	return Rect2i(min_pos, max_pos - min_pos)
