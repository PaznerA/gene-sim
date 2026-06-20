extends Node2D
## gene-sim thin UI entry point — 2D ecosystem view of one scope (S4.3).
##
## INVARIANT #2 (STOP THE LINE if violated): this renderer READS sim snapshots only. It must NEVER compute
## genotype→phenotype or any biology — all of that lives in the Rust core (crates/genome, crates/sim-core).
## GDScript here only loads/plays snapshot data and draws it: a tiled field backdrop, organism dot markers,
## and a toggleable per-cell data overlay. Data-layer SHADERS + zoom scopes land in S4.4.
##
## CLI (args after `--`):
##   --snap <file.bin>     Headless: parse one snapshot and report its header (S4.2 gate path).
##   --run  <dir>          Play snap_*.bin in <dir> as a live run (windowed; auto-advances, loops).
##   --shot <out.png>      Render one frame to a PNG then quit (needs a display; for verification).
##   --gen  <n>            With --run/--shot: pick the snapshot whose generation == n (else the last).
## With no args and a display, auto-discovers the newest data/runs/<id>/ that holds snap_*.bin.
##
## Keys (windowed): Space pause/resume · D cycle data overlay (off/density/allele/fitness) · ./, step.

## Load the reader by path, not via a `class_name` global: that registry is only populated by an editor
## import pass, so a bare identifier is unresolved under a fresh `--headless` run. `preload` needs no cache.
const SnapshotReader := preload("res://snapshot.gd")
const Organisms := preload("res://organisms.gd")

const OVERLAY_NAMES := ["off", "density", "allele_freq", "fitness"]
const FRAME_SECONDS := 0.45  # seconds per snapshot when playing a run
const TARGET_FIELD_PX := 880.0  # the field is scaled to about this many pixels on its long side

var _snaps: Array = []  # parsed snapshot.gd instances, ordered by generation
var _idx: int = 0
var _cell: float = 12.0
var _overlay_mode: int = 1  # index into OVERLAY_NAMES; 1 = density
var _paused: bool = false

var _terrain: TileMapLayer
var _overlay: Sprite2D
var _organisms: Node2D
var _hud: Label
var _timer: Timer


func _ready() -> void:
	var v := Engine.get_version_info()
	print("gene-sim UI booted — Godot %s (%s)" % [v.string, DisplayServer.get_name()])

	# ---- S4.2 headless gate path: parse one snapshot, report header, quit. Keep exact output ("snapshot OK").
	var snap_path := _arg_value("--snap")
	var shot_path := _arg_value("--shot")
	if snap_path != "" and shot_path == "":
		var snap := SnapshotReader.load_from(snap_path)
		if snap == null:
			printerr("snapshot load FAILED: %s" % snap_path)
			get_tree().quit(1)
			return
		print("snapshot OK — %dx%d, gen=%d, population=%d, cells=%d, channels=%d" % [
			snap.width, snap.height, snap.generation, snap.population, snap.cell_count(), snap.channel_count])
		get_tree().quit()
		return

	# ---- Resolve the snapshots to show: explicit --run dir, a single --snap (for --shot), or auto-discover.
	var run_dir := _arg_value("--run")
	if run_dir != "":
		_snaps = _load_run(run_dir)
	elif snap_path != "":
		var one := SnapshotReader.load_from(snap_path)
		if one != null:
			_snaps = [one]
	else:
		var newest := _newest_run_dir()
		if newest != "":
			print("auto-discovered run: %s" % newest)
			_snaps = _load_run(newest)

	if _snaps.is_empty():
		# Headless smoke (S4.1) with nothing to show, or no run found: boot cleanly and exit.
		if DisplayServer.get_name() == "headless":
			print("headless smoke OK")
		else:
			print("no snapshots to render (pass --run <dir> or --snap <file>)")
		get_tree().quit()
		return

	_build_scene()
	_idx = _pick_index(int(_arg_value("--gen", "-1")))
	_show(_idx)

	# Headless render smoke (gate): build the scene from a run, prove it constructs without a GPU, quit.
	if _has_flag("--check"):
		print("render scene OK — %d snapshots, cell=%d, grid %dx%d" % [
			_snaps.size(), int(_cell), _snaps[0].width, _snaps[0].height])
		get_tree().quit()
		return

	if shot_path != "":
		await _take_shot(shot_path)
		return

	# Live playback (windowed): advance through the run on a timer.
	_timer = Timer.new()
	_timer.wait_time = FRAME_SECONDS
	_timer.timeout.connect(_advance)
	add_child(_timer)
	if _snaps.size() > 1:
		_timer.start()


# ──────────────────────────── scene construction (read-only presentation) ────────────────────────────

func _build_scene() -> void:
	var first = _snaps[0]
	var w: int = first.width
	var h: int = first.height
	_cell = maxf(3.0, floorf(TARGET_FIELD_PX / float(max(w, h))))
	var field_px := Vector2(float(w) * _cell, float(h) * _cell)

	_terrain = _build_terrain(w, h, int(_cell))
	add_child(_terrain)

	_overlay = Sprite2D.new()
	_overlay.centered = false
	_overlay.texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	_overlay.modulate = Color(1, 1, 1, 0.55)
	add_child(_overlay)

	_organisms = Organisms.new()
	add_child(_organisms)

	# A camera framing the whole field; S4.4 adds zoom scopes on top of this.
	var cam := Camera2D.new()
	cam.position = field_px * 0.5
	add_child(cam)
	cam.make_current()  # must be in-tree first

	# HUD on its own CanvasLayer so it ignores the world camera.
	var ui := CanvasLayer.new()
	add_child(ui)
	_hud = Label.new()
	_hud.position = Vector2(14, 10)
	_hud.add_theme_font_size_override("font_size", 18)
	_hud.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
	_hud.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.9))
	_hud.add_theme_constant_override("outline_size", 8)
	ui.add_child(_hud)

	# Size the window to the field (+ margin) when we have a display.
	if DisplayServer.get_name() != "headless":
		var win := (field_px + Vector2(40, 40)).max(Vector2(640, 480))
		DisplayServer.window_set_size(Vector2i(int(win.x), int(win.y)))
	RenderingServer.set_default_clear_color(Color(0.06, 0.08, 0.07))


## A tiled grass field: a small procedurally-generated atlas of green shades placed with hash variation.
## This is the "2D TileMap ecosystem view of one scope" (a field) — pure backdrop, no biology.
func _build_terrain(w: int, h: int, cell: int) -> TileMapLayer:
	var shades := [
		Color(0.16, 0.30, 0.15), Color(0.19, 0.34, 0.17),
		Color(0.14, 0.27, 0.14), Color(0.21, 0.37, 0.19),
	]
	var n := shades.size()
	var atlas := Image.create(cell * n, cell, false, Image.FORMAT_RGBA8)
	for ti in n:
		for yy in cell:
			for xx in cell:
				# subtle per-pixel jitter so tiles read as grass, not flat blocks.
				var j := (_hash01(xx, yy, ti) - 0.5) * 0.06
				var c: Color = shades[ti]
				atlas.set_pixel(ti * cell + xx, yy, Color(c.r + j, c.g + j, c.b + j))
	var tex := ImageTexture.create_from_image(atlas)

	var ts := TileSet.new()
	ts.tile_size = Vector2i(cell, cell)
	var src := TileSetAtlasSource.new()
	src.texture = tex
	src.texture_region_size = Vector2i(cell, cell)
	for ti in n:
		src.create_tile(Vector2i(ti, 0))
	var sid := ts.add_source(src)

	var layer := TileMapLayer.new()
	layer.tile_set = ts
	for y in h:
		for x in w:
			var ti := int(_hash01(x, y, 7) * float(n)) % n
			layer.set_cell(Vector2i(x, y), sid, Vector2i(ti, 0))
	return layer


# ──────────────────────────── per-snapshot update ────────────────────────────

func _show(i: int) -> void:
	if i < 0 or i >= _snaps.size():
		return
	var snap = _snaps[i]
	_organisms.set_snapshot(snap, _cell)
	_update_overlay(snap)
	_hud.text = "gen %d   pop %d   grid %dx%d   layer: %s%s   [%d/%d]" % [
		snap.generation, snap.population, snap.width, snap.height,
		OVERLAY_NAMES[_overlay_mode], ("  (paused)" if _paused else ""),
		i + 1, _snaps.size()]


func _update_overlay(snap) -> void:
	if _overlay_mode == 0:
		_overlay.visible = false
		return
	_overlay.visible = true
	var channel: PackedFloat32Array
	match _overlay_mode:
		2: channel = snap.allele_freq
		3: channel = snap.fitness
		_: channel = snap.density
	var img := Image.create(snap.width, snap.height, false, Image.FORMAT_RGBA8)
	for y in snap.height:
		for x in snap.width:
			img.set_pixel(x, y, _heat(channel[y * snap.width + x]))
	_overlay.texture = ImageTexture.create_from_image(img)
	_overlay.scale = Vector2(_cell, _cell)


## Transparent→blue→cyan→yellow→red ramp; alpha rises with intensity so empty cells stay clear.
func _heat(t: float) -> Color:
	if t <= 0.0:
		return Color(0, 0, 0, 0)
	var c: Color
	if t < 0.5:
		c = Color(0.1, 0.3, 0.9).lerp(Color(0.1, 0.9, 0.7), t / 0.5)
	else:
		c = Color(0.95, 0.85, 0.1).lerp(Color(0.95, 0.15, 0.1), (t - 0.5) / 0.5)
	c.a = 0.15 + 0.7 * clampf(t, 0.0, 1.0)
	return c


func _advance() -> void:
	if _paused or _snaps.size() < 2:
		return
	_idx = (_idx + 1) % _snaps.size()
	_show(_idx)


# ──────────────────────────── input (windowed) ────────────────────────────

func _unhandled_input(event: InputEvent) -> void:
	if not (event is InputEventKey and event.pressed and not event.echo):
		return
	match event.keycode:
		KEY_SPACE:
			_paused = not _paused
			_show(_idx)
		KEY_D:
			_overlay_mode = (_overlay_mode + 1) % OVERLAY_NAMES.size()
			_show(_idx)
		KEY_PERIOD:
			_paused = true
			_idx = (_idx + 1) % _snaps.size()
			_show(_idx)
		KEY_COMMA:
			_paused = true
			_idx = (_idx - 1 + _snaps.size()) % _snaps.size()
			_show(_idx)
		KEY_ESCAPE:
			get_tree().quit()


# ──────────────────────────── snapshot loading / discovery (read-only) ────────────────────────────

## Load every snap_*.bin in a run dir, ordered by generation.
func _load_run(dir_path: String) -> Array:
	var out: Array = []
	var d := DirAccess.open(dir_path)
	if d == null:
		printerr("cannot open run dir: %s" % dir_path)
		return out
	for name in d.get_files():
		if name.begins_with("snap_") and name.ends_with(".bin"):
			var snap := SnapshotReader.load_from(dir_path.path_join(name))
			if snap != null:
				out.append(snap)
	out.sort_custom(func(a, b): return a.generation < b.generation)
	print("loaded %d snapshots from %s" % [out.size(), dir_path])
	return out


## Newest data/runs/<id>/ (by modified time) that contains at least one snap_*.bin. "" if none.
func _newest_run_dir() -> String:
	var runs := "res://../data/runs"
	var d := DirAccess.open(runs)
	if d == null:
		return ""
	var best := ""
	var best_mtime := -1
	for sub in d.get_directories():
		var p := runs.path_join(sub)
		var has_snap := false
		var sd := DirAccess.open(p)
		if sd != null:
			for f in sd.get_files():
				if f.begins_with("snap_") and f.ends_with(".bin"):
					has_snap = true
					break
		if has_snap:
			var m := FileAccess.get_modified_time(p)
			if m > best_mtime:
				best_mtime = m
				best = p
	return best


## Index of the snapshot whose generation == want; -1/unknown → the last one.
func _pick_index(want: int) -> int:
	if want >= 0:
		for i in _snaps.size():
			if int(_snaps[i].generation) == want:
				return i
	return _snaps.size() - 1


# ──────────────────────────── screenshot (verification) ────────────────────────────

func _take_shot(out_path: String) -> void:
	# Two frames so the TileMap, overlay texture and organism _draw() have all flushed to the viewport.
	await RenderingServer.frame_post_draw
	await RenderingServer.frame_post_draw
	var tex := get_viewport().get_texture()
	var img: Image = tex.get_image() if tex != null else null
	if img == null:
		printerr("shot FAILED: no viewport image (headless has no GPU; run windowed for --shot)")
		get_tree().quit(1)
		return
	var err := img.save_png(out_path)
	if err != OK:
		printerr("shot FAILED: save_png(%s) err %d" % [out_path, err])
		get_tree().quit(1)
		return
	print("shot OK — %s (%dx%d), gen=%d" % [out_path, img.get_width(), img.get_height(), _snaps[_idx].generation])
	get_tree().quit()


# ──────────────────────────── helpers ────────────────────────────

## Read a `--flag value` pair from the user command line (args after `--`). Returns `fallback` if absent.
func _arg_value(flag: String, fallback: String = "") -> String:
	var args := OS.get_cmdline_user_args()
	var idx := args.find(flag)
	if idx != -1 and idx + 1 < args.size():
		return args[idx + 1]
	return fallback


## True if a valueless flag (e.g. `--check`) is present on the user command line.
func _has_flag(flag: String) -> bool:
	return OS.get_cmdline_user_args().has(flag)


func _hash01(x: int, y: int, k: int) -> float:
	var h := (x * 73856093) ^ (y * 19349663) ^ ((k + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
