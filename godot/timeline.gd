extends Control
## Full-width bottom timeline: a generation axis with per-snapshot ticks, a play-head at the current frame,
## and click-to-seek. Read-only presentation (invariant #2) — it only visualises which snapshot is shown and
## emits a seek request. Injection markers (from actions.ndjson) will hang off this axis in a later slice.

signal seek(index: int)

const PAD := 12.0

var _count: int = 0
var _idx: int = 0
var _gens: Array = []  # generation number per snapshot index, for labels


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
