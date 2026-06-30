extends Control
## Full-width bottom timeline: a generation axis with per-snapshot ticks, a play-head at the current frame,
## and click-to-seek. Read-only presentation (invariant #2) — it only visualises which snapshot is shown and
## emits a seek request. INTERVENTION markers (SP-3.7) hang off this axis: each journaled Action (CRISPR / PCR /
## Antibiotic / Nutrient / Toxin) shows as a per-tool coloured + glyphed tab at its generation. The markers are a
## deterministic PROJECTION of the journal (the journal is the source of truth) — a replayed/scrubbed session
## shows every intervention exactly where it fired (see main.gd::_rebuild_markers_from_journal).

signal seek(index: int)

const PAD := 12.0
# Selected-marker EFFECT sparkline (live-session-sparkline): the small mini-chart shown for the HOVERED marker
# ONLY (so the axis is never re-cluttered). Bounded card dimensions in px.
const SPARK_W := 60.0
const SPARK_H := 24.0

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
	"oversight": {"color": Color(0.95, 0.85, 0.42), "glyph": "⚖"}, # gold (ADR-017 OVERSIGHT earned-credit deep edit)
}

var _count: int = 0
var _idx: int = 0
var _gens: Array = []  # generation number per snapshot index, for labels
var _markers: Array = []  # [{generation:int, tool:String, applied:bool, label:String, effect:PackedFloat32Array}] interventions
var _hover: int = -1  # index into _markers of the hovered marker (selected-marker effect sparkline); -1 = none


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
	# Markers only ever APPEND in _injections, so an existing hover index stays valid; on a rebuild/shrink (e.g. a
	# session reload) drop a now-stale selection. The effect series re-attached here just GROWS live for the held
	# hover — a marker/snapshot STATE change (inv #3), one redraw, not per-frame.
	if _hover >= _markers.size():
		_hover = -1
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

	# SELECTED/hovered-marker EFFECT sparkline (live-session-sparkline): a bounded mini-chart of the run metric
	# (normalized mean fitness) over the window AFTER the marker fired — drawn for the HOVERED marker ONLY, so the
	# timeline is NOT re-cluttered (no dense per-marker sparklines). Pure presentation (inv #2): it renders the
	# marker's pre-computed `effect` series (built in main.gd from the ordered render history); it reads no genome.
	# inv #3: this extra draw fires only on a selection / marker / snapshot STATE change (the queue_redraw sites),
	# never per-frame. No selection → nothing extra (the timeline looks exactly as it does today).
	if _hover >= 0 and _hover < _markers.size() and g1 > g0:
		var hm = _markers[_hover]
		var hg: int = int(hm.get("generation", 0))
		if hg >= g0 and hg <= g1:
			var eff: PackedFloat32Array = hm.get("effect", PackedFloat32Array())
			if eff.size() >= 2:
				var hmx := PAD + tw * float(hg - g0) / float(g1 - g0)
				_draw_effect_spark(hmx, w, h, eff, _tool_color(str(hm.get("tool", "crispr"))))


## Draw the bounded effect sparkline card near a marker's tab at local x `mx`. `eff` is the marker's normalized
## [0,1] series (built in main.gd from the ORDERED render history); `col` is the per-tool colour. A PURE function of
## its arguments — no biology, no RNG/Time/OS (inv #2/#3) — so the extra draw stays presentation-only.
func _draw_effect_spark(mx: float, w: float, h: float, eff: PackedFloat32Array, col: Color) -> void:
	var bw := SPARK_W
	var bh := minf(SPARK_H, h - 8.0)
	var bx := clampf(mx - bw * 0.5, PAD, maxf(PAD, w - PAD - bw))
	var by := 3.0
	# Backing card + a faint per-tool frame so the trace reads against the ticks/markers behind it.
	draw_rect(Rect2(bx, by, bw, bh), Color(0.0, 0.0, 0.0, 0.66))
	draw_rect(Rect2(bx, by, bw, bh), Color(col.r, col.g, col.b, 0.5), false, 1.0)
	# A short connector from the card toward the marker tab (which marker this chart belongs to).
	draw_line(Vector2(mx, by + bh), Vector2(mx, by + bh + 4.0), Color(col.r, col.g, col.b, 0.5), 1.0)
	var inset := 3.0
	var n := eff.size()
	var pts := PackedVector2Array()
	pts.resize(n)
	for k in n:
		var fx := bx + inset + (bw - 2.0 * inset) * float(k) / float(n - 1)
		var fy := by + bh - inset - (bh - 2.0 * inset) * clampf(eff[k], 0.0, 1.0)
		pts[k] = Vector2(fx, fy)
	draw_polyline(pts, col, 1.5, true)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		# Selected-marker sparkline (live-session-sparkline): track the nearest marker under the cursor (the SAME
		# nearest-x logic the tooltip uses) and redraw ONLY when that selection index actually CHANGES — a selection
		# STATE change, never a per-frame/per-pixel redraw (inv #3).
		var hover := _nearest_marker(event.position.x)
		if hover != _hover:
			_hover = hover
			queue_redraw()
		return
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		if _count <= 1:
			return
		var tw := size.x - 2.0 * PAD
		var frac := clampf((event.position.x - PAD) / maxf(1.0, tw), 0.0, 1.0)
		seek.emit(int(round(frac * float(_count - 1))))


## Clear the hovered-marker selection when the cursor leaves the timeline so the effect sparkline doesn't linger.
## A selection STATE change → one redraw (inv #3), never per-frame.
func _notification(what: int) -> void:
	if what == NOTIFICATION_MOUSE_EXIT and _hover != -1:
		_hover = -1
		queue_redraw()


## The index into `_markers` of the marker whose tab is nearest local x `at_x` AND within the hover threshold, or
## -1 for none. The shared selection-by-nearest-x logic used by BOTH the hover sparkline (_gui_input) and the
## native tooltip (_get_tooltip). Pure read of the ORDERED `_markers` (inv #2/#3): a forward scan keyed by each
## marker's stored generation — no Dictionary/hash-order iteration, no RNG/Time/OS.
func _nearest_marker(at_x: float) -> int:
	if _count <= 1 or _markers.is_empty():
		return -1
	var g0: int = _gens[0]
	var g1: int = _gens[_count - 1]
	if g1 <= g0:
		return -1
	var tw := size.x - 2.0 * PAD
	var best := -1
	var best_dx := 7.0  # pick the nearest marker within 7px of the cursor x
	for mi in _markers.size():
		var g: int = int(_markers[mi].get("generation", 0))
		if g < g0 or g > g1:
			continue
		var mx := PAD + tw * float(g - g0) / float(g1 - g0)
		var dx: float = absf(at_x - mx)
		if dx <= best_dx:
			best_dx = dx
			best = mi
	return best


## Native hover tooltip naming the intervention under the cursor (SP-3.7): "🧫 PCR · gen 42 · +2.4M J". Read-only
## (inv #2) — it just reflects the marker's stored {tool, generation, label}. Empty string → no tooltip (the
## default), so hovering the bare axis shows nothing extra.
func _get_tooltip(at_position: Vector2) -> String:
	var mi := _nearest_marker(at_position.x)  # shared nearest-x selection (same as the hover sparkline)
	if mi < 0:
		return ""
	var m = _markers[mi]
	var g: int = int(m.get("generation", 0))
	var tool: String = str(m.get("tool", "crispr"))
	var style: Dictionary = TOOL_STYLE.get(tool, TOOL_STYLE["crispr"])
	var label: String = str(m.get("label", ""))
	var best := "%s %s · gen %d" % [style["glyph"], tool.to_upper(), g]
	if label != "":
		best += " · " + label
	return best
