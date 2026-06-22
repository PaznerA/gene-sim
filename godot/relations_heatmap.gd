extends Control
## Relations view (Rel-UI.0): the emergent S×S inter-species FlowMatrix drawn as a diverging heatmap.
##
## INVARIANT #2 (STOP THE LINE if violated): read-only presentation. Every joule of cross-species flow is
## MEASURED in the Rust core (the F4 FlowMatrix — recorded at trophic_transfer/mineralize in canonical order,
## integer-add only, never a HashMap; the row-sum==0 diagonal-pairing is applied in-core). This node receives a
## finished flat `Vec<i64>` (row-major, `flat[i*s+j]` = NET joules that flowed FROM species j INTO species i)
## and maps each integer to a colored cell + a printed number. It computes NO biology: the only arithmetic is
## the per-call max-abs over OFF-diagonal entries, which is pure DISPLAY scaling — identical in kind to main.gd's
## `_mean_pop` / the `_inferno` legend ramp. It NEVER sums/signs/normalizes-as-biology or derives flow.
##
## INVARIANT #3: a deterministic draw of recorded integers — no RNG. NO class_name (preload convention, resolves
## under a fresh `--headless` run); modeled on sparkline.gd's read-only `_draw()`.
##
## Layout: CELL (row=i, col=j) reads flat[i*s + j] DIRECTLY (no transpose). Row i = SINK (energy GAINED), col j =
## SOURCE (energy GIVEN). Diagonal (the self-reference sink that makes each row sum to zero) is drawn muted/hatched
## so the eye reads the OFF-diagonals — the real inter-species edges. Axis labels = species names (down the left =
## sink, across the top = source); they line up with the matrix indices BY CONSTRUCTION (SpeciesId ordinals).

const LABEL_W := 96.0  # left gutter for the per-row (sink) species name
const HEADER_H := 56.0  # top band for the per-column (source) species names (rotated text is awkward; stacked)
const CELL_MIN := 28.0  # minimum cell edge so a small roster still reads
const PAD := 8.0

var _names: PackedStringArray = PackedStringArray()
var _flat: PackedInt64Array = PackedInt64Array()
var _s: int = 0
var _max_abs: int = 0  # per-set max-abs over OFF-diagonal entries (display scaling only; 0 ⇒ uniform neutral)


## Axis labels, in SpeciesId order (the same order observe_species()/the FlowMatrix use → row/col i is the i-th
## species by construction, no index remap). Sizes the grid; does NOT itself trigger a redraw of the matrix.
func setup(names: PackedStringArray) -> void:
	_names = names
	queue_redraw()


## The flat S*S row-major i64 matrix (the F4 contract). Recomputes the off-diagonal max-abs for display scaling
## (the renderer's ONLY arithmetic on the data — not biology) and redraws. A short/empty/over-long array or s<=0
## is treated as a valid degenerate input (State 1/2): the grid still renders, all-neutral.
func set_matrix(flat: PackedInt64Array, s: int) -> void:
	_flat = flat
	_s = maxi(0, s)
	_max_abs = 0
	if _s > 0 and _flat.size() == _s * _s:
		for i in _s:
			for j in _s:
				if i == j:
					continue  # the diagonal self-sink is not an inter-species edge — exclude from scaling
				var a: int = absi(int(_flat[i * _s + j]))
				if a > _max_abs:
					_max_abs = a
	queue_redraw()


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.0, 0.0, 0.0, 0.32))
	# Size the grid from the species count (labels), so the grid + real names render even when the matrix is
	# missing/zero (State 1/2). Fall back to the matrix dimension if names are absent.
	var n := _names.size()
	if n == 0:
		n = _s
	if n <= 0:
		return
	var grid_origin := Vector2(LABEL_W + PAD, HEADER_H + PAD)
	var avail := size - grid_origin - Vector2(PAD, PAD)
	var cell := maxf(CELL_MIN, minf(avail.x / float(n), avail.y / float(n)))
	var print_values := (n <= 6)  # when S<=6 the integer J value fits legibly inside the cell

	# Column (source) headers across the top.
	for j in n:
		var cx := grid_origin.x + float(j) * cell
		var nm := _names[j] if j < _names.size() else "sp%d" % j
		_draw_small(nm, Vector2(cx + 2.0, HEADER_H - 14.0), Color(0.74, 0.82, 0.74), cell - 4.0)

	for i in n:
		var cy := grid_origin.y + float(i) * cell
		# Row (sink) label down the left gutter.
		var rnm := _names[i] if i < _names.size() else "sp%d" % i
		_draw_small(rnm, Vector2(PAD, cy + cell * 0.5 - 4.0), Color(0.74, 0.82, 0.74), LABEL_W - PAD)
		for j in n:
			var cx := grid_origin.x + float(j) * cell
			var rect := Rect2(cx, cy, cell - 2.0, cell - 2.0)
			var have := (_s == n and _flat.size() == _s * _s)
			var v: int = int(_flat[i * _s + j]) if have else 0
			if i == j:
				# Diagonal: the self-reference sink (negation of the row's off-diagonal net). Muted + hatched so
				# the eye skips it and reads the real inter-species edges off the diagonal.
				draw_rect(rect, Color(0.10, 0.11, 0.12, 0.85))
				_hatch(rect)
			else:
				draw_rect(rect, _cell_color(v))
			# Subtle cell border.
			draw_rect(rect, Color(0.0, 0.0, 0.0, 0.35), false, 1.0)
			if print_values and (have or i != j):
				var txt := str(v)
				_draw_small(txt, Vector2(cx + 3.0, cy + cell * 0.5 - 7.0),
					Color(0.95, 0.97, 0.95) if i != j else Color(0.5, 0.52, 0.54), cell - 6.0)


## Diverging ramp on the signed integer, normalized by the per-call off-diagonal max-abs. Positive (i GAINS from j
## ⇒ j is a SOURCE / mutualist / prey for i) = warm green; negative (j drains i) = cool red; zero = neutral dark.
## Magnitude → saturation/value. GUARD: max-abs==0 ⇒ skip normalization (no div-by-zero), paint neutral.
func _cell_color(v: int) -> Color:
	var neutral := Color(0.13, 0.14, 0.15, 0.92)
	if _max_abs <= 0 or v == 0:
		return neutral
	var t := clampf(float(absi(v)) / float(_max_abs), 0.0, 1.0)
	if v > 0:
		# j feeds i → warm green, deepening with magnitude.
		return Color(0.12, 0.16, 0.12).lerp(Color(0.30, 0.86, 0.42), t)
	# j drains i → cool red.
	return Color(0.16, 0.12, 0.12).lerp(Color(0.90, 0.32, 0.30), t)


## Diagonal hatch (a few thin lines) — read-only decoration so the self-sink cell reads as "not an edge".
func _hatch(rect: Rect2) -> void:
	var step := 6.0
	var x := rect.position.x - rect.size.y
	while x < rect.position.x + rect.size.x:
		var a := Vector2(maxf(x, rect.position.x), rect.position.y + maxf(0.0, rect.position.x - x))
		var b := Vector2(minf(x + rect.size.y, rect.position.x + rect.size.x),
			rect.position.y + minf(rect.size.y, rect.position.x + rect.size.x - x))
		draw_line(a, b, Color(0.28, 0.30, 0.32, 0.55), 1.0)
		x += step


## Small clipped label (font drawn via the default theme font). Truncates by max width so long names don't bleed.
func _draw_small(text: String, at: Vector2, col: Color, max_w: float) -> void:
	var font := get_theme_default_font()
	if font == null:
		return
	var fs := 10
	var s := text
	while s.length() > 1 and font.get_string_size(s, HORIZONTAL_ALIGNMENT_LEFT, -1, fs).x > max_w:
		s = s.substr(0, s.length() - 1)
	draw_string(font, at, s, HORIZONTAL_ALIGNMENT_LEFT, -1, fs, col)
