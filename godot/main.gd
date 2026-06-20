extends Node2D
## gene-sim thin UI entry point — 2D ecosystem view of one scope (S4.3).
##
## INVARIANT #2 (STOP THE LINE if violated): this renderer READS sim snapshots only. It must NEVER compute
## genotype→phenotype or any biology — all of that lives in the Rust core (crates/genome, crates/sim-core).
## GDScript here only loads/plays snapshot data and draws it: a tiled field backdrop, organism dot markers,
## and toggleable per-cell data-layer SHADERS (density/allele_freq/fitness) over a zoomable viewport (S4.4).
##
## CLI (args after `--`):
##   --snap <file.bin>     Headless: parse one snapshot and report its header (S4.2 gate path).
##   --run  <dir>          Play snap_*.bin in <dir> as a live run (windowed; auto-advances, loops).
##   --shot <out.png>      Render one frame to a PNG then quit (needs a display; for verification).
##   --gen  <n>            With --run/--shot: pick the snapshot whose generation == n (else the last).
##   --layer <0..3>        With --shot: preselect the data layer (0 off / 1 density / 2 allele / 3 fitness).
##   --zoom  <f>           With --shot: preset the zoom scope (1 field … 6 cells).
## With no args and a display, auto-discovers the newest data/runs/<id>/ that holds snap_*.bin.
##
## Keys (windowed): Space pause · D cycle layer · ,/. step · 1/2/3 zoom scope · wheel zoom · arrows pan.

## Load the reader by path, not via a `class_name` global: that registry is only populated by an editor
## import pass, so a bare identifier is unresolved under a fresh `--headless` run. `preload` needs no cache.
const SnapshotReader := preload("res://snapshot.gd")
const Organisms := preload("res://organisms.gd")
const Lsystem := preload("res://lsystem.gd")
const DataLayerShader := preload("res://data_layer.gdshader")

const OVERLAY_NAMES := ["off", "density", "allele_freq", "fitness"]
const FRAME_SECONDS := 0.45  # seconds per snapshot when playing a run
const TARGET_FIELD_PX := 880.0  # the field is scaled to about this many pixels on its long side
# Zoom "scopes": magnification presets the viewport switches between (keys 1/2/3; SPEC §W10).
const SCOPES := [{"name": "field", "zoom": 1.0}, {"name": "patch", "zoom": 2.6}, {"name": "cells", "zoom": 6.0}]
const ZOOM_MIN := 0.6
const ZOOM_MAX := 12.0

var _snaps: Array = []  # parsed snapshot.gd instances, ordered by generation
var _idx: int = 0
var _cell: float = 12.0
var _overlay_mode: int = 1  # index into OVERLAY_NAMES; 1 = density
var _paused: bool = false
var _view_mode: int = 0  # 0 = ecosystem, 1 = specimen (L-system plants)
var _specimens: Dictionary = {}  # parsed specimens.json: {baseline:{...}, edits:[...]}
var _run_dir: String = ""
var _field_px := Vector2.ZERO

var _world: Node2D  # holds the ecosystem layers (terrain/overlay/organisms)
var _specimen_root: Node2D  # holds the L-system plant specimens
var _terrain: TileMapLayer
var _overlay: Sprite2D
var _organisms: Node2D
var _cam: Camera2D
var _hud: Label
var _legend_label: Label
var _legend: Control
var _timer: Timer
var _view_button: Button
var _play_button: Button
var _layer_picker: OptionButton
var _specimen_bounds := Rect2()


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
		_run_dir = run_dir
		_snaps = _load_run(run_dir)
	elif snap_path != "":
		var one := SnapshotReader.load_from(snap_path)
		if one != null:
			_snaps = [one]
	else:
		var newest := _newest_run_dir()
		if newest != "":
			print("auto-discovered run: %s" % newest)
			_run_dir = newest
			_snaps = _load_run(newest)
	if _run_dir != "":
		_specimens = _load_specimens(_run_dir)

	if _snaps.is_empty():
		# Headless smoke (S4.1) with nothing to show, or no run found: boot cleanly and exit.
		if DisplayServer.get_name() == "headless":
			print("headless smoke OK")
		else:
			print("no snapshots to render (pass --run <dir> or --snap <file>)")
		get_tree().quit()
		return

	_build_scene()
	var layer_arg := _arg_value("--layer")  # optional: preselect a data layer (0 off … 3 fitness) for --shot
	if layer_arg != "":
		_overlay_mode = clampi(int(layer_arg), 0, OVERLAY_NAMES.size() - 1)
	_idx = _pick_index(int(_arg_value("--gen", "-1")))
	_show(_idx)
	var zoom_arg := _arg_value("--zoom")  # optional: preset zoom scope for --shot
	if zoom_arg != "":
		_set_zoom(float(zoom_arg))
	if _arg_value("--view") == "specimen":  # optional: open the L-system specimen view for --shot
		_set_view_mode(1)

	# Headless render smoke (gate): build the scene + specimen plants, prove it constructs without a GPU, quit.
	if _has_flag("--check"):
		_render_specimens()  # exercise the L-system build path headlessly (catches GDScript errors)
		print("render scene OK — %d snapshots, %d specimens, cell=%d, grid %dx%d" % [
			_snaps.size(), _specimen_list().size(), int(_cell), _snaps[0].width, _snaps[0].height])
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
	_field_px = Vector2(float(w) * _cell, float(h) * _cell)

	# Ecosystem layers live under _world so the whole view can be toggled off for the specimen view.
	_world = Node2D.new()
	add_child(_world)

	_terrain = _build_terrain(w, h, int(_cell))
	_world.add_child(_terrain)

	_overlay = Sprite2D.new()
	_overlay.centered = false
	_overlay.texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST  # one data texel = one crisp cell block
	var mat := ShaderMaterial.new()
	mat.shader = DataLayerShader
	_overlay.material = mat
	_world.add_child(_overlay)

	_organisms = Organisms.new()
	_world.add_child(_organisms)

	# L-system specimen view (S4.5) — hidden until toggled.
	_specimen_root = Node2D.new()
	_specimen_root.visible = false
	add_child(_specimen_root)

	# A camera framing the whole field; S4.4 adds zoom scopes on top of this.
	_cam = Camera2D.new()
	_cam.position = _field_px * 0.5
	add_child(_cam)
	_cam.make_current()  # must be in-tree first

	# HUD + controls on their own CanvasLayer so they ignore the world camera.
	var ui := CanvasLayer.new()
	add_child(ui)
	_build_hud(ui, _field_px)
	_build_controls(ui, _field_px)

	# Size the window to the field (+ margin) when we have a display.
	if DisplayServer.get_name() != "headless":
		var win := (_field_px + Vector2(40, 96)).max(Vector2(720, 540))
		DisplayServer.window_set_size(Vector2i(int(win.x), int(win.y)))
	RenderingServer.set_default_clear_color(Color(0.06, 0.08, 0.07))


## A tiled grass field: a small procedurally-generated atlas of green shades placed with hash variation.
## This is the "2D TileMap ecosystem view of one scope" (a field) — pure backdrop, no biology.
func _build_terrain(w: int, h: int, cell: int) -> TileMapLayer:
	var shades := [
		Color(0.16, 0.30, 0.15), Color(0.19, 0.34, 0.17),
		Color(0.14, 0.27, 0.14), Color(0.21, 0.37, 0.19),
		Color(0.17, 0.31, 0.16), Color(0.11, 0.22, 0.12),  # last = darker soil patch
	]
	var n := shades.size()
	var atlas := Image.create(cell * n, cell, false, Image.FORMAT_RGBA8)
	for ti in n:
		for yy in cell:
			for xx in cell:
				# subtle per-pixel jitter so tiles read as grass, not flat blocks.
				var j := (_hash01(xx, yy, ti) - 0.5) * 0.05
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
			# Shade from a COARSE block so the field reads as grassy patches, not per-tile checker noise,
			# with an occasional single-cell speckle for texture.
			var ti := int(_hash01(int(x / 3.0), int(y / 3.0), 7) * float(n))
			if _hash01(x, y, 11) > 0.86:
				ti = int(_hash01(x, y, 13) * float(n))
			layer.set_cell(Vector2i(x, y), sid, Vector2i(ti % n, 0))
	return layer


# ──────────────────────────── HUD + legend ────────────────────────────

## Build the status line (in a translucent panel) and the colormap legend (bottom-left).
func _build_hud(ui: CanvasLayer, field_px: Vector2) -> void:
	var panel := PanelContainer.new()
	panel.position = Vector2(12, 10)
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.42)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(8)
	panel.add_theme_stylebox_override("panel", sb)
	ui.add_child(panel)
	_hud = Label.new()
	_hud.add_theme_font_size_override("font_size", 17)
	_hud.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
	panel.add_child(_hud)

	# Colormap legend: the active layer's name + the inferno gradient bar (low → high).
	_legend = PanelContainer.new()
	_legend.position = Vector2(12, maxf(120.0, field_px.y - 52.0))
	var lsb := StyleBoxFlat.new()
	lsb.bg_color = Color(0.0, 0.0, 0.0, 0.42)
	lsb.set_corner_radius_all(6)
	lsb.set_content_margin_all(8)
	_legend.add_theme_stylebox_override("panel", lsb)
	ui.add_child(_legend)
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 3)
	_legend.add_child(col)
	_legend_label = Label.new()
	_legend_label.add_theme_font_size_override("font_size", 14)
	_legend_label.add_theme_color_override("font_color", Color(0.9, 0.94, 0.9))
	col.add_child(_legend_label)
	var bar := TextureRect.new()
	bar.texture = _legend_texture()
	bar.custom_minimum_size = Vector2(208, 12)
	bar.stretch_mode = TextureRect.STRETCH_SCALE
	col.add_child(bar)


## 1-D inferno gradient texture matching data_layer.gdshader (low → high).
func _legend_texture() -> ImageTexture:
	var n := 208
	var img := Image.create(n, 12, false, Image.FORMAT_RGBA8)
	for x in n:
		var c := _inferno(float(x) / float(n - 1))
		for y in 12:
			img.set_pixel(x, y, c)
	return ImageTexture.create_from_image(img)


## CPU mirror of the shader's inferno ramp (for the legend bar only).
func _inferno(t: float) -> Color:
	var c0 := Color(0.05, 0.03, 0.15)
	var c1 := Color(0.35, 0.07, 0.43)
	var c2 := Color(0.75, 0.18, 0.33)
	var c3 := Color(0.96, 0.49, 0.14)
	var c4 := Color(0.99, 0.92, 0.55)
	if t < 0.25:
		return c0.lerp(c1, t / 0.25)
	elif t < 0.5:
		return c1.lerp(c2, (t - 0.25) / 0.25)
	elif t < 0.75:
		return c2.lerp(c3, (t - 0.5) / 0.25)
	return c3.lerp(c4, (t - 0.75) / 0.25)


# ──────────────────────────── controls bar ────────────────────────────

## A bottom control bar: view toggle, play/pause, step, data-layer picker. All change VIEW state only —
## no biology (invariant #2). Mirrors the keyboard shortcuts so the UI is discoverable.
func _build_controls(ui: CanvasLayer, field_px: Vector2) -> void:
	var bar := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.5)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(6)
	bar.add_theme_stylebox_override("panel", sb)
	bar.position = Vector2(12, field_px.y + 16)
	ui.add_child(bar)

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	bar.add_child(row)

	_view_button = Button.new()
	_view_button.text = "View: Ecosystem"
	_view_button.pressed.connect(_on_view_pressed)
	row.add_child(_view_button)

	_play_button = Button.new()
	_play_button.text = "⏸ Pause"
	_play_button.pressed.connect(_on_play_pressed)
	row.add_child(_play_button)

	var prev := Button.new()
	prev.text = "◀"
	prev.pressed.connect(_step_rel.bind(-1))
	row.add_child(prev)

	var nxt := Button.new()
	nxt.text = "▶"
	nxt.pressed.connect(_step_rel.bind(1))
	row.add_child(nxt)

	var lbl := Label.new()
	lbl.text = "  Layer:"
	lbl.add_theme_color_override("font_color", Color(0.9, 0.94, 0.9))
	row.add_child(lbl)

	_layer_picker = OptionButton.new()
	for nm in OVERLAY_NAMES:
		_layer_picker.add_item(nm)
	_layer_picker.selected = _overlay_mode
	_layer_picker.item_selected.connect(_on_layer_selected)
	row.add_child(_layer_picker)


func _on_view_pressed() -> void:
	_set_view_mode(1 - _view_mode)


func _on_play_pressed() -> void:
	_paused = not _paused
	_update_play_button()
	_refresh_hud()


func _on_layer_selected(idx: int) -> void:
	_overlay_mode = idx
	if _view_mode == 0:
		_show(_idx)


func _update_play_button() -> void:
	if _play_button != null:
		_play_button.text = "▶ Play" if _paused else "⏸ Pause"


## Step one snapshot relative (ecosystem view only); pauses playback.
func _step_rel(delta: int) -> void:
	if _view_mode != 0 or _snaps.is_empty():
		return
	_paused = true
	_update_play_button()
	_show((_idx + delta + _snaps.size()) % _snaps.size())


# ──────────────────────────── specimen (L-system) view ────────────────────────────

func _set_view_mode(m: int) -> void:
	_view_mode = m
	_world.visible = (m == 0)
	_specimen_root.visible = (m == 1)
	if _view_button != null:
		_view_button.text = "View: Specimen" if m == 1 else "View: Ecosystem"
	if _layer_picker != null:
		_layer_picker.disabled = (m == 1)
	if m == 1:
		_render_specimens()
		_frame_specimens()
	else:
		_frame_world()
	_refresh_hud()


## Flat list of specimens to draw: baseline first, then each edited genome.
func _specimen_list() -> Array:
	var out: Array = []
	if _specimens.has("baseline"):
		out.append(_specimens["baseline"])
	if _specimens.has("edits"):
		for e in _specimens["edits"]:
			out.append(e)
	return out


## Build one L-system plant per specimen, laid out in a row with a caption. The plant geometry comes from
## the core-exported trait vector via _plant_params_from_traits (presentation mapping — no biology, inv #2).
func _render_specimens() -> void:
	for c in _specimen_root.get_children():
		c.queue_free()
	var list := _specimen_list()
	_specimen_bounds = Rect2()
	if list.is_empty():
		return
	var spacing := 300.0
	var union := Rect2()
	var has_union := false
	for i in list.size():
		var spec: Dictionary = list[i]
		var holder := Node2D.new()
		holder.position = Vector2(float(i) * spacing, 0.0)
		_specimen_root.add_child(holder)

		var plant: Node2D = Lsystem.new()
		holder.add_child(plant)
		plant.build(_plant_params_from_traits(spec.get("traits", {}), i + 1))

		var label := Label.new()
		label.text = str(spec.get("label", "specimen"))
		label.add_theme_font_size_override("font_size", 15)
		label.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
		label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.9))
		label.add_theme_constant_override("outline_size", 6)
		label.size = Vector2(220, 0)
		label.position = Vector2(-110, 18)
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		holder.add_child(label)

		var pb: Rect2 = plant.bounds()
		var wb := Rect2(holder.position + pb.position, pb.size).merge(
			Rect2(holder.position + Vector2(-110, 18), Vector2(220, 44)))
		if has_union:
			union = union.merge(wb)
		else:
			union = wb
			has_union = true
	_specimen_bounds = union


## Map a core-exported trait vector (each in [0,1]) to L-system visual params. PRESENTATION ONLY (the
## genome→trait biology already ran in the Rust core; this is trait→appearance, the renderer's job).
func _plant_params_from_traits(t: Dictionary, seed_val: int) -> Dictionary:
	var growth := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
	var refl := clampf(float(t.get("reflectance", 0.5)), 0.0, 1.0)
	var drought := clampf(float(t.get("drought_tolerance", 0.5)), 0.0, 1.0)
	var fec := clampf(float(t.get("fecundity", 0.5)), 0.0, 1.0)
	var ksl := clampf(float(t.get("kill_switch_linkage", 0.0)), 0.0, 1.0)
	return {
		"iterations": 4 + int(round(growth * 2.0)),  # growth → size/complexity (4..6)
		"angle_deg": 16.0 + refl * 32.0,  # reflectance → branch spread
		"segment_len": 5.0 + growth * 9.0,  # growth → reach
		"len_falloff": 0.80 + drought * 0.12,  # drought tolerance → sturdier taper
		"thickness": 3.0 + growth * 3.5,
		"leaf_size": 2.0 + fec * 6.5,  # fecundity → bigger/more prominent leaves
		"jitter_deg": 3.0 + ksl * 9.0,  # kill-switch linkage → unruliness
		"seed": seed_val,
		"branch_base": Color(0.36, 0.24, 0.12),
		"branch_tip": Color(0.30, 0.55, 0.20).lerp(Color(0.66, 0.62, 0.20), drought),
		"leaf_color": Color(0.85, 0.55, 0.20).lerp(Color(0.35, 0.78, 0.30), refl),
	}


func _frame_world() -> void:
	_cam.position = _field_px * 0.5
	_cam.zoom = Vector2.ONE


func _frame_specimens() -> void:
	if _specimen_bounds.size == Vector2.ZERO:
		_cam.position = Vector2.ZERO
		_cam.zoom = Vector2.ONE
		return
	var vp := get_viewport_rect().size
	var z := minf(vp.x / _specimen_bounds.size.x, vp.y / _specimen_bounds.size.y) * 0.82
	_cam.zoom = Vector2(z, z)
	_cam.position = _specimen_bounds.position + _specimen_bounds.size * 0.5


# ──────────────────────────── per-snapshot update ────────────────────────────

func _show(i: int) -> void:
	if i < 0 or i >= _snaps.size():
		return
	_idx = i
	var snap = _snaps[i]
	_organisms.set_snapshot(snap, _cell)
	_update_overlay(snap)
	_refresh_hud()


## Feed the per-cell data texture (R=density, G=allele_freq, B=fitness) to the data-layer shader and select
## the active channel via the `layer` uniform. The colormap + alpha live in data_layer.gdshader (GPU).
func _update_overlay(snap) -> void:
	if _overlay_mode == 0:
		_overlay.visible = false
		return
	_overlay.visible = true
	_overlay.texture = ImageTexture.create_from_image(snap.to_data_image())
	_overlay.scale = Vector2(_cell, _cell)
	var mat := _overlay.material as ShaderMaterial
	if mat != null:
		mat.set_shader_parameter("layer", _overlay_mode - 1)  # 0 density / 1 allele_freq / 2 fitness


func _refresh_hud() -> void:
	if _hud == null:
		return
	# Specimen view: caption the L-system plants; hide the data legend.
	if _view_mode == 1:
		var edits := _specimen_list().size() - 1
		if edits >= 0:
			_hud.text = "specimen view — baseline + %d edited genome(s); each plant's shape is its trait vector   [V back]" % maxi(0, edits)
		else:
			_hud.text = "specimen view — no specimens.json (re-run harness with --specimens)   [V back]"
		if _legend != null:
			_legend.visible = false
		return
	if _snaps.is_empty():
		return
	var snap = _snaps[_idx]
	_hud.text = "gen %d   pop %d   grid %dx%d   layer: %s%s   scope: %s (×%.1f)   [%d/%d]" % [
		snap.generation, snap.population, snap.width, snap.height,
		OVERLAY_NAMES[_overlay_mode], ("  (paused)" if _paused else ""),
		_scope_label(), _cam.zoom.x, _idx + 1, _snaps.size()]
	if _legend != null:
		_legend.visible = _overlay_mode != 0
		_legend_label.text = "%s   low → high" % OVERLAY_NAMES[_overlay_mode]


## Name the current zoom scope from the magnification (HUD only).
func _scope_label() -> String:
	var z := _cam.zoom.x
	if z < 1.8:
		return "field"
	elif z < 4.2:
		return "patch"
	return "cells"


func _set_zoom(z: float) -> void:
	var zc := clampf(z, ZOOM_MIN, ZOOM_MAX)
	_cam.zoom = Vector2(zc, zc)
	_refresh_hud()


## Jump to a named zoom scope preset (keys 1/2/3).
func _set_scope(i: int) -> void:
	_set_zoom(float(SCOPES[i]["zoom"]))


## Pan the camera; step is in world pixels, scaled inversely with zoom so it feels constant on screen.
func _pan(dir: Vector2) -> void:
	_cam.position += dir * (90.0 / _cam.zoom.x)
	_refresh_hud()


func _advance() -> void:
	if _paused or _view_mode != 0 or _snaps.size() < 2:
		return
	_idx = (_idx + 1) % _snaps.size()
	_show(_idx)


# ──────────────────────────── input (windowed) ────────────────────────────

func _unhandled_input(event: InputEvent) -> void:
	# Mouse wheel = continuous zoom (viewport scope).
	if event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			_set_zoom(_cam.zoom.x * 1.15)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			_set_zoom(_cam.zoom.x / 1.15)
		return

	if not (event is InputEventKey and event.pressed and not event.echo):
		return
	match event.keycode:
		KEY_SPACE:
			_paused = not _paused
			_update_play_button()
			_refresh_hud()
		KEY_V:
			_set_view_mode(1 - _view_mode)
		KEY_D:
			if _view_mode == 0:
				_overlay_mode = (_overlay_mode + 1) % OVERLAY_NAMES.size()
				if _layer_picker != null:
					_layer_picker.selected = _overlay_mode
				_show(_idx)
		KEY_PERIOD:
			_paused = true
			_show((_idx + 1) % _snaps.size())
		KEY_COMMA:
			_paused = true
			_show((_idx - 1 + _snaps.size()) % _snaps.size())
		KEY_1:
			_set_scope(0)
		KEY_2:
			_set_scope(1)
		KEY_3:
			_set_scope(2)
		KEY_LEFT:
			_pan(Vector2.LEFT)
		KEY_RIGHT:
			_pan(Vector2.RIGHT)
		KEY_UP:
			_pan(Vector2.UP)
		KEY_DOWN:
			_pan(Vector2.DOWN)
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


## Load specimens.json (baseline + edited trait vectors) from a run dir, if present. Read-only — the trait
## values were computed by the core; the renderer only reads them (invariant #2). {} if absent/malformed.
func _load_specimens(dir_path: String) -> Dictionary:
	var path := dir_path.path_join("specimens.json")
	if not FileAccess.file_exists(path):
		return {}
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return {}
	var data: Variant = JSON.parse_string(f.get_as_text())
	if typeof(data) != TYPE_DICTIONARY:
		printerr("specimens.json: not a JSON object")
		return {}
	var edits: Array = data.get("edits", [])
	print("loaded specimens: baseline + %d edits from %s" % [edits.size(), dir_path])
	return data


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
