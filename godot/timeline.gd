extends Control
## Full-width bottom timeline: a generation axis with per-snapshot ticks, a play-head at the current frame,
## and click-to-seek. Read-only presentation (invariant #2) — it only visualises which snapshot is shown and
## emits a seek request. INTERVENTION markers (SP-3.7) hang off this axis: each journaled Action (CRISPR / PCR /
## Antibiotic / Nutrient / Toxin) shows as a per-tool coloured + glyphed tab at its generation. The markers are a
## deterministic PROJECTION of the journal (the journal is the source of truth) — a replayed/scrubbed session
## shows every intervention exactly where it fired (see main.gd::_rebuild_markers_from_journal).

signal seek(index: int)

const PAD := 12.0

# Per-tool colour + glyph for the intervention markers (SP-3.7). Keyed by the `tool` string the
# dispatcher stamps (_active_tool / _record_tool_outcome). An unknown tool falls back to the CRISPR look,
# so an older marker dict that carries only {generation, applied} still draws (back-compat).
const TOOL_STYLE := {
	"crispr": {"color": Color(0.42, 0.9, 0.46), "glyph": "🧬"},   # green
	"pcr": {"color": Color(0.36, 0.82, 0.92), "glyph": "🧫"},      # cyan
	"cull": {"color": Color(0.95, 0.42, 0.42), "glyph": "💊"},     # red (antibiotic)
	"nutrient": {"color": Color(0.95, 0.78, 0.32), "glyph": "🌱"}, # amber
	"toxin": {"color": Color(0.74, 0.46, 0.95), "glyph": "☣"},    # violet
	"inoculate": {"color": Color(0.62, 0.85, 0.38), "glyph": "🦠"}, # biohazard-lime (ADR-019 immigration / contamination)
}

var _count: int = 0
var _idx: int = 0
var _gens: Array = []  # generation number per snapshot index, for labels
var _markers: Array = []  # [{generation:int, tool:String, applied:bool, label:String}] interventions on the gen axis


## Per-tool colour for a marker; falls back to the CRISPR green for an unknown/missing tool (back-compat with the
## pre-SP-3.7 {generation, applied} marker shape). A failed/inert marker is drawn dimmed by the caller.
func _tool_color(tool: String) -> Color:
	var style: Dictionary = TOOL_STYLE.get(tool, TOOL_STYLE["crispr"])
	return style["color"]


func _tool_glyph(tool: String) -> String:
	var style: Dictionary = TOOL_STYLE.get(tool, TOOL_STYLE["crispr"])
	return style["glyph"]


func set_markers(markers: Array) -> void:
	_markers = markers
	queue_redraw()


func setup(gens: Array) -> void:
	_gens = gens
	_count = gens.size()
	queue_redraw()


func set_index(i: int) -> void:
	_idx = i
	queue_redraw()


func _draw() -> void:
	var w := size.x
	var h := size.y
	var mid := h * 0.5
	# Panel background + track.
	draw_rect(Rect2(0.0, 0.0, w, h), Color(0.0, 0.0, 0.0, 0.5))
	draw_rect(Rect2(PAD, mid - 3.0, w - 2.0 * PAD, 6.0), Color(1, 1, 1, 0.12))
	if _count <= 1:
		return
	var tw := w - 2.0 * PAD
	var font := ThemeDB.fallback_font

	# Per-snapshot ticks; label every few.
	var label_every := int(ceil(float(_count) / 8.0))
	for i in _count:
		var x := PAD + tw * float(i) / float(_count - 1)
		draw_line(Vector2(x, mid - 6.0), Vector2(x, mid + 6.0), Color(1, 1, 1, 0.22), 1.0)
		if i % label_every == 0:
			var g: int = _gens[i] if i < _gens.size() else i
			draw_string(font, Vector2(x - 8.0, mid + 20.0), str(g), HORIZONTAL_ALIGNMENT_LEFT, -1, 11,
				Color(0.7, 0.76, 0.7))

	# Intervention markers (SP-3.7), placed on the generation axis with a PER-TOOL colour + glyph. A failed/inert
	# intervention is dimmed (lower alpha). The marker shape is the EXISTING `mx` placement + downward-tab polygon.
	var g0: int = _gens[0]
	var g1: int = _gens[_count - 1]
	if g1 > g0:
		for m in _markers:
			var g: int = int(m.get("generation", 0))
			if g < g0 or g > g1:
				continue  # outside the visible (rolling) window
			var mx := PAD + tw * float(g - g0) / float(g1 - g0)
			var tool: String = str(m.get("tool", "crispr"))
			var applied := bool(m.get("applied", false))
			var mc: Color = _tool_color(tool)
			if not applied:
				mc.a = 0.4  # dim the failed/inert variant (it still shows where it was attempted)
			draw_line(Vector2(mx, 6.0), Vector2(mx, h - 16.0), mc, 2.0)
			draw_polygon(
				PackedVector2Array([Vector2(mx - 4.0, 4.0), Vector2(mx + 4.0, 4.0), Vector2(mx, 11.0)]),
				PackedColorArray([mc, mc, mc]))  # downward marker tab
			# Per-tool glyph above the tab (a compact legend of which tool fired where).
			draw_string(font, Vector2(mx - 6.0, mid - 12.0), _tool_glyph(tool), HORIZONTAL_ALIGNMENT_LEFT,
				-1, 11, mc)

	# Play-head.
	var px := PAD + tw * float(_idx) / float(_count - 1)
	draw_line(Vector2(px, 4.0), Vector2(px, h - 4.0), Color(0.96, 0.86, 0.3, 0.9), 2.0)
	draw_circle(Vector2(px, mid), 5.0, Color(0.96, 0.86, 0.3))
	var gen: int = _gens[_idx] if _idx < _gens.size() else _idx
	draw_string(font, Vector2(px + 9.0, mid - 7.0), "gen %d" % gen, HORIZONTAL_ALIGNMENT_LEFT, -1, 13,
		Color(0.95, 0.98, 0.95))


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		if _count <= 1:
			return
		var tw := size.x - 2.0 * PAD
		var frac := clampf((event.position.x - PAD) / maxf(1.0, tw), 0.0, 1.0)
		seek.emit(int(round(frac * float(_count - 1))))


## Native hover tooltip naming the intervention under the cursor (SP-3.7): "🧫 PCR · gen 42 · +2.4M J". Read-only
## (inv #2) — it just reflects the marker's stored {tool, generation, label}. Empty string → no tooltip (the
## default), so hovering the bare axis shows nothing extra.
func _get_tooltip(at_position: Vector2) -> String:
	if _count <= 1 or _markers.is_empty():
		return ""
	var g0: int = _gens[0]
	var g1: int = _gens[_count - 1]
	if g1 <= g0:
		return ""
	var tw := size.x - 2.0 * PAD
	var best := ""
	var best_dx := 7.0  # pick the nearest marker within 7px of the cursor x
	for m in _markers:
		var g: int = int(m.get("generation", 0))
		if g < g0 or g > g1:
			continue
		var mx := PAD + tw * float(g - g0) / float(g1 - g0)
		var dx: float = absf(at_position.x - mx)
		if dx <= best_dx:
			best_dx = dx
			var tool: String = str(m.get("tool", "crispr"))
			var style: Dictionary = TOOL_STYLE.get(tool, TOOL_STYLE["crispr"])
			var label: String = str(m.get("label", ""))
			best = "%s %s · gen %d" % [style["glyph"], tool.to_upper(), g]
			if label != "":
				best += " · " + label
	return best
