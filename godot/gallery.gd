extends CanvasLayer
## STARTERS GALLERY — a RollerCoaster-Tycoon-style SCENARIO PICKER over the Stage-1 starter library
## (data/presets/starters/index.json + <slug>.json gen-1 docs + <slug>/ gen-N checkpoint session dirs).
##
## LEFT  : a scrollable ItemList of starters (name + caption + dynamics + a gen-1/checkpoint badge +
##         sustainability/predator flags), read from res://data/presets/starters/index.json.
## RIGHT : the selected starter's DESCRIPTION (roster · dynamics · env · recorded interventions) + an
##         ANIMATION PREVIEW area (the scenario-gif-preview <slug>.gif if present, else a static info card)
##         with a THICK timeline SLIDER under it. The slider is the EXISTING timeline.gd widget (reused, so
##         the recorded-edit markers + click-to-seek plumbing are identical to the live timeline); scrubbing
##         it moves a play-head across the RECORDED gen axis — scrubbable BACK through the journaled edits.
##
## Renderer-only (inv #2): this script only MOVES inert JSON bytes (FileAccess reads the res:// docs; the
## ndjson markers are a pure string projection of the journal, the SAME projection main.gd derives from the
## core's journal) and drives EXISTING #[func]s on Play — NO biology, NO new core action:
##   * gen-1 checkpoint badge → emit play_gen1(cfg); main.gd routes cfg through the proven menu Start path
##     (roster keys → set_roster, temp_q/season → set_environment, containment → set_containment, reset).
##   * gen-N checkpoint badge → emit play_checkpoint(slug, markers); main.gd calls the EXISTING load_session
##     #[func] on the staged <slug>/ session dir, then sets the recorded markers on the live timeline.
## Degrades gracefully: a missing/empty index → a clear "no starters" card + Back; a malformed doc is skipped
## with a push_warning; the Play button is has_method-guarded by main.gd (an older cdylib / file replay).
## Loaded by path (no class_name; ADR-010):  const Gallery := preload("res://gallery.gd")

const Timeline := preload("res://timeline.gd")

# gen-1 → reuse the menu Start cfg; gen-N → reuse load_session on the staged session dir + the parsed markers;
# Back → reopen the new-run menu. main.gd wires all three (it owns the LiveSim + the proven reset/load paths).
signal play_gen1(cfg)            # cfg dict identical to MainMenu.start_run (roster/env/containment/seed)
signal play_checkpoint(slug, markers)  # slug = <slug>/ session dir name; markers = the recorded-edit projection
signal back()                    # return to the new-run main menu (the gallery frees itself)

const STARTERS_DIR := "res://data/presets/starters"
const INDEX_PATH := STARTERS_DIR + "/index.json"
const SEASONS := ["Spring", "Summer", "Autumn", "Winter"]
const CONTAINMENT_LABELS := ["Sealed (OFF)", "Clean (ISO 7)", "Lab (ISO 8)", "Open"]
const PREVIEW_TICKS := 60  # how many gen-axis sample points the thick slider draws for a checkpoint preview
# The predator species key (its presence in a roster flags an org-eats-org food web in the list).
const PREDATOR_KEY := "bdellovibrio"

var _live: Object = null
var _entries: Array = []        # the index.json rows: [{slug,name,kind,caption,dynamics,source_hash}]
var _detail_cache: Dictionary = {}  # slug → the loaded detail dict (roster/env/markers), built lazily on select
var _selected: int = -1

var _list: ItemList = null
var _name_label: Label = null
var _desc: RichTextLabel = null
var _preview_box: VBoxContainer = null
var _preview_image: TextureRect = null
var _preview_note: Label = null
var _timeline: Control = null
var _scrub_label: Label = null
var _play_button: Button = null


## Called by main.gd before the overlay is added to the tree. `live` is the LiveSim (Play drives its EXISTING
## reset/load_session #[func]s); a null live (file replay / older cdylib) only disables the Play action.
func setup(live: Object) -> void:
	_live = live


func _ready() -> void:
	layer = 55  # above the HUD, in the same modal band as the main menu (50)
	_build()
	_load_index()
	_populate_list()
	if not _entries.is_empty():
		select_entry(0)


# ──────────────────────────── UI ──────────────────────────────────────────────────────────────────────────

func _build() -> void:
	var dim := ColorRect.new()
	dim.color = Color(0.02, 0.03, 0.03, 0.86)
	dim.set_anchors_preset(Control.PRESET_FULL_RECT)
	dim.mouse_filter = Control.MOUSE_FILTER_STOP  # block clicks reaching the (paused) sim behind the gallery
	add_child(dim)

	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	dim.add_child(center)

	var card := PanelContainer.new()
	var csb := StyleBoxFlat.new()
	csb.bg_color = Color(0.06, 0.10, 0.08, 0.99)
	csb.set_corner_radius_all(12)
	csb.set_content_margin_all(20)
	csb.border_width_left = 1
	csb.border_width_top = 1
	csb.border_width_right = 1
	csb.border_width_bottom = 1
	csb.border_color = Color(0.2, 0.5, 0.32, 0.7)
	card.add_theme_stylebox_override("panel", csb)
	center.add_child(card)

	var outer := VBoxContainer.new()
	outer.add_theme_constant_override("separation", 12)
	outer.custom_minimum_size = Vector2(940, 600)
	card.add_child(outer)

	var title := Label.new()
	title.text = "STARTERS  ·  SCENARIO LIBRARY"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color(0.7, 0.95, 0.75))
	outer.add_child(title)
	var sub := Label.new()
	sub.text = "Pick a saved starter, scrub its timeline, then Play. Gen-1 = a fresh run · Checkpoint = restored at gen N with its recorded edits."
	sub.add_theme_color_override("font_color", Color(0.6, 0.72, 0.62))
	outer.add_child(sub)

	# Two columns: LEFT scrollable list · RIGHT description + preview + thick scrub slider.
	var cols := HBoxContainer.new()
	cols.add_theme_constant_override("separation", 16)
	cols.size_flags_vertical = Control.SIZE_EXPAND_FILL
	outer.add_child(cols)

	# LEFT — the scenario list.
	var left := VBoxContainer.new()
	left.add_theme_constant_override("separation", 6)
	left.custom_minimum_size = Vector2(360, 0)
	cols.add_child(left)
	var list_label := Label.new()
	list_label.text = "Library"
	list_label.add_theme_font_size_override("font_size", 11)
	list_label.add_theme_color_override("font_color", Color(0.55, 0.68, 0.58))
	left.add_child(list_label)
	_list = ItemList.new()
	_list.custom_minimum_size = Vector2(360, 460)
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_list.auto_height = false
	_list.item_selected.connect(_on_item_selected)
	left.add_child(_list)
	var back := Button.new()
	back.text = "←  Back to New Run"
	back.pressed.connect(_on_back_pressed)
	left.add_child(back)

	# RIGHT — description + preview + thick scrub slider + Play.
	var right := VBoxContainer.new()
	right.add_theme_constant_override("separation", 10)
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	cols.add_child(right)

	_name_label = Label.new()
	_name_label.text = "—"
	_name_label.add_theme_font_size_override("font_size", 18)
	_name_label.add_theme_color_override("font_color", Color(0.85, 0.95, 0.85))
	right.add_child(_name_label)

	_desc = RichTextLabel.new()
	_desc.bbcode_enabled = true
	_desc.fit_content = false
	_desc.scroll_active = true
	_desc.custom_minimum_size = Vector2(520, 210)
	_desc.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right.add_child(_desc)

	# ANIMATION PREVIEW area: a <slug>.gif scenario preview if present, else a static info card.
	var preview_label := Label.new()
	preview_label.text = "Preview"
	preview_label.add_theme_font_size_override("font_size", 11)
	preview_label.add_theme_color_override("font_color", Color(0.55, 0.68, 0.58))
	right.add_child(preview_label)
	_preview_box = VBoxContainer.new()
	_preview_box.add_theme_constant_override("separation", 4)
	var pframe := PanelContainer.new()
	var psb := StyleBoxFlat.new()
	psb.bg_color = Color(0.03, 0.05, 0.05, 0.9)
	psb.set_corner_radius_all(8)
	psb.set_content_margin_all(8)
	pframe.add_theme_stylebox_override("panel", psb)
	pframe.add_child(_preview_box)
	right.add_child(pframe)
	_preview_image = TextureRect.new()
	_preview_image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_preview_image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	_preview_image.custom_minimum_size = Vector2(520, 120)
	_preview_image.visible = false
	_preview_box.add_child(_preview_image)
	_preview_note = Label.new()
	_preview_note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_preview_note.custom_minimum_size = Vector2(520, 96)
	_preview_note.add_theme_color_override("font_color", Color(0.72, 0.82, 0.74))
	_preview_box.add_child(_preview_note)

	# THICK timeline SLIDER (the EXISTING timeline.gd widget, reused): the recorded gen axis + edit markers,
	# click-to-seek scrub. Tall (thick). Its `seek` signal moves the play-head readout BACK through the markers.
	_timeline = Timeline.new()
	_timeline.custom_minimum_size = Vector2(520, 56)
	_timeline.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_timeline.seek.connect(_on_scrub)
	right.add_child(_timeline)
	_scrub_label = Label.new()
	_scrub_label.text = "scrub the timeline →"
	_scrub_label.add_theme_color_override("font_color", Color(0.6, 0.72, 0.62))
	right.add_child(_scrub_label)

	_play_button = Button.new()
	_play_button.text = "▶  PLAY / LOAD"
	_play_button.custom_minimum_size = Vector2(0, 40)
	_play_button.add_theme_font_size_override("font_size", 16)
	_play_button.pressed.connect(_on_play_pressed)
	right.add_child(_play_button)


# ──────────────────────────── index + list ────────────────────────────────────────────────────────────────

## Read res://data/presets/starters/index.json into `_entries` (the flat slug-sorted gallery rows). A missing /
## malformed index degrades to an empty list (the right panel shows a clear "no starters" card). inv #2: a pure
## read of inert JSON.
func _load_index() -> void:
	_entries = []
	var parsed: Variant = _read_json(INDEX_PATH)
	if typeof(parsed) != TYPE_ARRAY:
		push_warning("Starters: no index at %s (was data/presets staged into res://?)" % INDEX_PATH)
		return
	for row in parsed:
		if typeof(row) != TYPE_DICTIONARY:
			continue
		var d: Dictionary = row
		_entries.append({
			"slug": str(d.get("slug", "")),
			"name": str(d.get("name", d.get("slug", "?"))),
			"kind": str(d.get("kind", "gen1")),
			"caption": str(d.get("caption", "")),
			"dynamics": str(d.get("dynamics", "")),
			"source_hash": str(d.get("source_hash", "")),
		})


## Fill the ItemList with one row per starter: "Name — caption  [badge]  <sustain> <predator>". The badge marks
## gen-1 vs checkpoint; the sustainability glyph comes from the caption's trailing descriptor; the predator glyph
## lights when the roster carries Bdellovibrio (loaded lazily from the per-starter doc). inv #2: cosmetic labels
## off inert fields — no biology computed.
func _populate_list() -> void:
	if _list == null:
		return
	_list.clear()
	if _entries.is_empty():
		_name_label.text = "No starters found"
		_desc.text = "[color=#c98]The starter library is empty or unstaged.[/color]\n\nRun the promote tool to populate it:\n  cargo run -p harness -- --promote-default-set\n  cargo run -p harness -- --promote-gem <gem.json> --starter-name <slug> --checkpoint-gen <N>\n\nThen re-stage data/presets into the renderer (run.sh does this)."
		if _play_button != null:
			_play_button.disabled = true
		return
	for e in _entries:
		var detail: Dictionary = _detail_for(e)
		var badge := "[checkpoint @%d]" % int(detail.get("checkpoint_gen", 0)) if str(e["kind"]) == "checkpoint" else "[gen-1]"
		var sustain := _sustainability_glyph(str(e["caption"]))
		var predator := "  🦠" if bool(detail.get("predator", false)) else ""
		var line := "%s — %s   %s   %s%s" % [str(e["name"]), str(e["caption"]), badge, sustain, predator]
		_list.add_item(line)


# ──────────────────────────── selection → description + preview + scrub axis ───────────────────────────────

func _on_item_selected(i: int) -> void:
	select_entry(i)


## Select starter `i`: build the right-panel description, load/clear the gif preview, and set the thick timeline
## scrub axis (gen 0..checkpoint_gen + the recorded markers for a checkpoint; a flat axis for a pristine gen-1).
func select_entry(i: int) -> void:
	if i < 0 or i >= _entries.size():
		return
	_selected = i
	if _list != null and _list.item_count > i:
		_list.select(i)
	var e: Dictionary = _entries[i]
	var detail: Dictionary = _detail_for(e)
	_name_label.text = str(e["name"])
	_desc.text = _describe(e, detail)
	_setup_preview(e, detail)
	_setup_scrub(detail)
	if _play_button != null:
		_play_button.disabled = false
		_play_button.text = "▶  PLAY — fresh run" if str(e["kind"]) != "checkpoint" else "▶  LOAD — restore @gen %d" % int(detail.get("checkpoint_gen", 0))


## Compose the description (BBCode): roster · dynamics · env · the recorded interventions. Pure presentation of
## the inert starter fields (inv #2).
func _describe(e: Dictionary, detail: Dictionary) -> String:
	var lines: Array = []
	lines.append("[b]%s[/b]   ·   provenance %s" % [str(e["dynamics"]).to_upper(), str(e["source_hash"])])
	lines.append("")
	# Roster (key × starting count), count>0 only, in load-bearing order.
	lines.append("[color=#9cd1a8]Roster[/color]")
	var roster: Array = detail.get("roster", [])
	if roster.is_empty():
		lines.append("  (none)")
	for entry in roster:
		var key := str(entry[0])
		var count := int(entry[1])
		var tag := "  🦠 predator" if key == PREDATOR_KEY else ""
		lines.append("  • %s × %d%s" % [key, count, tag])
	lines.append("")
	# Environment.
	lines.append("[color=#9cd1a8]Environment[/color]")
	lines.append("  temperature %.2f   ·   season %s   ·   containment %s" % [
		float(detail.get("temp", 0.5)),
		SEASONS[clampi(int(detail.get("season", 0)), 0, SEASONS.size() - 1)],
		CONTAINMENT_LABELS[clampi(int(detail.get("containment", 0)), 0, CONTAINMENT_LABELS.size() - 1)],
	])
	lines.append("")
	# Recorded interventions (checkpoint only).
	lines.append("[color=#9cd1a8]Recorded interventions[/color]")
	var markers: Array = detail.get("markers", [])
	if str(e["kind"]) != "checkpoint":
		lines.append("  (none — gen-1 is a pristine pre-edit starting point)")
	elif markers.is_empty():
		lines.append("  (none recorded before the checkpoint generation)")
	else:
		for m in markers:
			lines.append("  • gen %d   ·   %s" % [int(m.get("generation", 0)), str(m.get("tool", "crispr")).to_upper()])
	return "\n".join(lines)


## Preview area: show res://data/presets/starters/<slug>.gif if present (best-effort static decode → a TextureRect),
## else a static info card. Godot does not animate GIFs natively, so this shows the scenario preview frame when the
## asset ships; absent one, the card summarises the dynamics + how to read the scrub slider below.
func _setup_preview(e: Dictionary, detail: Dictionary) -> void:
	var gif_path := "%s/%s.gif" % [STARTERS_DIR, str(e["slug"])]
	var tex: Texture2D = _try_load_gif(gif_path)
	if tex != null:
		_preview_image.texture = tex
		_preview_image.visible = true
		_preview_note.visible = false
		return
	_preview_image.visible = false
	_preview_note.visible = true
	if str(e["kind"]) == "checkpoint":
		_preview_note.text = "%s · %s\nRestored at generation %d with its recorded journal. Scrub the timeline below back through the recorded CRISPR edit markers, then press Load to continue the run forward from the checkpoint." % [
			str(e["name"]), str(e["caption"]), int(detail.get("checkpoint_gen", 0))]
	else:
		_preview_note.text = "%s · %s\nA pristine gen-1 starting point (roster + environment, no edits). Press Play to launch a fresh deterministic run from this configuration." % [
			str(e["name"]), str(e["caption"])]


## Set the thick scrub slider's gen axis + markers (the EXISTING timeline.gd widget). For a checkpoint the axis is
## gen 0..checkpoint_gen sampled into PREVIEW_TICKS ticks, with the recorded edit markers placed by generation —
## so the slider scrubs BACK through them. For a gen-1 the axis is flat (no recorded run).
func _setup_scrub(detail: Dictionary) -> void:
	if _timeline == null:
		return
	var max_gen := int(detail.get("max_gen", 0))
	var markers: Array = detail.get("markers", [])
	var gens: Array = []
	if max_gen <= 0:
		gens = [0]
	else:
		var ticks: int = mini(PREVIEW_TICKS, max_gen + 1)
		for t in ticks:
			gens.append(int(round(float(t) * float(max_gen) / float(maxi(1, ticks - 1)))))
	_timeline.setup(gens)
	_timeline.set_markers(markers)
	_timeline.set_index(gens.size() - 1)  # play-head at the checkpoint (scrub LEFT to go back through edits)
	_update_scrub_label(gens.size() - 1, gens)


func _on_scrub(index: int) -> void:
	if _selected < 0:
		return
	var detail: Dictionary = _detail_for(_entries[_selected])
	var gens: Array = _scrub_axis(detail)
	if _timeline != null:
		_timeline.set_index(clampi(index, 0, gens.size() - 1))
	_update_scrub_label(index, gens)


func _scrub_axis(detail: Dictionary) -> Array:
	var max_gen := int(detail.get("max_gen", 0))
	if max_gen <= 0:
		return [0]
	var ticks: int = mini(PREVIEW_TICKS, max_gen + 1)
	var gens: Array = []
	for t in ticks:
		gens.append(int(round(float(t) * float(max_gen) / float(maxi(1, ticks - 1)))))
	return gens


func _update_scrub_label(index: int, gens: Array) -> void:
	if _scrub_label == null:
		return
	if gens.size() <= 1:
		_scrub_label.text = "no recorded timeline (gen-1)"
		return
	var gen: int = int(gens[clampi(index, 0, gens.size() - 1)])
	_scrub_label.text = "timeline @ gen %d  (scrub back through the recorded edits)" % gen


# ──────────────────────────── Play / Back ─────────────────────────────────────────────────────────────────

## Play/Load the selected starter. gen-1 → emit play_gen1(cfg) (main.gd routes it through the menu Start path);
## checkpoint → emit play_checkpoint(slug, markers) (main.gd calls the EXISTING load_session #[func]). The gallery
## frees itself after emitting (mirrors the menu). No-op with nothing selected.
func _on_play_pressed() -> void:
	play_selected()


func play_selected() -> void:
	if _selected < 0 or _selected >= _entries.size():
		return
	var e: Dictionary = _entries[_selected]
	var detail: Dictionary = _detail_for(e)
	if str(e["kind"]) == "checkpoint":
		play_checkpoint.emit(str(e["slug"]), detail.get("markers", []))
	else:
		play_gen1.emit(_gen1_cfg(detail))
	queue_free()


func _on_back_pressed() -> void:
	back.emit()
	queue_free()


## Build the cfg dict _on_menu_start consumes from a gen-1 starter's loaded detail: roster (count>0, in order),
## env (temp/season), containment, source_seed. Mirrors MainMenu.start_run's payload (the renderer only moves
## inert strings + ints; the core builds every species — inv #2).
func _gen1_cfg(detail: Dictionary) -> Dictionary:
	var roster: Array = []
	var total := 0
	for entry in detail.get("roster", []):
		var count := int(entry[1])
		if count > 0:
			roster.append({"stem": str(entry[0]), "count": count})
			total += count
	var active := str((roster[0] as Dictionary).get("stem", "default")) if not roster.is_empty() else "default"
	return {
		"seed": int(detail.get("source_seed", 0)),
		"lat": 0.0,
		"lon": 0.0,
		"temp": float(detail.get("temp", 0.5)),
		"season": int(detail.get("season", 0)),
		"entities": maxi(total, 50),
		"mission": false,
		"species": active,
		"roster": roster,
		"containment": int(detail.get("containment", 0)),
	}


# ──────────────────────────── per-starter detail loading (inert JSON → normalized dict) ────────────────────

## Lazily load + cache a starter's full detail (roster/env/markers/predator). gen-1 reads <slug>.json; a checkpoint
## reads <slug>/starter.json (metadata) + <slug>/seed.json (roster + env) + parses <slug>/actions.ndjson (the
## recorded markers). Pure inert reads (inv #2). A malformed doc returns an empty-but-safe detail.
func _detail_for(e: Dictionary) -> Dictionary:
	var slug := str(e["slug"])
	if _detail_cache.has(slug):
		return _detail_cache[slug]
	var detail: Dictionary
	if str(e["kind"]) == "checkpoint":
		detail = _load_checkpoint_detail(slug)
	else:
		detail = _load_gen1_detail(slug)
	_detail_cache[slug] = detail
	return detail


func _load_gen1_detail(slug: String) -> Dictionary:
	var parsed: Variant = _read_json("%s/%s.json" % [STARTERS_DIR, slug])
	var roster: Array = []
	var temp := 0.5
	var season := 0
	var containment := 0
	var source_seed := 0
	if typeof(parsed) == TYPE_DICTIONARY:
		var doc: Dictionary = parsed
		source_seed = int(doc.get("source_seed", 0))
		var config_v: Variant = doc.get("config", {})
		if typeof(config_v) == TYPE_DICTIONARY:
			var config: Dictionary = config_v
			roster = _roster_from_pairs(config.get("roster", []))
			temp = clampf(float(int(config.get("temp_q", 500))) / 1000.0, 0.0, 1.0)
			season = int(config.get("season", 0))
			containment = int(config.get("containment_level", 0))
	else:
		push_warning("Starters: malformed gen-1 doc for %s" % slug)
	return {
		"roster": roster, "temp": temp, "season": season, "containment": containment,
		"source_seed": source_seed, "checkpoint_gen": 0, "markers": [], "max_gen": 0,
		"predator": _roster_has_predator(roster),
	}


func _load_checkpoint_detail(slug: String) -> Dictionary:
	var checkpoint_gen := 0
	var source_seed := 0
	var meta: Variant = _read_json("%s/%s/starter.json" % [STARTERS_DIR, slug])
	if typeof(meta) == TYPE_DICTIONARY:
		checkpoint_gen = int((meta as Dictionary).get("checkpoint_gen", 0))
		source_seed = int((meta as Dictionary).get("source_seed", 0))
	# Roster + env from the session's seed.json (the EnvConfig the core replays under).
	var roster: Array = []
	var temp := 0.5
	var season := 0
	var seed_v: Variant = _read_json("%s/%s/seed.json" % [STARTERS_DIR, slug])
	if typeof(seed_v) == TYPE_DICTIONARY:
		var sj: Dictionary = seed_v
		temp = clampf(float(sj.get("avg_temp", 0.5)), 0.0, 1.0)
		season = int(sj.get("season", 0))
		for r in sj.get("roster", []):
			if typeof(r) != TYPE_DICTIONARY:
				continue
			var spec_v: Variant = (r as Dictionary).get("species", {})
			var key := ""
			var count := 0
			if typeof(spec_v) == TYPE_DICTIONARY:
				var spec: Dictionary = spec_v
				key = str(spec.get("key", ""))
				var niche_v: Variant = spec.get("niche", {})
				if typeof(niche_v) == TYPE_DICTIONARY:
					count = int((niche_v as Dictionary).get("entity_count", 0))
			if key != "":
				roster.append([key, count])
	# Recorded markers — the journal projection (parse actions.ndjson).
	var proj: Dictionary = _parse_session_markers(slug)
	return {
		"roster": roster, "temp": temp, "season": season, "containment": 0,
		"source_seed": source_seed, "checkpoint_gen": checkpoint_gen,
		"markers": proj.get("markers", []), "max_gen": maxi(checkpoint_gen, int(proj.get("max_gen", 0))),
		"predator": _roster_has_predator(roster),
	}


## Parse a checkpoint session's actions.ndjson into timeline markers — the SAME journal→marker projection main.gd
## derives from the core (each Advance accumulates the gen cursor; each region/edit action drops a per-tool marker
## at the running gen). The Action serde is externally tagged ({"Advance":N} / {"ApplyEdit":{…}} / {"RegionCull":
## {…}} …), so this is pure string/JSON parsing — NO biology (inv #2). Returns {markers, max_gen}.
func _parse_session_markers(slug: String) -> Dictionary:
	var path := "%s/%s/actions.ndjson" % [STARTERS_DIR, slug]
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return {"markers": [], "max_gen": 0}
	var gen := 0
	var markers: Array = []
	while not f.eof_reached():
		var line := f.get_line().strip_edges()
		if line == "":
			continue
		var v: Variant = JSON.parse_string(line)
		if typeof(v) != TYPE_DICTIONARY:
			continue
		var d: Dictionary = v
		if d.has("Advance"):
			gen += int(d["Advance"])
			continue
		for key in d.keys():
			var tool := _action_key_to_tool(str(key))
			if tool != "":
				markers.append({"generation": gen, "tool": tool, "applied": true, "label": str(key)})
			break  # one tag per Action line
	f.close()
	return {"markers": markers, "max_gen": gen}


## Map an externally-tagged Action key → a timeline.gd tool string (the SAME mapping main.gd::_journal_kind_to_tool
## uses, keyed on the serde variant names). "" = a non-marker action (a bare Advance is handled before this).
func _action_key_to_tool(key: String) -> String:
	match key:
		"ApplyEdit", "ApplyEditRegion": return "crispr"
		"RegionPcrAmplify": return "pcr"
		"RegionCull": return "cull"
		"RegionNutrient": return "nutrient"
		"RegionToxin": return "toxin"
		"RegionInoculate": return "inoculate"
		_: return ""


## Convert a serde-tuple roster ([[key,count], …]) into [[key,count], …] keeping order (gen-1 docs store zero-count
## absent species; the description/Play filter those). Defensive against malformed rows.
func _roster_from_pairs(pairs: Variant) -> Array:
	var out: Array = []
	if typeof(pairs) != TYPE_ARRAY:
		return out
	for entry in pairs:
		if typeof(entry) == TYPE_ARRAY and (entry as Array).size() >= 2:
			out.append([str(entry[0]), int(entry[1])])
	return out


func _roster_has_predator(roster: Array) -> bool:
	for entry in roster:
		if typeof(entry) == TYPE_ARRAY and (entry as Array).size() >= 2 \
				and str(entry[0]) == PREDATOR_KEY and int(entry[1]) > 0:
			return true
	return false


## Sustainability flag glyph from a caption's trailing descriptor (steady/takeovers/crashes/flat). Cosmetic facet
## off the inert caption string (inv #2) — not a computed phenotype.
func _sustainability_glyph(caption: String) -> String:
	var segs := caption.split("·")
	var descriptor := str(segs[segs.size() - 1]).strip_edges() if segs.size() > 0 else ""
	match descriptor:
		"steady": return "🌱 steady"
		"takeovers": return "🔄 turnover"
		"crashes": return "⚠ crash-prone"
		_: return descriptor


# ──────────────────────────── helpers ─────────────────────────────────────────────────────────────────────

## Read a res:// JSON file into a Variant (Dictionary/Array) or null. inv #2: inert bytes only.
func _read_json(path: String) -> Variant:
	if not FileAccess.file_exists(path):
		return null
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return null
	var text := f.get_as_text()
	f.close()
	return JSON.parse_string(text)


## Best-effort static GIF decode → a Texture2D (Godot does not animate GIFs; we show the first/only frame as a
## scenario preview when the asset ships). Returns null if absent or undecodable (→ the static info card).
func _try_load_gif(path: String) -> Texture2D:
	if not FileAccess.file_exists(path):
		return null
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return null
	var bytes := f.get_buffer(f.get_length())
	f.close()
	var img := Image.new()
	if img.load_png_from_buffer(bytes) != OK and img.load_jpg_from_buffer(bytes) != OK:
		# Godot 4 has no GIF-from-buffer loader; a .gif decode is best-effort and usually falls through here.
		return null
	return ImageTexture.create_from_image(img)


# ──────────────────────────── headless verify surface (inv #4) ─────────────────────────────────────────────

## Drive every gallery row headlessly (the --gallery-check gate): select each starter (builds the description +
## preview + scrub axis, so a GDScript error in any branch goes RED) and tally the kinds + recorded markers.
## Returns {count, gen1, checkpoint, markers_total, first_gen1, first_checkpoint, checkpoint_markers}.
func check_headless() -> Dictionary:
	var gen1 := 0
	var checkpoint := 0
	var markers_total := 0
	var first_gen1 := -1
	var first_checkpoint := -1
	var checkpoint_markers := 0
	for i in _entries.size():
		select_entry(i)
		var e: Dictionary = _entries[i]
		var detail: Dictionary = _detail_for(e)
		var mk: int = (detail.get("markers", []) as Array).size()
		markers_total += mk
		if str(e["kind"]) == "checkpoint":
			checkpoint += 1
			if first_checkpoint < 0:
				first_checkpoint = i
				checkpoint_markers = mk
		else:
			gen1 += 1
			if first_gen1 < 0:
				first_gen1 = i
	return {
		"count": _entries.size(), "gen1": gen1, "checkpoint": checkpoint,
		"markers_total": markers_total, "first_gen1": first_gen1,
		"first_checkpoint": first_checkpoint, "checkpoint_markers": checkpoint_markers,
	}
