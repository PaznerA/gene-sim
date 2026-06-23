extends Control
## Relations view — node-link GRAPH of the emergent trophic web (companion to the S×S FlowMatrix heatmap; Item 4).
##
## INVARIANT #2 (STOP THE LINE if violated): read-only presentation. NODES = species, laid out on a ring, SIZED by
## population (a core export) and COLOURED by the species' morphotype via SpeciesVisualMap — the SAME table the
## ecosystem field uses, so the graph and the map agree on which species is which. EDGES = the core-MEASURED
## FlowMatrix net joule flows (flat[i*s+j] = net J that flowed FROM source j INTO sink i); each edge is drawn
## source→sink with thickness/opacity ∝ |flow|. The renderer LAYS OUT finished integers + draws arrows; it computes
## NO biology (no flow derivation, no genotype→phenotype). The only arithmetic is DISPLAY scaling (max-abs / max-pop
## normalization + a circle layout) — identical in kind to relations_heatmap.gd's _max_abs ramp.
##
## INVARIANT #3: a deterministic draw of recorded integers — no RNG. Node angles are a pure function of the
## SpeciesId ordinal (index order = observe_species()/FlowMatrix order, by construction). NO class_name (the
## preload convention, resolves under a fresh --headless run); modeled on relations_heatmap.gd's read-only _draw().

const SpeciesVisualMap := preload("res://species_visual_map.gd")

const PAD := 18.0
const NODE_MIN_R := 7.0
const NODE_MAX_R := 30.0
const EDGE_MIN_W := 1.5
const EDGE_MAX_W := 9.0

var _names: PackedStringArray = PackedStringArray()   # SpeciesId order
var _keys: PackedStringArray = PackedStringArray()    # per-species glyph key (→ SpeciesVisualMap colour)
var _roles: PackedStringArray = PackedStringArray()   # per-species role (colour fallback)
var _pops: PackedInt64Array = PackedInt64Array()      # per-species population (→ node radius)
var _flat: PackedInt64Array = PackedInt64Array()      # flat s*s FlowMatrix (row-major)
var _s: int = 0
var _max_abs: int = 0  # off-diagonal max |flow| (edge-width scaling; 0 ⇒ no edges)
var _max_pop: int = 0  # max population (node-radius scaling; 0 ⇒ all min)


## Feed the graph: per-species metadata (names/keys/roles/pops, all in SpeciesId order) + the flat FlowMatrix.
## Recomputes the display-scaling maxes + redraws. A degenerate/short matrix renders NODES ONLY (no edges) — the
## same graceful-degrade discipline the heatmap uses for State 1/2. All inputs are finished core exports (inv #2).
func set_data(names: PackedStringArray, keys: PackedStringArray, roles: PackedStringArray,
		pops: PackedInt64Array, flat: PackedInt64Array, s: int) -> void:
	_names = names
	_keys = keys
	_roles = roles
	_pops = pops
	_flat = flat
	_s = maxi(0, s)
	_max_abs = 0
	_max_pop = 0
	for p in _pops:
		if int(p) > _max_pop:
			_max_pop = int(p)
	if _s > 0 and _flat.size() == _s * _s:
		for i in _s:
			for j in _s:
				if i == j:
					continue
				var a: int = absi(int(_flat[i * _s + j]))
				if a > _max_abs:
					_max_abs = a
	queue_redraw()


## Ring position of node i (start at the top, go clockwise). A single species sits at the centre.
func _node_pos(i: int, n: int, center: Vector2, radius: float) -> Vector2:
	if n <= 1:
		return center
	var ang := -PI * 0.5 + float(i) * TAU / float(n)
	return center + Vector2(cos(ang), sin(ang)) * radius


## Node radius from population (sqrt scale so AREA ~ population — a fairer visual weight than a linear radius).
func _node_radius(i: int) -> float:
	if _max_pop <= 0 or i >= _pops.size():
		return NODE_MIN_R
	var t := sqrt(clampf(float(_pops[i]) / float(_max_pop), 0.0, 1.0))
	return lerpf(NODE_MIN_R, NODE_MAX_R, t)


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.0, 0.0, 0.0, 0.32))
	var n := _names.size()
	if n == 0:
		n = _s
	if n <= 0:
		return
	var center := size * 0.5
	var ring := maxf(36.0, minf(size.x, size.y) * 0.5 - NODE_MAX_R - PAD)

	var pos: Array = []  # [Vector2] node centres, index = SpeciesId
	for i in n:
		pos.append(_node_pos(i, n, center, ring))

	# EDGES first (under the nodes): one per unordered pair {a,b}, oriented SOURCE→SINK (the gainer), exactly the
	# orientation main.gd's _format_flow_summary uses so the arrows agree with the narrated "primary flows" line.
	var have := (_s == n and _flat.size() == _s * _s)
	if have:
		for a in n:
			for b in range(a + 1, n):
				var v: int = int(_flat[b * _s + a])  # net J from a into b
				if v == 0:
					continue
				var src := a
				var dst := b
				if v < 0:
					src = b
					dst = a
					v = -v
				_draw_edge(pos[src], pos[dst], _node_radius(src), _node_radius(dst), v)

	# NODES on top.
	for i in n:
		var r := _node_radius(i)
		var key := _keys[i] if i < _keys.size() else "default"
		var role := _roles[i] if i < _roles.size() else ""
		var col: Color = SpeciesVisualMap.color_for(key, role)
		draw_circle(pos[i], r, col)
		draw_arc(pos[i], r, 0.0, TAU, 28, Color(0.0, 0.0, 0.0, 0.55), 1.5)
		var pop: int = int(_pops[i]) if i < _pops.size() else 0
		var nm := _names[i] if i < _names.size() else "sp%d" % i
		_draw_label("%s · %d" % [nm, pop], pos[i] + Vector2(0.0, r + 4.0))


## A directed edge SOURCE→SINK: a line whose thickness/opacity ∝ |v|/max-abs + an arrowhead at the sink rim.
## Warm green (j feeds i) — consistent with the heatmap's "j feeds i = warm green". Endpoints pulled back to the
## node rims so the arrow touches the circle, not its centre. Pure display geometry (inv #2/#3).
func _draw_edge(p_src: Vector2, p_dst: Vector2, r_src: float, r_dst: float, v: int) -> void:
	var d := p_dst - p_src
	var l := d.length()
	if l < 0.001:
		return
	d /= l
	var a := p_src + d * r_src
	var b := p_dst - d * r_dst
	var t := 1.0 if _max_abs <= 0 else clampf(float(v) / float(_max_abs), 0.0, 1.0)
	var w := lerpf(EDGE_MIN_W, EDGE_MAX_W, t)
	var col := Color(0.30, 0.86, 0.42, lerpf(0.32, 0.95, t))
	draw_line(a, b, col, w)
	# Arrowhead at the sink end.
	var back := -d
	var head := w + 9.0
	var left := b + back.rotated(0.42) * head
	var right := b + back.rotated(-0.42) * head
	draw_colored_polygon(PackedVector2Array([b, left, right]), col)


## Small centered label under a node (default theme font). Read-only decoration.
func _draw_label(text: String, at: Vector2) -> void:
	var font := get_theme_default_font()
	if font == null:
		return
	var fs := 10
	var w := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs).x
	draw_string(font, at - Vector2(w * 0.5, 0.0), text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs, Color(0.92, 0.96, 0.92))
