extends CanvasLayer
## Pre-sim MAIN MENU overlay (ADR-012 E4): the player sets the world — seed (or random), GPS latitude/longitude,
## average temperature, season, population — before the run starts. Emits `start_run(cfg)` on Start; main.gd then
## reconfigures the LiveSim + reseeds in place (no relaunch).
##
## Renderer-only (inv #2): the PREVIEW row (day length / insolation / temperature) is computed by the CORE via
## LiveSim.preview_climate — this script never computes climate itself. Loaded by path (no class_name; ADR-010):
##   const MainMenu := preload("res://main_menu.gd")

signal start_run(cfg)  # { seed, lat, lon, temp, season, entities, mission, species, roster:[{stem,count}], containment }
# LOAD-GEM-REPLAY v2: the player picks a saved emergent-run gem (data/runs/gems/<…>.json — GITIGNORED, so an
# ABSOLUTE filesystem path, NOT res://) to replay. main.gd reads + composes the run from the gem's config and asks
# the CORE to resolve its mid-run CRISPR edit schedule (no biology here — inv #2; the menu only hands the path).
signal load_gem(path)  # absolute path to a gem .json the player chose in the Load-Gem FileDialog
# STARTERS GALLERY: the menu's "Starters Gallery" button opens the RollerCoaster-Tycoon-style scenario picker
# (gallery.gd) over data/presets/starters/. main.gd shows it; the gallery routes Play back to this menu's proven
# Start path (gen-1 fresh run) or LiveSim.load_session (gen-N checkpoint). The menu only requests the open.
signal open_gallery()

const SEASONS := ["Spring", "Summer", "Autumn", "Winter"]
# SP-2: the multi-species ROSTER master list. [label, file stem under data/species/]. Each stem is the FILE name
# under res://data/species/<stem>.json that bakes a SpeciesSpec; the CORE derives the species key from the spec
# (so the plant's stem is "default" but its core key is "" — the renderer only moves the inert file stem, inv #2).
# Verified present under both data/species/ and godot/data/species/.
const ALL_SPECIES := [
	["Plant (abstract)", "default"],
	["E. coli K-12 core", "ecoli"],
	["Bdellovibrio (predator)", "bdellovibrio"],
	["Mycoplasma", "mycoplasma"],
	["Bacillus", "bacillus"],
	["Pseudomonas", "pseudomonas"],
	["Staphylococcus", "staph"],
	["Cutibacterium", "cutibacterium"],
	["Aspergillus niger", "aspergillus-niger"],
	["Penicillium", "penicillium"],
]
# ContainmentLevel ladder (ADR-019 S3, ISO-14644): index = the ordinal pushed to LiveSim.set_containment / the core
# sim_core::ContainmentLevel (0 Sealed · 1 Clean · 2 Lab · 3 Open). Default Sealed (0) = empty schedule = hash-neutral.
# Mirrors main.gd CONTAINMENT_LABELS + the godot-sim set_containment ladder.
const CONTAINMENT_LABELS := ["🔒 Sealed (OFF)", "Clean (ISO 7)", "Lab (ISO 8)", "☣ Open"]
const ROSTER_DEFAULT_COUNT := 1000  # per-species starting count default (the player overrides per row)
const ROSTER_COUNT_MAX := 20000

var _live: Object = null
var _seed: int = 42
var _season: int = 0
var _mission_default: bool = false

var _seed_edit: LineEdit = null
var _random_chk: CheckBox = null
var _lat: HSlider = null
var _lon: HSlider = null
var _temp: HSlider = null
var _entities: HSlider = null
var _lat_val: Label = null
var _lon_val: Label = null
var _temp_val: Label = null
var _ent_val: Label = null
var _season_btn: Button = null
var _mission_chk: CheckBox = null
# SP-2 ROSTER state: the dynamic species-row composer (replaces the single _species_btn). Each row is
# {species: OptionButton over ALL_SPECIES, count: SpinBox, row: HBoxContainer}. Row order is the canonical,
# load-bearing key (menu-row order == roster Vec order == SpeciesId order in the core, inv #3) — a reorder is a
# DIFFERENT but still-deterministic run.
var _roster_box: VBoxContainer = null
var _roster_rows: Array = []
var _containment_btn: OptionButton = null
var _preview: Label = null


## Called by main.gd before the overlay is added to the tree. `live` is the LiveSim (for the core preview);
## `p_mission` seeds the mission checkbox (the --mission CLI flag), default off.
func setup(live: Object, p_seed: int, p_mission: bool = false) -> void:
	_live = live
	_seed = p_seed
	_mission_default = p_mission


func _ready() -> void:
	layer = 50  # above the HUD layers
	_build()
	_refresh_values()
	_update_preview()


func _build() -> void:
	# Dim full-screen backdrop that also blocks clicks reaching the (paused) sim behind the menu.
	var dim := ColorRect.new()
	dim.color = Color(0.02, 0.03, 0.03, 0.82)
	dim.set_anchors_preset(Control.PRESET_FULL_RECT)
	dim.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(dim)

	# A CenterContainer over the whole screen reliably centers the card (PRESET_CENTER mis-places a
	# size-to-content PanelContainer).
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	dim.add_child(center)

	# Centered card.
	var card := PanelContainer.new()
	var csb := StyleBoxFlat.new()
	csb.bg_color = Color(0.06, 0.10, 0.08, 0.98)
	csb.set_corner_radius_all(12)
	csb.set_content_margin_all(22)
	csb.border_width_left = 1
	csb.border_width_top = 1
	csb.border_width_right = 1
	csb.border_width_bottom = 1
	csb.border_color = Color(0.2, 0.5, 0.32, 0.7)
	card.add_theme_stylebox_override("panel", csb)
	center.add_child(card)

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 10)
	col.custom_minimum_size = Vector2(440, 0)
	card.add_child(col)

	var title := Label.new()
	title.text = "GENE-SIM  ·  NEW RUN"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color(0.7, 0.95, 0.75))
	col.add_child(title)
	var sub := Label.new()
	sub.text = "Set the world, then press START."
	sub.add_theme_color_override("font_color", Color(0.6, 0.72, 0.62))
	col.add_child(sub)

	# Item 3: one-click QUICK START — load the bundled "Primordial Soil" starter preset (a trophic-realistic
	# multi-species roster + env + containment) into the composer below, so a legible multi-species map is one click
	# away (the way to actually SEE the per-species size contrast on the field). Renderer-only (inv #2): it reads
	# inert preset JSON via FileAccess + fills the existing widgets; the CORE still builds every species from the
	# roster at reset. Degrades gracefully (a push_warning, form untouched) if the preset is absent / malformed.
	var starter := Button.new()
	starter.text = "📂  Load Starter — “Primordial Soil”"
	starter.tooltip_text = "Fill the form below with the bundled starter preset:\na producer-heavy 4-species soil community (plant · E. coli · Bacillus · Bdellovibrio)."
	starter.pressed.connect(_on_load_starter)
	col.add_child(starter)

	# LOAD-GEM-REPLAY v2: replay a saved emergent-run gem (the discovery harness writes them to the GITIGNORED
	# data/runs/gems/). The button opens a FILESYSTEM FileDialog (absolute path — gems are NOT under res://); on a
	# pick the menu emits `load_gem(path)` and main.gd composes the run + replays the gem's mid-run CRISPR edit
	# schedule (resolved by the CORE, inv #2). The menu only moves the chosen file path.
	var load_gem_btn := Button.new()
	load_gem_btn.text = "💎  Load Gem — replay a saved discovery"
	load_gem_btn.tooltip_text = "Open a saved emergent-run gem (data/runs/gems/*.json) and replay it,\nincluding its mid-run CRISPR edit schedule (resolved by the core)."
	load_gem_btn.pressed.connect(_on_load_gem_pressed)
	col.add_child(load_gem_btn)

	# STARTERS GALLERY: open the scenario picker over the promoted starter library (gen-1 fresh-config starters +
	# gen-N checkpoint sessions). A RollerCoaster-Tycoon-style list + a description/preview/scrub panel. Renderer-
	# only (inv #2): the menu just requests the open; the gallery moves inert JSON + drives existing #[func]s.
	var gallery_btn := Button.new()
	gallery_btn.text = "🗺️  Starters Gallery — scenario picker"
	gallery_btn.tooltip_text = "Browse the saved starter library: pick a scenario, scrub its recorded timeline,\nthen Play (gen-1 = a fresh run · checkpoint = restored at gen N with its edits)."
	gallery_btn.pressed.connect(_on_open_gallery_pressed)
	col.add_child(gallery_btn)

	col.add_child(_sep())

	# --- SEED row: a random toggle + a fixed-value field.
	var seed_row := HBoxContainer.new()
	seed_row.add_theme_constant_override("separation", 8)
	_random_chk = CheckBox.new()
	_random_chk.text = "Random seed"
	_random_chk.toggled.connect(_on_random_toggled)
	seed_row.add_child(_random_chk)
	_seed_edit = LineEdit.new()
	_seed_edit.text = str(_seed)
	_seed_edit.custom_minimum_size = Vector2(190, 0)
	_seed_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	seed_row.add_child(_seed_edit)
	var reroll := Button.new()
	reroll.text = "⟳"
	reroll.tooltip_text = "Roll a new random seed"
	reroll.pressed.connect(_on_reroll)
	seed_row.add_child(reroll)
	col.add_child(_labeled("SEED", seed_row))

	# --- ENVIRONMENT sliders (each updates the core preview live).
	_lat = _add_slider(col, "Latitude", -90.0, 90.0, 1.0, 0.0)
	_lat_val = _last_value_label
	_lon = _add_slider(col, "Longitude", -180.0, 180.0, 1.0, 0.0)
	_lon_val = _last_value_label
	_temp = _add_slider(col, "Avg temperature", 0.0, 1.0, 0.01, 0.5)
	_temp_val = _last_value_label
	_entities = _add_slider(col, "Population", 50.0, 20000.0, 50.0, 1000.0)
	_ent_val = _last_value_label

	# --- SEASON cycle.
	var season_row := HBoxContainer.new()
	season_row.add_theme_constant_override("separation", 8)
	var prev := Button.new()
	prev.text = "<"
	prev.pressed.connect(_on_season_prev)
	season_row.add_child(prev)
	_season_btn = Button.new()
	_season_btn.custom_minimum_size = Vector2(120, 0)
	_season_btn.disabled = true  # display only; the < > cycle it
	season_row.add_child(_season_btn)
	var next := Button.new()
	next.text = ">"
	next.pressed.connect(_on_season_next)
	season_row.add_child(next)
	col.add_child(_labeled("Season", season_row))

	# --- ROSTER (SP-2): compose the run from N species, each with its own starting count. Dynamic add/remove rows
	# over the 10 baked stems. Seeded with ONE default Plant/"default"/1000 row, so the default composer == today's
	# single-plant default (hash-neutral). The renderer assembles {stem,count} dicts — biology stays in core (inv #2).
	_roster_box = VBoxContainer.new()
	_roster_box.add_theme_constant_override("separation", 4)
	col.add_child(_labeled("Roster (species + starting count)", _roster_box))
	_roster_rows = []
	_add_roster_row(0, ROSTER_DEFAULT_COUNT)  # one default Plant row
	var add_btn := Button.new()
	add_btn.text = "+ Add species"
	add_btn.tooltip_text = "Add another species to the roster (spawns at reset; order is load-bearing)"
	add_btn.pressed.connect(_on_add_species)
	col.add_child(add_btn)

	# --- CONTAINMENT (ADR-019 S3): the up-front "design your consortium, then watch it get contaminated" choice.
	# Default Sealed (0) = empty schedule = hash-neutral. The selected ordinal maps 1:1 to ContainmentLevel.
	_containment_btn = OptionButton.new()
	for i in CONTAINMENT_LABELS.size():
		_containment_btn.add_item(CONTAINMENT_LABELS[i], i)
	_containment_btn.selected = 0  # Sealed
	_containment_btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_child(_labeled("Containment", _containment_btn))

	# --- MISSION toggle: off by default = free-play sandbox; on = the suppress-the-zone challenge.
	_mission_chk = CheckBox.new()
	_mission_chk.text = "Mission: suppress the zone"
	_mission_chk.button_pressed = _mission_default
	col.add_child(_mission_chk)

	col.add_child(_sep())

	# --- PREVIEW (core-computed) + START.
	_preview = Label.new()
	_preview.add_theme_color_override("font_color", Color(0.78, 0.9, 0.8))
	col.add_child(_preview)

	var start := Button.new()
	start.text = "▶  START RUN"
	start.custom_minimum_size = Vector2(0, 38)
	start.add_theme_font_size_override("font_size", 16)
	start.pressed.connect(_on_start)
	col.add_child(start)


# ──────────────────────────── ROSTER rows (SP-2 composer) ─────────────────────────────────────────────────

## Append one roster row to _roster_box: [species OptionButton over ALL_SPECIES + count SpinBox + "✕" remove].
## `stem_idx` preselects the species (index into ALL_SPECIES); `count` seeds the SpinBox. The row's widgets are
## tracked in _roster_rows so _on_start can read each (selected stem, count) in row order. Renderer-only (inv #2).
func _add_roster_row(stem_idx: int, count: int) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)

	var species := OptionButton.new()
	for i in ALL_SPECIES.size():
		species.add_item(ALL_SPECIES[i][0], i)
	species.selected = clampi(stem_idx, 0, ALL_SPECIES.size() - 1)
	species.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(species)

	var cnt := SpinBox.new()
	cnt.min_value = 0
	cnt.max_value = ROSTER_COUNT_MAX
	cnt.step = 50
	cnt.value = clampi(count, 0, ROSTER_COUNT_MAX)
	cnt.custom_minimum_size = Vector2(110, 0)
	row.add_child(cnt)

	var rm := Button.new()
	rm.text = "✕"
	rm.tooltip_text = "Remove this species from the roster"
	row.add_child(rm)

	var entry := {"species": species, "count": cnt, "row": row}
	rm.pressed.connect(_on_remove_species.bind(entry))
	_roster_rows.append(entry)
	_roster_box.add_child(row)


## "+ Add species": append a fresh row defaulting to the first stem NOT already in the roster (so a quick compose
## doesn't dup the plant), falling back to index 0 when all are used.
func _on_add_species() -> void:
	var used := {}
	for e in _roster_rows:
		used[int((e["species"] as OptionButton).selected)] = true
	var pick := 0
	for i in ALL_SPECIES.size():
		if not used.has(i):
			pick = i
			break
	_add_roster_row(pick, ROSTER_DEFAULT_COUNT)


## Remove a roster row (frees its widgets + drops it from _roster_rows). Guards a minimum of one row so the run
## always has at least one species to spawn.
func _on_remove_species(entry: Dictionary) -> void:
	if _roster_rows.size() <= 1:
		return  # keep at least one species
	_roster_rows.erase(entry)
	var row: Node = entry["row"]
	if row != null:
		row.queue_free()


# ──────────────────────────── Item 3: "Load Starter" preset ─────────────────────────────────────────────────

const STARTER_PRESET_PATH := "res://data/presets/primordial.json"


## "📂 Load Starter": read the bundled Primordial preset (inert JSON via FileAccess — same res:// path the species
## data rides, inv #2) and PREFILL the composer: rebuild the roster rows from preset.roster (key + count, in the
## load-bearing row order), set the env sliders (seed/lat/lon/temp/season), the Population total, and the
## containment level. The CORE still builds every species at reset — GDScript only moves stems/ints into widgets.
## Degrades gracefully (a warning, the form left as-is) on a missing/malformed preset.
func _on_load_starter() -> void:
	if not FileAccess.file_exists(STARTER_PRESET_PATH):
		push_warning("Load Starter: preset not found at %s (was data/presets staged into res://?)" % STARTER_PRESET_PATH)
		return
	var f := FileAccess.open(STARTER_PRESET_PATH, FileAccess.READ)
	if f == null:
		push_warning("Load Starter: could not open %s" % STARTER_PRESET_PATH)
		return
	var text := f.get_as_text()
	f.close()
	var parsed: Variant = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		push_warning("Load Starter: malformed preset JSON in %s" % STARTER_PRESET_PATH)
		return
	var preset: Dictionary = parsed

	# --- ROSTER: rebuild rows from preset.roster (key → ALL_SPECIES index, + count), in order (row order is the
	# load-bearing SpeciesId key, inv #3). Detach the old rows synchronously so the form never shows old+new at once.
	var roster: Array = preset.get("roster", [])
	if not roster.is_empty():
		for e in _roster_rows:
			var old_row: Node = e.get("row")
			if old_row != null and is_instance_valid(old_row):
				_roster_box.remove_child(old_row)
				old_row.queue_free()
		_roster_rows.clear()
		var total := 0
		for entry in roster:
			var d: Dictionary = entry
			var stem := str(d.get("key", "default"))
			var count := int(d.get("count", ROSTER_DEFAULT_COUNT))
			_add_roster_row(_species_index_for_stem(stem), count)
			total += maxi(0, count)
		# Reflect the composed total on the Population slider so its readout matches the roster (the per-row counts
		# are authoritative for a composed run; this keeps the displayed number honest).
		if total > 0 and _entities != null:
			_entities.value = clampf(float(total), _entities.min_value, _entities.max_value)

	# --- ENV: seed / lat / lon / temp / season (each guarded so a partial preset only sets what it carries).
	var env: Dictionary = preset.get("env", {})
	if env.has("seed") and _seed_edit != null:
		if _random_chk != null:
			_random_chk.button_pressed = false  # a fixed starter seed → turn random off
		_seed_edit.editable = true
		_seed_edit.text = str(int(env.get("seed", _seed)))
	if env.has("lat") and _lat != null:
		_lat.value = float(env.get("lat", _lat.value))
	if env.has("lon") and _lon != null:
		_lon.value = float(env.get("lon", _lon.value))
	if env.has("temp") and _temp != null:
		_temp.value = float(env.get("temp", _temp.value))
	if env.has("season"):
		_season = _season_index(str(env.get("season", "Spring")))

	# --- CONTAINMENT level (ordinal into CONTAINMENT_LABELS / sim_core::ContainmentLevel).
	var cont: Dictionary = preset.get("containment", {})
	if _containment_btn != null and cont.has("level"):
		_containment_btn.selected = clampi(int(cont.get("level", 0)), 0, CONTAINMENT_LABELS.size() - 1)

	_refresh_values()
	_update_preview()


# ──────────────────────────── LOAD-GEM-REPLAY v2: the "Load Gem" FileDialog ────────────────────────────────


## "💎 Load Gem": open a FILESYSTEM FileDialog over the gitignored data/runs/gems/ (gems are NOT under res://, so
## ACCESS_FILESYSTEM + an absolute current_dir). On a pick the dialog emits file_selected → we re-emit `load_gem`
## with the absolute path and free the menu (main.gd composes + replays the gem). Renderer-only (inv #2): the menu
## only moves the file path; the CORE parses the gem + resolves its biology.
func _on_load_gem_pressed() -> void:
	var dlg := FileDialog.new()
	dlg.access = FileDialog.ACCESS_FILESYSTEM
	dlg.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	dlg.title = "Load Gem — data/runs/gems/*.json"
	dlg.use_native_dialog = false
	dlg.add_filter("*.json", "Gem JSON")
	# Best-effort default dir: the gitignored data/runs/gems beside the project (dev) — res:// is godot/, so
	# res://../data/runs/gems globalizes to <repo>/data/runs/gems. Fall back up the tree if it isn't there yet.
	for candidate in ["res://../data/runs/gems", "res://../data/runs", "res://.."]:
		var abs := ProjectSettings.globalize_path(candidate)
		if DirAccess.dir_exists_absolute(abs):
			dlg.current_dir = abs
			break
	dlg.size = Vector2i(760, 500)
	dlg.file_selected.connect(_on_gem_file_selected)
	dlg.canceled.connect(dlg.queue_free)
	add_child(dlg)
	dlg.popup_centered()


## A gem file was chosen: hand its absolute path to main.gd and dismiss the menu (mirrors _on_start's emit+free).
func _on_gem_file_selected(path: String) -> void:
	load_gem.emit(path)
	queue_free()


## "🗺️ Starters Gallery": request the scenario picker + dismiss this menu (main.gd shows the gallery, which routes
## Play back to Start / load_session; Back reopens this menu). Mirrors _on_start's emit+free.
func _on_open_gallery_pressed() -> void:
	open_gallery.emit()
	queue_free()


## ALL_SPECIES index for a file stem (preset roster key → composer row selection). Unknown stem → 0 (the abstract
## plant) so a preset that names an unbundled species still loads SOMETHING rather than failing.
func _species_index_for_stem(stem: String) -> int:
	for i in ALL_SPECIES.size():
		if String(ALL_SPECIES[i][1]) == stem:
			return i
	push_warning("Load Starter: unknown species stem '%s' → defaulting to plant" % stem)
	return 0


## SEASONS index for a season name string (the preset stores "Spring"…"Winter"); unknown → 0 (Spring).
func _season_index(season_name: String) -> int:
	for i in SEASONS.size():
		if SEASONS[i] == season_name:
			return i
	return 0


var _last_value_label: Label = null  # set by _add_slider so the caller can keep the readout label


## A labelled HSlider row with a live numeric readout; returns the slider (and stashes the readout in
## `_last_value_label`). Each drag updates the core preview.
func _add_slider(parent: VBoxContainer, label: String, lo: float, hi: float, step: float, val: float) -> HSlider:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	var s := HSlider.new()
	s.min_value = lo
	s.max_value = hi
	s.step = step
	s.value = val
	s.custom_minimum_size = Vector2(250, 0)
	s.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	s.value_changed.connect(_on_slider_changed)
	row.add_child(s)
	var vl := Label.new()
	vl.custom_minimum_size = Vector2(80, 0)
	vl.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	vl.add_theme_color_override("font_color", Color(0.85, 0.93, 0.85))
	row.add_child(vl)
	_last_value_label = vl
	parent.add_child(_labeled(label, row))
	return s


func _labeled(text: String, body: Control) -> VBoxContainer:
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 2)
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", 11)
	l.add_theme_color_override("font_color", Color(0.55, 0.68, 0.58))
	box.add_child(l)
	box.add_child(body)
	return box


func _sep() -> HSeparator:
	return HSeparator.new()


func _on_slider_changed(_v: float) -> void:
	_refresh_values()
	_update_preview()


func _refresh_values() -> void:
	if _lat_val != null:
		_lat_val.text = "%.0f°" % _lat.value
	if _lon_val != null:
		_lon_val.text = "%.0f°" % _lon.value
	if _temp_val != null:
		_temp_val.text = "%.2f" % _temp.value
	if _ent_val != null:
		_ent_val.text = "%d" % int(_entities.value)
	if _season_btn != null:
		_season_btn.text = SEASONS[_season]


## The PREVIEW row — numbers from the CORE (LiveSim.preview_climate), never computed here (inv #2).
func _update_preview() -> void:
	if _live == null or _preview == null:
		return
	var p: Dictionary = _live.preview_climate(_lat.value, _lon.value, _temp.value, _season)
	_preview.text = "preview (core):   day length %.2f   ·   insolation %.2f   ·   temperature %.2f" % [
		float(p.get("day_length", 0.0)),
		float(p.get("insolation", 0.0)),
		float(p.get("temperature", 0.0)),
	]


func _on_random_toggled(on: bool) -> void:
	_seed_edit.editable = not on
	if on:
		_on_reroll()


func _on_reroll() -> void:
	_seed_edit.text = str(randi())


func _on_season_prev() -> void:
	_season = (_season + SEASONS.size() - 1) % SEASONS.size()
	_refresh_values()
	_update_preview()


func _on_season_next() -> void:
	_season = (_season + 1) % SEASONS.size()
	_refresh_values()
	_update_preview()


func _on_start() -> void:
	var seed_val := _seed
	if _seed_edit.text.is_valid_int():
		seed_val = int(_seed_edit.text)
	else:
		# Empty/garbage field: fall back to the last seed but write it back so the run uses exactly what the
		# field now shows (no silent "why didn't my seed take" surprise).
		_seed_edit.text = str(seed_val)
	# SP-2: assemble the roster [{stem,count}] from the rows, in row order (the load-bearing key). Drop count==0
	# rows, but never empty the roster (keep ≥1 so the run always spawns something).
	var roster: Array = []
	for e in _roster_rows:
		var idx: int = int((e["species"] as OptionButton).selected)
		var stem: String = ALL_SPECIES[clampi(idx, 0, ALL_SPECIES.size() - 1)][1]
		var count: int = int((e["count"] as SpinBox).value)
		if count > 0:
			roster.append({"stem": stem, "count": count})
	if roster.is_empty() and not _roster_rows.is_empty():
		# Every row was zeroed: keep the first row so the run is non-empty (per-species counts are authoritative,
		# but an all-zero roster would spawn nothing).
		var first: Dictionary = _roster_rows[0]
		var fidx: int = int((first["species"] as OptionButton).selected)
		roster.append({"stem": ALL_SPECIES[clampi(fidx, 0, ALL_SPECIES.size() - 1)][1], "count": ROSTER_DEFAULT_COUNT})
	# Back-compat: legacy/CLI single-species readers still see cfg.species == the first roster stem.
	var species_stem: String = String(roster[0]["stem"]) if not roster.is_empty() else "default"
	var containment: int = _containment_btn.selected if _containment_btn != null else 0
	start_run.emit(
		{
			"seed": seed_val,
			"lat": _lat.value,
			"lon": _lon.value,
			"temp": _temp.value,
			"season": _season,
			"entities": int(_entities.value),
			"mission": _mission_chk.button_pressed,
			"species": species_stem,
			"roster": roster,
			"containment": containment,
		}
	)
	queue_free()
