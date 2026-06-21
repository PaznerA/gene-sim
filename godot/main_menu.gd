extends CanvasLayer
## Pre-sim MAIN MENU overlay (ADR-012 E4): the player sets the world — seed (or random), GPS latitude/longitude,
## average temperature, season, population — before the run starts. Emits `start_run(cfg)` on Start; main.gd then
## reconfigures the LiveSim + reseeds in place (no relaunch).
##
## Renderer-only (inv #2): the PREVIEW row (day length / insolation / temperature) is computed by the CORE via
## LiveSim.preview_climate — this script never computes climate itself. Loaded by path (no class_name; ADR-010):
##   const MainMenu := preload("res://main_menu.gd")

signal start_run(cfg)  # { seed:int, lat:float, lon:float, temp:float, season:int, entities:int, mission:bool }

const SEASONS := ["Spring", "Summer", "Autumn", "Winter"]

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
	start_run.emit(
		{
			"seed": seed_val,
			"lat": _lat.value,
			"lon": _lon.value,
			"temp": _temp.value,
			"season": _season,
			"entities": int(_entities.value),
			"mission": _mission_chk.button_pressed,
		}
	)
	queue_free()
