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
##   --iso                 Render the ecosystem isometrically (CPU diamonds); orthographic is the default.
##   --live [--seed N]     Drive an OPEN-ENDED run live via the LiveSim gdext node (build the cdylib first:
##                         cargo build --manifest-path crates/godot-sim/Cargo.toml). Space pauses, ▶ steps.
##   --view specimen       Open the L-system specimen view (instead of the ecosystem view) for --shot.
##   --focus <i>           With --view specimen: focus specimen i (0 baseline, 1.. edits) for --shot.
## With no args and a display, auto-discovers the newest data/runs/<id>/ that holds snap_*.bin.
##
## Keys (windowed): Space pause · V toggle ecosystem/specimen · Tab cycle specimen · D cycle layer ·
##   ,/. step · 1/2/3 zoom scope · wheel zoom · arrows pan.
## Mouse (windowed): drag = pan · hover = cell/plant tooltip · click = pin detail (cell stats + genome
##   ontology in ecosystem; focus + detail a plant in specimen).

## Load the reader by path, not via a `class_name` global: that registry is only populated by an editor
## import pass, so a bare identifier is unresolved under a fresh `--headless` run. `preload` needs no cache.
const SnapshotReader := preload("res://snapshot.gd")
const Organisms := preload("res://organisms.gd")
const Lsystem := preload("res://lsystem.gd")
const Timeline := preload("res://timeline.gd")
const Iso := preload("res://iso.gd")
const IsoGround := preload("res://iso_ground.gd")
const DataLayerShader := preload("res://data_layer.gdshader")

const OVERLAY_NAMES := ["off", "density", "allele_freq", "fitness", "soil_moisture", "soil_nutrients", "soil_ph"]
# The 5 species-genome traits, in canonical order (matches the core's Trait::ALL). Iterate THIS, never the
# specimens.json Dictionary's keys, so the readout order is stable (inv #3 hygiene, even in UI).
const TRAIT_KEYS := ["growth_rate", "reflectance", "drought_tolerance", "fecundity", "kill_switch_linkage"]
const FRAME_SECONDS := 0.45  # seconds per snapshot when playing a run
const TARGET_FIELD_PX := 880.0  # the field is scaled to about this many pixels on its long side
# Zoom "scopes": magnification presets the viewport switches between (keys 1/2/3; SPEC §W10).
const SCOPES := [{"name": "field", "zoom": 1.0}, {"name": "patch", "zoom": 2.6}, {"name": "cells", "zoom": 6.0}]
const ZOOM_MIN := 0.6
const ZOOM_MAX := 12.0
# --live (P5): drive the sim live via the LiveSim gdext node instead of replaying snapshot files.
const LIVE_GRID := Vector2i(48, 48)  # snapshot grid pulled from LiveSim each tick
const LIVE_STEP := 1  # generations advanced per tick (a FIXED integer — deterministic cadence, inv #3)
const LIVE_HISTORY := 150  # rolling snapshot buffer kept for the timeline / scrubbing

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
var _iso = null  # iso.gd transform instance when --iso is active; null = orthographic (default)
var _iso_ground: Node2D  # CPU-diamond ground + data overlay (iso mode only)
var _vignette: CanvasLayer  # screen-space edge darkening (ecosystem view only)
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
var _focus: int = 0  # which specimen (index into _specimen_list()) is focused in the specimen view
var _specimen_panel: Control
var _specimen_picker: OptionButton
var _trait_rows: Array = []  # [{bar:ProgressBar, value:Label, delta:Label}] one per TRAIT_KEYS entry
var _prev_button: Button
var _next_button: Button
var _speed_slider: HSlider
var _scope_buttons: Array = []  # 3 Buttons, one per SCOPES preset (field/patch/cells)
var _gen_slider: HSlider
var _gen_label: Label
var _frame_seconds: float = FRAME_SECONDS  # runtime playback interval (the speed slider mutates this)
var _syncing: bool = false  # re-entrancy guard so programmatic widget updates don't recurse via signals
var _timeline: Control  # full-width bottom generation timeline (timeline.gd)
var _tooltip: PanelContainer
var _tooltip_label: Label
var _detail_panel: PanelContainer
var _detail_box: VBoxContainer
var _dragging: bool = false  # left-button drag-pan in progress
var _drag_moved: float = 0.0  # accumulated drag distance (to tell a click from a drag)
var _live = null  # LiveSim gdext node when --live is active (drives an open-ended run); null = file replay
var _intervention_panel: Control  # live-mode CRISPR injection UI
var _cas_picker: OptionButton
var _locus_picker: OptionButton
var _guide_edit: LineEdit
var _inject_status: Label
var _cas_ids: Array = []  # cas variant id per _cas_picker item
var _locus_ids: Array = []  # locus id per _locus_picker item
var _injections: Array = []  # [{generation, applied}] for the timeline markers


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

	# ---- Resolve the snapshots to show: --live drives the sim via LiveSim; else a --run dir / --snap / auto.
	if _has_flag("--live") and _setup_live():
		pass  # _setup_live populated _snaps from a live LiveSim reset
	else:
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
	if _live != null and _has_flag("--inject"):  # optional: fire one demo injection for --shot verification
		_live.step(20)
		_live_advance()
		_on_inject_pressed()
	if _arg_value("--view") == "specimen":  # optional: open the L-system specimen view for --shot
		_set_view_mode(1)
		var focus_arg := _arg_value("--focus")  # optional: focus a specific specimen (0=baseline, 1..=edits)
		if focus_arg != "" and not _specimen_list().is_empty():
			_focus = clampi(int(focus_arg), 0, _specimen_list().size() - 1)
			if _specimen_picker != null:
				_specimen_picker.select(_focus)
			_on_specimen_selected(_focus)  # re-run readout/emphasis/frame for the chosen specimen
	var inspect_arg := _arg_value("--inspect")  # "x,y": pin the cell detail panel for --shot
	if inspect_arg != "" and _view_mode == 0 and not _snaps.is_empty():
		var parts := inspect_arg.split(",")
		if parts.size() == 2:
			var cx := int(parts[0])
			var cy := int(parts[1])
			var snap = _snaps[_idx]
			if cx >= 0 and cy >= 0 and cx < snap.width and cy < snap.height:
				var i: int = cy * snap.width + cx
				_fill_detail("Cell (%d, %d)" % [cx, cy], _cell_lines(snap, i))

	# Headless render smoke (gate): build the scene + specimen plants + the detail panel, prove it all
	# constructs without a GPU, quit.
	if _has_flag("--check"):
		_render_specimens()  # exercise the L-system build path headlessly (catches GDScript errors)
		_fill_detail("(check)", ["density 0.0"])  # exercise the detail/ontology rendering path
		print("render scene OK — %d snapshots, %d specimens, cell=%d, grid %dx%d" % [
			_snaps.size(), _specimen_list().size(), int(_cell), _snaps[0].width, _snaps[0].height])
		get_tree().quit()
		return

	if shot_path != "":
		await _take_shot(shot_path)
		return

	# Playback timer (windowed): in --live, advance the LiveSim each tick; else play through the file run.
	_timer = Timer.new()
	_timer.wait_time = _frame_seconds
	_timer.timeout.connect(_live_advance if _live != null else _advance)
	add_child(_timer)
	if _live != null or _snaps.size() > 1:
		_timer.start()


# ──────────────────────────── live mode (P5): drive the sim via the LiveSim gdext node ────────────────────

## Load the LiveSim GDExtension at RUNTIME (so the default project + gate stay extension-free), instantiate
## it, reset, and seed _snaps with the gen-0 snapshot. Returns false (→ fall back to file replay) if the
## cdylib is not built or the extension fails to load. The renderer only CALLS LiveSim (inv #2: biology in Rust).
func _setup_live() -> bool:
	var dylib := ProjectSettings.globalize_path("res://../crates/godot-sim/target/debug/libgodot_sim.dylib")
	if not FileAccess.file_exists(dylib):
		dylib = ProjectSettings.globalize_path("res://../crates/godot-sim/target/debug/libgodot_sim.so")
	if not FileAccess.file_exists(dylib):
		printerr("--live needs the LiveSim cdylib. Build it:  cargo build --manifest-path crates/godot-sim/Cargo.toml")
		return false
	if not ClassDB.class_exists("LiveSim"):
		var ext := "user://gene_sim_live.gdextension"
		var f := FileAccess.open(ext, FileAccess.WRITE)
		if f == null:
			printerr("--live: cannot write the runtime .gdextension")
			return false
		f.store_string(("[configuration]\nentry_symbol = \"gdext_rust_init\"\ncompatibility_minimum = 4.6\n"
			+ "[libraries]\nmacos.debug = \"%s\"\nmacos.release = \"%s\"\nlinux.debug = \"%s\"\nlinux.release = \"%s\"\n")
			% [dylib, dylib, dylib, dylib])
		f.close()
		var st := GDExtensionManager.load_extension(ext)
		if not ClassDB.class_exists("LiveSim"):
			printerr("--live: failed to load LiveSim extension (status %d)" % st)
			return false
	_live = ClassDB.instantiate("LiveSim")
	_live.reset(int(_arg_value("--seed", "42")))
	var snap = SnapshotReader.parse_bytes(_live.snapshot(LIVE_GRID.x, LIVE_GRID.y))
	if snap == null:
		printerr("--live: LiveSim.snapshot() returned unparseable bytes")
		_live = null
		return false
	_snaps = [snap]
	print("LIVE MODE — driving LiveSim (open-ended run, %d gen/tick)" % LIVE_STEP)
	return true


## One live tick: advance the sim a fixed integer number of generations, pull the new snapshot, append it to
## the rolling history, and display it. Deterministic cadence (inv #3: LIVE_STEP is a fixed integer).
func _live_advance() -> void:
	if _paused or _view_mode != 0 or _live == null:
		return
	_live.step(LIVE_STEP)
	var snap = SnapshotReader.parse_bytes(_live.snapshot(LIVE_GRID.x, LIVE_GRID.y))
	if snap == null:
		return
	_snaps.append(snap)
	if _snaps.size() > LIVE_HISTORY:
		_snaps.pop_front()
	if _timeline != null:
		var gens: Array = []
		for s in _snaps:
			gens.append(s.generation)
		_timeline.setup(gens)
		_timeline.set_markers(_injections)
	_show(_snaps.size() - 1)


## Live-mode CRISPR intervention UI (P6): pick a Cas variant / locus / guide and Inject. The renderer only
## REQUESTS the edit (LiveSim.apply_edit) — the core applies it (authoritative PAM/score/gate stay in crispr,
## inv #2); the species-granular EditAction carries no organism handle (inv #6). Hidden unless --live.
func _build_intervention_ui(ui: CanvasLayer) -> void:
	_intervention_panel = _dark_panel(0.55)
	var fs := _field_screen_size()
	_intervention_panel.position = Vector2(maxf(240.0, fs.x - 274.0), 70.0)
	_intervention_panel.custom_minimum_size = Vector2(262, 0)
	_intervention_panel.visible = (_live != null)
	ui.add_child(_intervention_panel)

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 5)
	_intervention_panel.add_child(col)

	var hdr := Label.new()
	hdr.text = "⌖ CRISPR Intervention"
	hdr.add_theme_font_size_override("font_size", 15)
	hdr.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
	col.add_child(hdr)

	var r1 := HBoxContainer.new()
	r1.add_child(_dim_label("Cas:"))
	_cas_picker = OptionButton.new()
	r1.add_child(_cas_picker)
	col.add_child(r1)

	var r2 := HBoxContainer.new()
	r2.add_child(_dim_label("Locus:"))
	_locus_picker = OptionButton.new()
	r2.add_child(_locus_picker)
	col.add_child(r2)

	var r3 := HBoxContainer.new()
	r3.add_child(_dim_label("Guide:"))
	_guide_edit = LineEdit.new()
	_guide_edit.text = "ACGTGGACGTTTTAGGCCGG"
	_guide_edit.custom_minimum_size = Vector2(160, 0)
	_guide_edit.text_submitted.connect(_on_guide_submitted)
	r3.add_child(_guide_edit)
	col.add_child(r3)

	var inject := Button.new()
	inject.text = "Inject edit"
	inject.pressed.connect(_on_inject_pressed)
	col.add_child(inject)

	_inject_status = _dim_label("")
	_inject_status.custom_minimum_size = Vector2(250, 0)
	_inject_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	col.add_child(_inject_status)

	if _live != null:
		for v in _live.cas_variants():
			_cas_picker.add_item(str((v as Dictionary).get("name", "cas")))
			_cas_ids.append(int((v as Dictionary).get("id", 0)))
		for l in _live.loci():
			_locus_picker.add_item(str((l as Dictionary).get("name", "locus")))
			_locus_ids.append(int((l as Dictionary).get("id", 0)))


func _on_guide_submitted(_text: String) -> void:
	_on_inject_pressed()


## Request a CRISPR edit from the running LiveSim, show the outcome, and mark it on the timeline.
func _on_inject_pressed() -> void:
	if _live == null or _cas_picker.selected < 0 or _locus_picker.selected < 0:
		return
	var cas_id := int(_cas_ids[_cas_picker.selected])
	var locus_id := int(_locus_ids[_locus_picker.selected])
	var outcome: Dictionary = _live.apply_edit(cas_id, locus_id, _guide_edit.text)
	var applied := bool(outcome.get("applied", false))
	_inject_status.text = ("✓ " if applied else "✗ ") + str(outcome.get("detail", ""))
	_inject_status.add_theme_color_override(
		"font_color", Color(0.5, 0.92, 0.52) if applied else Color(0.96, 0.55, 0.5))
	_injections.append({"generation": int(outcome.get("generation", 0)), "applied": applied})
	if _timeline != null:
		_timeline.set_markers(_injections)


# ──────────────────────────── scene construction (read-only presentation) ────────────────────────────

func _build_scene() -> void:
	var first = _snaps[0]
	var w: int = first.width
	var h: int = first.height
	_cell = maxf(3.0, floorf(TARGET_FIELD_PX / float(max(w, h))))
	_field_px = Vector2(float(w) * _cell, float(h) * _cell)

	# Isometric mode (P3): a CPU-diamond ground + iso-projected organisms, instead of the ortho TileMap +
	# axis-aligned shader overlay. Behind --iso; orthographic stays the default. Read-only presentation (#2).
	if _has_flag("--iso"):
		_iso = Iso.new()
		var b: Rect2 = _iso.field_bounds(w, h, _cell)
		_iso.origin = -b.position + Vector2(20, 20)  # shift the negative-x left edge fully on-screen
	print("ecosystem mode: %s" % ("ISOMETRIC (--iso)" if _iso != null else "orthographic"))

	# Ecosystem layers live under _world so the whole view can be toggled off for the specimen view.
	_world = Node2D.new()
	add_child(_world)

	if _iso != null:
		_iso_ground = IsoGround.new()
		_iso_ground.setup(w, h, _cell, _iso)
		_world.add_child(_iso_ground)
	else:
		_terrain = _build_terrain(w, h, int(_cell))
		_world.add_child(_terrain)

	_overlay = Sprite2D.new()
	_overlay.centered = false
	_overlay.texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST  # one data texel = one crisp cell block
	var mat := ShaderMaterial.new()
	mat.shader = DataLayerShader
	_overlay.visible = (_iso == null)  # iso draws the data overlay into the diamonds instead
	_world.add_child(_overlay)

	_organisms = Organisms.new()
	_organisms.set_iso(_iso)
	_world.add_child(_organisms)

	# L-system specimen view (S4.5) — hidden until toggled.
	_specimen_root = Node2D.new()
	_specimen_root.visible = false
	add_child(_specimen_root)

	# A camera framing the whole field; S4.4 adds zoom scopes on top of this.
	_cam = Camera2D.new()
	_cam.position = _field_center()
	add_child(_cam)
	_cam.make_current()  # must be in-tree first

	# Screen-space edge vignette (S4). It sits ABOVE the world (Node2D, effective layer 0) but BELOW the UI:
	# there is no integer between 0 and 1, so we set explicit layers — vignette=1, UI=2 — and rely on layer
	# order, not tree order. Hidden in the specimen view (clean dark backdrop there).
	_build_vignette()

	# HUD + controls on their own CanvasLayer (layer 2) so they sit above the vignette and ignore the camera.
	var ui := CanvasLayer.new()
	ui.layer = 2
	add_child(ui)
	# UI positions key off the on-screen field size (the iso diamond bbox under --iso, else the ortho rect).
	var field_screen := _field_screen_size()
	_build_hud(ui, field_screen)
	_build_controls(ui, field_screen)
	_build_specimen_ui(ui, field_screen)
	_build_interaction_ui(ui)
	_build_timeline(ui)
	_build_intervention_ui(ui)

	# Size the window to the field (+ margin) when we have a display.
	if DisplayServer.get_name() != "headless":
		# Extra bottom margin so the two-row control bar (S3) is fully on-screen, not clipped.
		var win := (_field_screen_size() + Vector2(40, 150)).max(Vector2(760, 600))
		DisplayServer.window_set_size(Vector2i(int(win.x), int(win.y)))
	RenderingServer.set_default_clear_color(Color(0.06, 0.08, 0.07))


## A tiled grass field: a small procedurally-generated atlas of green shades placed with hash variation.
## This is the "2D TileMap ecosystem view of one scope" (a field) — pure backdrop, no biology.
func _build_terrain(w: int, h: int, cell: int) -> TileMapLayer:
	var shades := [
		Color(0.18, 0.33, 0.17), Color(0.21, 0.37, 0.19),
		Color(0.16, 0.30, 0.16), Color(0.24, 0.41, 0.22),
		Color(0.19, 0.34, 0.18), Color(0.15, 0.27, 0.15),  # last = slightly darker soil patch (not black)
	]
	var n := shades.size()
	var atlas := Image.create(cell * n, cell, false, Image.FORMAT_RGBA8)
	for ti in n:
		for yy in cell:
			for xx in cell:
				# Per-pixel speckle + a per-column green streak so tiles read as grass blades, not flat blocks.
				var speckle := (_hash01(xx, yy, ti) - 0.5) * 0.04
				var blade := (_hash01(xx, ti * 7 + 1, 3) - 0.5) * 0.05  # vertical blade streaks (mostly green)
				var c: Color = shades[ti]
				atlas.set_pixel(ti * cell + xx, yy, Color(
					clampf(c.r + speckle, 0.0, 1.0),
					clampf(c.g + speckle + blade * 1.4, 0.0, 1.0),
					clampf(c.b + speckle, 0.0, 1.0)))
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


## Screen-space edge vignette (layer 1, between world and UI). MOUSE_FILTER_IGNORE so it never eats the
## wheel-zoom / button clicks that pass to the world or the UI above it.
func _build_vignette() -> void:
	_vignette = CanvasLayer.new()
	_vignette.layer = 1
	add_child(_vignette)
	var tr := TextureRect.new()
	tr.texture = _vignette_texture()
	tr.set_anchors_preset(Control.PRESET_FULL_RECT)
	tr.stretch_mode = TextureRect.STRETCH_SCALE
	tr.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	tr.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_vignette.add_child(tr)


## A radial gradient: transparent centre → soft dark frame at the edges/corners (CPU image, headless-safe).
func _vignette_texture() -> ImageTexture:
	var n := 256
	var img := Image.create(n, n, false, Image.FORMAT_RGBA8)
	var c := Vector2(n - 1, n - 1) * 0.5
	var maxd := c.length()
	for y in n:
		for x in n:
			var d := (Vector2(x, y) - c).length() / maxd  # 0 centre … 1 corner
			var a := pow(clampf((d - 0.5) / 0.5, 0.0, 1.0), 1.6) * 0.5
			img.set_pixel(x, y, Color(0, 0, 0, a))
	return ImageTexture.create_from_image(img)


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

## A bottom control bar (two rows): row 1 = view toggle / play-pause / step / data-layer picker; row 2 =
## playback-speed slider / zoom-scope buttons / generation scrubber. All change VIEW state only — no biology
## (invariant #2). Mirrors the keyboard shortcuts so the UI is discoverable.
func _build_controls(ui: CanvasLayer, field_px: Vector2) -> void:
	var bar := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.5)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(6)
	bar.add_theme_stylebox_override("panel", sb)
	bar.position = Vector2(12, field_px.y + 16)
	ui.add_child(bar)

	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 6)
	bar.add_child(rows)

	# Row 1 — view / playback / step / layer.
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	rows.add_child(row)

	_view_button = Button.new()
	_view_button.text = "View: Ecosystem"
	_view_button.pressed.connect(_on_view_pressed)
	row.add_child(_view_button)

	_play_button = Button.new()
	_play_button.text = "⏸ Pause"
	_play_button.pressed.connect(_on_play_pressed)
	row.add_child(_play_button)

	_prev_button = Button.new()
	_prev_button.text = "◀"
	_prev_button.pressed.connect(_step_rel.bind(-1))
	row.add_child(_prev_button)

	_next_button = Button.new()
	_next_button.text = "▶"
	_next_button.pressed.connect(_step_rel.bind(1))
	row.add_child(_next_button)

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

	# Row 2 — speed / scope / generation scrubber.
	var row2 := HBoxContainer.new()
	row2.add_theme_constant_override("separation", 8)
	rows.add_child(row2)

	row2.add_child(_dim_label("Speed:"))
	_speed_slider = HSlider.new()
	_speed_slider.min_value = 0.5  # 0.5× … 4× playback speed
	_speed_slider.max_value = 4.0
	_speed_slider.step = 0.1
	_speed_slider.value = 1.0
	_speed_slider.custom_minimum_size = Vector2(90, 0)
	_speed_slider.value_changed.connect(_on_speed_changed)
	row2.add_child(_speed_slider)

	row2.add_child(_dim_label("  Scope:"))
	var group := ButtonGroup.new()
	group.allow_unpress = true  # _scope_label() buckets continuous zoom; no preset may be active
	for i in SCOPES.size():
		var b := Button.new()
		b.text = str(SCOPES[i]["name"]).capitalize()
		b.toggle_mode = true
		b.button_group = group
		b.pressed.connect(_set_scope.bind(i))
		row2.add_child(b)
		_scope_buttons.append(b)

	row2.add_child(_dim_label("  Gen:"))
	_gen_slider = HSlider.new()
	_gen_slider.min_value = 0
	_gen_slider.max_value = maxi(1, _snaps.size() - 1)
	_gen_slider.step = 1
	_gen_slider.value = _idx
	_gen_slider.custom_minimum_size = Vector2(150, 0)
	_gen_slider.editable = _snaps.size() > 1
	_gen_slider.value_changed.connect(_on_gen_slider)
	row2.add_child(_gen_slider)

	_gen_label = _dim_label("")
	row2.add_child(_gen_label)

	_sync_controls()


## A small dimmed label used as an inline caption in the control bar.
func _dim_label(text: String) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_color_override("font_color", Color(0.82, 0.86, 0.82))
	return l


func _on_speed_changed(v: float) -> void:
	# Higher slider = faster playback = shorter interval.
	_frame_seconds = FRAME_SECONDS / maxf(0.1, v)
	if _timer != null:
		_timer.wait_time = _frame_seconds
	_sync_controls()


func _on_gen_slider(v: float) -> void:
	if _syncing or _view_mode != 0 or _snaps.size() < 2:
		return
	_paused = true
	_update_play_button()
	_show(int(round(v)))


## Push current state INTO the row-2 widgets without re-triggering their signals (re-entrancy guarded).
func _sync_controls() -> void:
	_syncing = true
	var eco := _view_mode == 0
	if _gen_slider != null:
		_gen_slider.editable = eco and _snaps.size() > 1
		_gen_slider.set_value_no_signal(_idx)
	if _gen_label != null:
		if eco and not _snaps.is_empty():
			_gen_label.text = "gen %d  [%d/%d]" % [_snaps[_idx].generation, _idx + 1, _snaps.size()]
		else:
			_gen_label.text = ""
	if _prev_button != null:
		_prev_button.disabled = not eco
	if _next_button != null:
		_next_button.disabled = not eco
	if _speed_slider != null:
		_speed_slider.editable = eco
	if _timeline != null:
		_timeline.set_index(_idx)
	_sync_scope_buttons()
	_syncing = false


## Reflect the current zoom scope in the toggle buttons (one pressed, or none at in-between zooms).
func _sync_scope_buttons() -> void:
	if _scope_buttons.is_empty():
		return
	var active := _scope_label()  # 'field' / 'patch' / 'cells'
	for i in _scope_buttons.size():
		(_scope_buttons[i] as Button).set_pressed_no_signal(str(SCOPES[i]["name"]) == active)


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
	if _vignette != null:
		_vignette.visible = (m == 0)
	if _detail_panel != null:
		_detail_panel.visible = false  # clear stale inspection on view switch
	if _tooltip != null:
		_tooltip.visible = false
	if _timeline != null:
		_timeline.visible = (m == 0)  # the timeline indexes snapshots, irrelevant in specimen view
	if _intervention_panel != null:
		_intervention_panel.visible = (_live != null and m == 0)
	if _view_button != null:
		_view_button.text = "View: Specimen" if m == 1 else "View: Ecosystem"
	if _layer_picker != null:
		_layer_picker.disabled = (m == 1)
	if _specimen_panel != null:
		_specimen_panel.visible = (m == 1)
	if m == 1:
		_render_specimens()  # also repopulates the picker
		_update_trait_readout()
		_emphasise_focus()
		_frame_focused_specimen()
	else:
		_frame_world()
	_sync_controls()  # enable/disable scrubber + step for the new mode
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
	_populate_specimen_picker()  # keep the A1 selector in sync with the rebuilt plant row


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
		"leaf_aspect": 0.5 + drought * 0.2,  # drought → narrower/sturdier leaves
		"jitter_deg": 3.0 + ksl * 9.0,  # kill-switch linkage → unruliness
		"seed": seed_val,
		"flower_count": int(round(fec * 4.0)),  # fecundity → more flowers (0..4)
		"petal_count": 5,
		"branch_base": Color(0.36, 0.24, 0.12),
		"branch_tip": Color(0.30, 0.55, 0.20).lerp(Color(0.66, 0.62, 0.20), drought),
		"leaf_color": Color(0.85, 0.55, 0.20).lerp(Color(0.35, 0.78, 0.30), refl),
		"flower_color": Color(0.95, 0.45, 0.55).lerp(Color(0.98, 0.85, 0.35), refl),
	}


## On-screen size of the ecosystem field — the iso diamond bbox under --iso, else the ortho rectangle.
func _field_screen_size() -> Vector2:
	if _iso != null and not _snaps.is_empty():
		return _iso.field_bounds(_snaps[0].width, _snaps[0].height, _cell).size
	return _field_px


## On-screen centre of the field (camera target).
func _field_center() -> Vector2:
	if _iso != null and not _snaps.is_empty():
		var b: Rect2 = _iso.field_bounds(_snaps[0].width, _snaps[0].height, _cell)
		return b.position + b.size * 0.5
	return _field_px * 0.5


## Grid cell under a world point — inverse iso transform under --iso, else the ortho division.
func _cell_at(world: Vector2) -> Vector2i:
	if _iso != null:
		var c: Vector2 = _iso.screen_to_cell(world, _cell)
		return Vector2i(int(floor(c.x)), int(floor(c.y)))
	return Vector2i(int(floor(world.x / _cell)), int(floor(world.y / _cell)))


func _frame_world() -> void:
	_cam.position = _field_center()
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


# ──────────────────────────── specimen UX panel (A1) ────────────────────────────

## A top-right panel for the specimen view: a picker to focus one specimen + a readout of its 5 trait values
## as bars with a delta-vs-baseline arrow. Reads only the core-exported trait vectors (presentation, inv #2).
func _build_specimen_ui(ui: CanvasLayer, field_px: Vector2) -> void:
	_specimen_panel = PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.5)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(10)
	_specimen_panel.add_theme_stylebox_override("panel", sb)
	_specimen_panel.position = Vector2(maxf(240.0, field_px.x - 304.0), 70.0)
	_specimen_panel.custom_minimum_size = Vector2(288, 0)
	_specimen_panel.visible = false
	ui.add_child(_specimen_panel)

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 6)
	_specimen_panel.add_child(col)

	var header := Label.new()
	header.text = "Specimen"
	header.add_theme_font_size_override("font_size", 16)
	header.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
	col.add_child(header)

	_specimen_picker = OptionButton.new()
	_specimen_picker.item_selected.connect(_on_specimen_selected)
	col.add_child(_specimen_picker)

	var traits_hdr := Label.new()
	traits_hdr.text = "Traits  (vs baseline)"
	traits_hdr.add_theme_font_size_override("font_size", 12)
	traits_hdr.add_theme_color_override("font_color", Color(0.7, 0.78, 0.7))
	col.add_child(traits_hdr)

	_trait_rows.clear()
	for key in TRAIT_KEYS:
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 6)
		col.add_child(row)

		var name_lbl := Label.new()
		name_lbl.text = str(key)
		name_lbl.custom_minimum_size = Vector2(118, 0)
		name_lbl.add_theme_font_size_override("font_size", 11)
		name_lbl.add_theme_color_override("font_color", Color(0.86, 0.9, 0.86))
		row.add_child(name_lbl)

		var bar := ProgressBar.new()
		bar.min_value = 0.0
		bar.max_value = 1.0
		bar.show_percentage = false
		bar.custom_minimum_size = Vector2(56, 12)
		var bg := StyleBoxFlat.new()
		bg.bg_color = Color(1, 1, 1, 0.10)
		bg.set_corner_radius_all(3)
		var fill := StyleBoxFlat.new()
		fill.bg_color = Color(0.45, 0.78, 0.45)
		fill.set_corner_radius_all(3)
		bar.add_theme_stylebox_override("background", bg)
		bar.add_theme_stylebox_override("fill", fill)
		row.add_child(bar)

		var val_lbl := Label.new()
		val_lbl.custom_minimum_size = Vector2(40, 0)
		val_lbl.add_theme_font_size_override("font_size", 11)
		val_lbl.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
		row.add_child(val_lbl)

		var delta_lbl := Label.new()
		delta_lbl.custom_minimum_size = Vector2(54, 0)
		delta_lbl.add_theme_font_size_override("font_size", 11)
		row.add_child(delta_lbl)

		_trait_rows.append({"bar": bar, "value": val_lbl, "delta": delta_lbl})


## Refill the picker from the current specimen list (baseline first). Clamps _focus into range.
func _populate_specimen_picker() -> void:
	if _specimen_picker == null:
		return
	_specimen_picker.clear()
	var list := _specimen_list()
	for spec in list:
		_specimen_picker.add_item(str((spec as Dictionary).get("label", "specimen")))
	_focus = clampi(_focus, 0, maxi(0, list.size() - 1))
	if list.size() > 0:
		_specimen_picker.select(_focus)


func _on_specimen_selected(idx: int) -> void:
	_focus = idx
	_update_trait_readout()
	_emphasise_focus()
	_frame_focused_specimen()


## Rewrite the trait bars/values/deltas for the focused specimen (vs baseline = list index 0).
func _update_trait_readout() -> void:
	if _trait_rows.is_empty():
		return
	var list := _specimen_list()
	if list.is_empty():
		return
	var focused: Dictionary = (list[clampi(_focus, 0, list.size() - 1)] as Dictionary).get("traits", {})
	var base: Dictionary = (list[0] as Dictionary).get("traits", {})
	for i in TRAIT_KEYS.size():
		var key: String = TRAIT_KEYS[i]
		var v := clampf(float(focused.get(key, 0.0)), 0.0, 1.0)
		var b := clampf(float(base.get(key, 0.0)), 0.0, 1.0)
		var row: Dictionary = _trait_rows[i]
		(row["bar"] as ProgressBar).value = v
		(row["value"] as Label).text = "%.3f" % v
		var d := v - b
		var delta: Label = row["delta"]
		if absf(d) < 0.0005:
			delta.text = "="
			delta.add_theme_color_override("font_color", Color(0.6, 0.62, 0.6))
		elif d > 0.0:
			delta.text = "▲ %+.2f" % d
			delta.add_theme_color_override("font_color", Color(0.42, 0.9, 0.46))
		else:
			delta.text = "▼ %+.2f" % d
			delta.add_theme_color_override("font_color", Color(0.95, 0.5, 0.45))


## Brighten the focused plant; dim the others. Holders are added in _specimen_list() order by _render_specimens.
func _emphasise_focus() -> void:
	if _specimen_root == null:
		return
	var kids := _specimen_root.get_children()
	for i in kids.size():
		(kids[i] as Node2D).modulate = Color.WHITE if i == _focus else Color(1, 1, 1, 0.3)


## Centre the camera on the focused specimen's plant (falls back to framing the whole row).
func _frame_focused_specimen() -> void:
	var kids := _specimen_root.get_children()
	if _focus < 0 or _focus >= kids.size():
		_frame_specimens()
		return
	var holder := kids[_focus] as Node2D
	var plant := holder.get_child(0) as Node2D  # the Lsystem is the first child (label is second)
	if plant == null or not plant.has_method("bounds"):
		_frame_specimens()
		return
	var pb: Rect2 = plant.bounds()
	if pb.size == Vector2.ZERO:
		_frame_specimens()
		return
	var wb := Rect2(holder.position + pb.position, pb.size).grow(60.0)
	var vp := get_viewport_rect().size
	var z := minf(vp.x / wb.size.x, vp.y / wb.size.y) * 0.9
	_cam.zoom = Vector2(z, z)
	_cam.position = wb.position + wb.size * 0.5


# ──────────────────────────── mouse interaction: hover tooltip + click detail ────────────────────────────

## Build the hover tooltip (follows the cursor) and the pinned detail panel (set on click). Both READ-ONLY:
## they surface per-cell snapshot data + the species genome's ontology tags the core exported (invariant #2).
func _build_interaction_ui(ui: CanvasLayer) -> void:
	_tooltip = _dark_panel(0.62)
	_tooltip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_tooltip.visible = false
	_tooltip_label = Label.new()
	_tooltip_label.add_theme_font_size_override("font_size", 12)
	_tooltip_label.add_theme_color_override("font_color", Color(0.95, 0.98, 0.95))
	_tooltip.add_child(_tooltip_label)
	ui.add_child(_tooltip)

	_detail_panel = _dark_panel(0.55)
	_detail_panel.position = Vector2(12, 112)
	_detail_panel.custom_minimum_size = Vector2(250, 0)
	_detail_panel.visible = false
	_detail_box = VBoxContainer.new()
	_detail_box.add_theme_constant_override("separation", 3)
	_detail_panel.add_child(_detail_box)
	ui.add_child(_detail_panel)


## Full-width bottom timeline: generation axis + play-head + click-to-seek (timeline.gd).
func _build_timeline(ui: CanvasLayer) -> void:
	_timeline = Timeline.new()
	_timeline.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	_timeline.offset_left = 8
	_timeline.offset_right = -8
	_timeline.offset_top = -54
	_timeline.offset_bottom = -6
	_timeline.seek.connect(_on_timeline_seek)
	ui.add_child(_timeline)
	var gens: Array = []
	for s in _snaps:
		gens.append(s.generation)
	_timeline.setup(gens)


func _on_timeline_seek(i: int) -> void:
	if _view_mode != 0 or _snaps.is_empty():
		return
	_paused = true
	_update_play_button()
	_show(i)


## A reusable translucent rounded panel (used by the tooltip + detail panel).
func _dark_panel(alpha: float) -> PanelContainer:
	var p := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, alpha)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(8)
	p.add_theme_stylebox_override("panel", sb)
	return p


## Per-cell / per-plant summary that tracks the cursor. Hidden when the cursor is over nothing relevant.
func _update_tooltip() -> void:
	if _tooltip == null:
		return
	var world := get_global_mouse_position()
	var text := ""
	if _view_mode == 0 and not _snaps.is_empty():
		var _cc := _cell_at(world)
		var cx := _cc.x
		var cy := _cc.y
		var snap = _snaps[_idx]
		if cx >= 0 and cy >= 0 and cx < snap.width and cy < snap.height:
			var i: int = cy * snap.width + cx
			text = "(%d,%d)  d %.2f  a %.2f  f %.2f  moist %.2f" % [cx, cy, snap.density[i], snap.allele_freq[i], snap.fitness[i], snap.soil_moisture[i]]
	elif _view_mode == 1:
		var hit := _specimen_at(world)
		if hit >= 0:
			text = str((_specimen_list()[hit] as Dictionary).get("label", ""))
	if text == "":
		_tooltip.visible = false
		return
	_tooltip_label.text = text
	_tooltip.visible = true
	_tooltip.position = get_viewport().get_mouse_position() + Vector2(16, 14)


## Index of the specimen whose plant bounds contain `world`, else -1.
func _specimen_at(world: Vector2) -> int:
	if _specimen_root == null:
		return -1
	var kids := _specimen_root.get_children()
	for i in kids.size():
		var holder := kids[i] as Node2D
		var plant := holder.get_child(0) as Node2D
		if plant != null and plant.has_method("bounds"):
			var pb: Rect2 = plant.bounds()
			if Rect2(holder.position + pb.position, pb.size).grow(40.0).has_point(world):
				return i
	return -1


## Left-click (not a drag): pin the detail panel for the clicked cell (ecosystem) or focus + detail the
## clicked plant (specimen).
func _on_click() -> void:
	var world := get_global_mouse_position()
	if _view_mode == 0:
		if _snaps.is_empty():
			return
		var snap = _snaps[_idx]
		var _cc := _cell_at(world)
		var cx := _cc.x
		var cy := _cc.y
		if cx >= 0 and cy >= 0 and cx < snap.width and cy < snap.height:
			var i: int = cy * snap.width + cx
			_fill_detail("Cell (%d, %d)" % [cx, cy], _cell_lines(snap, i))
		else:
			_detail_panel.visible = false
	else:
		var hit := _specimen_at(world)
		if hit >= 0:
			_focus = hit
			if _specimen_picker != null:
				_specimen_picker.select(_focus)
			_on_specimen_selected(_focus)
			_fill_detail(str((_specimen_list()[hit] as Dictionary).get("label", "specimen")), [])


## The per-cell stat lines (population channels + R1.0 soil channels) for the detail panel.
func _cell_lines(snap, i: int) -> Array:
	return [
		"density        %.3f" % snap.density[i],
		"allele_freq    %.3f" % snap.allele_freq[i],
		"fitness        %.3f" % snap.fitness[i],
		"soil moisture  %.3f" % snap.soil_moisture[i],
		"soil nutrients %.3f" % snap.soil_nutrients[i],
		"soil pH        %.3f" % snap.soil_ph[i],
	]


## Rewrite the detail panel: a title, optional stat lines, then the species-genome ontology (track-B prep).
func _fill_detail(title: String, stat_lines: Array) -> void:
	for c in _detail_box.get_children():
		c.queue_free()
	_detail_box.add_child(_detail_label(title, 15, Color(0.96, 0.99, 0.96)))
	for s in stat_lines:
		_detail_box.add_child(_detail_label(str(s), 12, Color(0.9, 0.94, 0.9)))
	var loci: Array = (_specimens.get("genome", {}) as Dictionary).get("loci", [])
	if not loci.is_empty():
		_detail_box.add_child(_detail_label("Genome (species) · ontology", 12, Color(0.7, 0.78, 0.7)))
		for l in loci:
			var ld: Dictionary = l
			var go: Array = ld.get("go_refs", [])
			_detail_box.add_child(_detail_label(
				"• %s   %s   %s" % [ld.get("name", ""), ld.get("so_term", ""), ", ".join(go)],
				11, Color(0.86, 0.9, 0.86)))
	_detail_panel.visible = true


func _detail_label(text: String, size: int, color: Color) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_color", color)
	return l


# ──────────────────────────── per-snapshot update ────────────────────────────

func _show(i: int) -> void:
	if i < 0 or i >= _snaps.size():
		return
	_idx = i
	var snap = _snaps[i]
	_organisms.set_snapshot(snap, _cell)
	if _iso_ground != null:
		_iso_ground.set_snapshot(snap, _overlay_mode)  # iso draws ground + data overlay as diamonds
	_update_overlay(snap)
	_refresh_hud()
	_sync_controls()


## Feed the per-cell data texture (R=density, G=allele_freq, B=fitness) to the data-layer shader and select
## the active channel via the `layer` uniform. The colormap + alpha live in data_layer.gdshader (GPU). Under
## --iso the shader sprite stays hidden — the iso ground node renders the overlay into the diamonds instead.
func _update_overlay(snap) -> void:
	if _iso != null:
		return  # iso_ground (fed in _show) owns the overlay in isometric mode
	if _overlay_mode == 0:
		_overlay.visible = false
		return
	_overlay.visible = true
	_overlay.texture = ImageTexture.create_from_image(snap.to_data_image())
	_overlay.scale = Vector2(_cell, _cell)
	var mat := _overlay.material as ShaderMaterial
	if mat != null:
		# layer 0..2 sample the population texture; 3..5 sample the soil texture (R1.0 made visible).
		mat.set_shader_parameter("layer", _overlay_mode - 1)
		mat.set_shader_parameter("soil_tex", ImageTexture.create_from_image(snap.to_soil_image()))


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
	var live_tag := ("● LIVE   " if _live != null else "")
	_hud.text = "%sgen %d   pop %d   grid %dx%d   layer: %s%s   scope: %s (×%.1f)   [%d/%d]" % [
		live_tag, snap.generation, snap.population, snap.width, snap.height,
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
	_sync_scope_buttons()


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
	if event is InputEventMouseButton:
		# Wheel = continuous zoom; left button = drag-pan / click-to-inspect.
		if event.button_index == MOUSE_BUTTON_WHEEL_UP and event.pressed:
			_set_zoom(_cam.zoom.x * 1.15)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN and event.pressed:
			_set_zoom(_cam.zoom.x / 1.15)
		elif event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_dragging = true
				_drag_moved = 0.0
			else:
				if _dragging and _drag_moved < 6.0:  # a click, not a drag → inspect
					_on_click()
				_dragging = false
		return

	if event is InputEventMouseMotion:
		if _dragging and (event.button_mask & MOUSE_BUTTON_MASK_LEFT):
			_cam.position -= event.relative / _cam.zoom.x  # drag the map under the cursor
			_drag_moved += event.relative.length()
			if _tooltip != null:
				_tooltip.visible = false
		else:
			_update_tooltip()
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
		KEY_TAB:
			# Cycle the focused specimen (specimen view only); guard empty list (no div-by-zero).
			if _view_mode == 1 and not _specimen_list().is_empty():
				_focus = (_focus + 1) % _specimen_list().size()
				if _specimen_picker != null:
					_specimen_picker.select(_focus)
				_on_specimen_selected(_focus)
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
