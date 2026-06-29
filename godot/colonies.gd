extends Node2D
## Read-only COLONY POLYGON layer (ADR-029 S2) — the Field-scope de-spam.
##
## Instead of drawing up to MAX_DOTS_PER_CELL markers per non-empty cell (organisms.gd — a haze of thousands
## of near-overlapping dots), this layer aggregates the field into a handful of legible Cities-Skylines
## DISTRICT polygons: one polygon per contiguous run of cells that share the same (dominant_species_id,
## dominant_variant_id) key. A CRISPR brush mints a fresh off-hash Variant id on the covered organisms (S1),
## so the brushed disc carries a distinct dominant_variant_id → it falls out here as its OWN connected
## component nested inside the parent species' territory (a nested sub-polygon, tinted by a bounded
## intra-species hue shift so it reads as family, not a foreign species).
##
## INVARIANT #2 (STOP THE LINE if violated): PURE PRESENTATION GEOMETRY. The Rust core already expressed
## genotype→phenotype and projected the two INERT per-cell identity ordinals (dominant_species_id,
## dominant_variant_id) into the snapshot. This layer reads those integers + the SpeciesVisualMap table and
## emits pixels: connected-components → boundary contour → Douglas-Peucker → Chaikin → fill/outline/label.
## NO genome is read, NO phenotype is computed, NO biology decision is made in GDScript.
##
## INVARIANT #3 (DETERMINISM IN THE RENDERER): the connected-component labeling iterates the label image in
## ROW-MAJOR order ONLY (a single width*height Int array) and a two-pass union-find. Compacted colony ids are
## assigned in row-major FIRST-APPEARANCE order. The two Dictionaries used (root→compact-id, boundary
## adjacency) are touched by KEYED access only — NEVER iterated in hash order for any order-sensitive output.
## The draw order is sorted by a deterministic (depth, seq) key. Redraw fires only on set_snapshot, never per
## frame (mirrors organisms.gd's discipline). No randf()/time — nothing here is stochastic.

const SpeciesVisualMap := preload("res://species_visual_map.gd")

const MIN_COLONY_CELLS := 3        # below this a colony renders as a soft haze speck, not a full district (anti-flicker)
const SIMPLIFY_EPS := 0.75         # Douglas-Peucker tolerance, in CELL units (de-jaggy the grid-stepped contour)
const FILL_ALPHA := 0.52           # district fill translucency (the map still reads through stacked districts)
const HAZE_ALPHA := 0.13           # sub-MIN_COLONY_CELLS speck haze alpha
const VARIANT_HUE_RANGE := 0.18    # a brushed district's hue shifts up to ±half this AROUND the species base hue
const VARIANT_KEY_BASE := 65536    # u16 ceiling: pack key = species_id * BASE + variant_id (exact, no collision)

# Per-cell identity ordinals the core exported (row-major, w*h), the two INERT integers the polygons group by.
var _w: int = 0
var _h: int = 0
var _cell: float = 12.0
var _density: PackedFloat32Array        # the non-empty test: density <= 0 → background (not in any colony)
var _fitness: PackedFloat32Array        # per-cell fitness → district brightness (value channel) by colony mean
var _dominant_species: PackedFloat32Array
var _dominant_variant: PackedFloat32Array  # may be EMPTY on an older (pre-GSS6) snapshot → treated as all-0
var _species_table: Dictionary = {}     # species_id:int -> {size, color, is_plant, morph}; keyed lookups only
var _iso = null                         # iso.gd; when set, district polygons project into the 2:1 diamond field

# Row-major label image (compacted colony id per cell; -1 = background). Member so the boundary tracer reads it
# without copying the PackedInt32Array per colony. Rebuilt in _rebuild_colonies.
var _labels: PackedInt32Array

# Cached per-colony draw entries (built once per snapshot in _rebuild_colonies, consumed by _draw). Each is a
# Dictionary: either a haze speck {haze:true, centroid, color, count} or a district
# {haze:false, points(grid corners), fill, outline, width, label, centroid, depth, seq}.
var _colony_draw: Array = []

# Short presentation glyph per morphotype (label prefix). Presentation only — a lookup, never biology (inv #2).
const GLYPH_BY_MORPH := {
	"plant": "P",
	"mold": "M",
	"rod": "R",
	"cocci": "O",
	"vibrioid": "V",
	"pleomorph": "E",
	"symbiont": "S",
}


## Route the district polygons through the isometric transform (matches organisms.gd). null = orthographic.
func set_iso(iso) -> void:
	_iso = iso
	queue_redraw()


## Point the layer at a parsed snapshot (snapshot.gd instance), a cell size in px, and the per-species visual
## table (built in main.gd via SpeciesVisualMap.build_table). Pulls the two identity planes + density/fitness,
## rebuilds the colonies, and redraws. GUARD: a pre-GSS6 snapshot lacks dominant_variant_id → it is treated as
## all-zero (every cell's variant is the wild-type colony), so the layer never crashes on an older cdylib.
func set_snapshot(snap, cell: float, species_table: Dictionary) -> void:
	_w = int(snap.width)
	_h = int(snap.height)
	_cell = cell
	_species_table = species_table
	_density = snap.density
	_fitness = snap.fitness
	_dominant_species = snap.dominant_species_id if "dominant_species_id" in snap else PackedFloat32Array()
	_dominant_variant = snap.dominant_variant_id if "dominant_variant_id" in snap else PackedFloat32Array()
	_rebuild_colonies()
	queue_redraw()


## DETERMINISTIC connected-components + per-colony aggregates + geometry, all in row-major passes (inv #3).
func _rebuild_colonies() -> void:
	_colony_draw = []
	_labels = PackedInt32Array()
	var n := _w * _h
	if n <= 0 or _dominant_species.size() != n or _density.size() != n:
		return  # no/short identity plane → draw nothing (graceful; organisms.gd still renders at closer scopes)
	var have_variant := _dominant_variant.size() == n

	# --- per-cell key: species_id*BASE + variant_id for non-empty cells, -1 for background (zero-pop). ---
	var keys := PackedInt64Array()
	keys.resize(n)
	for i in n:
		if _density[i] > 0.0:
			var sid := int(round(_dominant_species[i]))
			var vid := 0
			if have_variant:
				vid = int(round(_dominant_variant[i]))
			keys[i] = sid * VARIANT_KEY_BASE + vid
		else:
			keys[i] = -1

	# --- PASS 1: provisional labels + union-find over (up, left) 4-connected neighbours, ROW-MAJOR. ---
	var prov := PackedInt32Array()  # provisional label per cell (-1 = background)
	prov.resize(n)
	var parent: Array = []          # union-find parent (Array = by-reference so _uf_find can path-compress)
	for y in _h:
		for x in _w:
			var i := y * _w + x
			if keys[i] == -1:
				prov[i] = -1
				continue
			var k := keys[i]
			var lab := -1
			if y > 0:
				var iu := i - _w
				if prov[iu] != -1 and keys[iu] == k:
					lab = _uf_find(parent, prov[iu])
			if x > 0:
				var il := i - 1
				if prov[il] != -1 and keys[il] == k:
					var ll := _uf_find(parent, prov[il])
					if lab == -1:
						lab = ll
					elif ll != lab:
						# union the two roots into the smaller (deterministic, order-independent merge)
						if lab < ll:
							parent[ll] = lab
						else:
							parent[lab] = ll
							lab = ll
			if lab == -1:
				lab = parent.size()
				parent.append(lab)  # a fresh provisional root points at itself
			prov[i] = lab

	# --- PASS 2: resolve roots → compact ids (assigned in row-major first-appearance order) + aggregates. ---
	_labels.resize(n)
	var root_to_cid: Dictionary = {}  # KEYED access only (has/get/set) — never iterated in hash order (inv #3)
	var agg_count := PackedInt32Array()
	var agg_sumfit := PackedFloat32Array()
	var agg_sumx := PackedInt64Array()
	var agg_sumy := PackedInt64Array()
	var agg_species := PackedInt32Array()
	var agg_variant := PackedInt32Array()
	var agg_minx := PackedInt32Array()
	var agg_miny := PackedInt32Array()
	var agg_maxx := PackedInt32Array()
	var agg_maxy := PackedInt32Array()
	for y in _h:
		for x in _w:
			var i := y * _w + x
			if prov[i] == -1:
				_labels[i] = -1
				continue
			var root := _uf_find(parent, prov[i])
			var cid: int
			if root_to_cid.has(root):
				cid = root_to_cid[root]
			else:
				cid = agg_count.size()
				root_to_cid[root] = cid
				agg_count.append(0)
				agg_sumfit.append(0.0)
				agg_sumx.append(0)
				agg_sumy.append(0)
				agg_species.append(int(keys[i] / VARIANT_KEY_BASE))
				agg_variant.append(int(keys[i] % VARIANT_KEY_BASE))
				agg_minx.append(x)
				agg_miny.append(y)
				agg_maxx.append(x)
				agg_maxy.append(y)
			_labels[i] = cid
			agg_count[cid] += 1
			agg_sumfit[cid] += (_fitness[i] if i < _fitness.size() else 0.0)
			agg_sumx[cid] += x
			agg_sumy[cid] += y
			agg_minx[cid] = mini(agg_minx[cid], x)
			agg_miny[cid] = mini(agg_miny[cid], y)
			agg_maxx[cid] = maxi(agg_maxx[cid], x)
			agg_maxy[cid] = maxi(agg_maxy[cid], y)

	# --- per-colony GEOMETRY: boundary contour → simplify → smooth → cached draw entry (row-major id order). ---
	for cid in agg_count.size():
		var count := agg_count[cid]
		var centroid := Vector2(float(agg_sumx[cid]) / float(count) + 0.5, float(agg_sumy[cid]) / float(count) + 0.5)
		var mean_fit := agg_sumfit[cid] / float(count)
		var sid := agg_species[cid]
		var vid := agg_variant[cid]
		var meta: Dictionary = _species_table.get(sid, {})
		var base_col: Color = meta.get("color", SpeciesVisualMap.COLOR_DEFAULT)
		var morph := str(meta.get("morph", "plant"))
		if count < MIN_COLONY_CELLS:
			# Tiny speck → a soft haze blob, not a full district (anti-flicker; the brief's risk #2).
			_colony_draw.append({"haze": true, "centroid": centroid, "color": base_col, "count": count})
			continue
		var loop := _trace_boundary(cid, agg_minx[cid], agg_miny[cid], agg_maxx[cid], agg_maxy[cid])
		if loop.size() < 3:
			_colony_draw.append({"haze": true, "centroid": centroid, "color": base_col, "count": count})
			continue
		loop = _simplify_closed(loop, SIMPLIFY_EPS)
		loop = _chaikin_once(loop)
		var fill := _fill_color(base_col, mean_fit, vid)
		var outline := fill.darkened(0.45)
		outline.a = 0.92
		var width := clampf(1.0 + 0.45 * sqrt(float(count)), 1.5, 6.0)
		var label := str(GLYPH_BY_MORPH.get(morph, "?")) + (" v%d" % vid if vid != 0 else "") + " " + str(count)
		_colony_draw.append({
			"haze": false, "points": loop, "fill": fill, "outline": outline, "width": width,
			"label": label, "centroid": centroid, "depth": centroid.x + centroid.y, "seq": cid,
		})

	# Deterministic draw order: back-to-front by iso depth (centroid.x+centroid.y), seq tiebreak. Districts only
	# (haze specks stay at the end of the list in their row-major id order — they draw last, faint, on top).
	_colony_draw.sort_custom(_draw_order_lt)


## Strict, deterministic ordering for the draw list: districts by (depth, seq); haze entries keep id order
## after districts. Pure comparator (no state) so the sort is reproducible.
func _draw_order_lt(a: Dictionary, b: Dictionary) -> bool:
	var ah: bool = a.get("haze", false)
	var bh: bool = b.get("haze", false)
	if ah != bh:
		return not ah  # districts (haze=false) before haze specks
	if ah:
		return false  # both haze → keep insertion (row-major) order
	if a["depth"] != b["depth"]:
		return a["depth"] < b["depth"]
	return a["seq"] < b["seq"]


## Union-find root with path compression. `parent` is an Array (by-reference) so compression persists (inv #3:
## this is order-independent — the SAME root regardless of traversal, so the compacted ids are deterministic).
func _uf_find(parent: Array, x: int) -> int:
	var r := x
	while parent[r] != r:
		r = parent[r]
	while parent[x] != r:
		var nx: int = parent[x]
		parent[x] = r
		x = nx
	return r


## Trace the boundary of colony `cid`'s cell mask into ONE ordered closed loop of grid-corner points (a
## marching-squares-equivalent edge walk at cell-edge resolution). Each filled cell contributes a directed
## boundary half-edge per neighbour that is NOT in the colony, wound so the edges chain into a loop. The start
## corner is the FIRST edge of the topmost-leftmost cell (row-major) → deterministic seed; the walk follows
## keyed adjacency lookups (never a hash-order iteration). Returns grid-corner coords (corner (cx,cy) is the
## top-left of cell (cx,cy); range 0.._w / 0.._h).
func _trace_boundary(cid: int, minx: int, miny: int, maxx: int, maxy: int) -> PackedVector2Array:
	var adj: Dictionary = {}  # start_corner_key -> Array[end_corner_key]; KEYED access only (inv #3)
	var start_key := -1
	var edge_count := 0
	for y in range(miny, maxy + 1):
		for x in range(minx, maxx + 1):
			var i := y * _w + x
			if _labels[i] != cid:
				continue
			# Emit a half-edge per side whose neighbour is outside the colony (or off-grid). Winding (top→left→
			# bottom→right, each toward the next corner) makes the per-cell edges chain into a single loop.
			# above
			if y == 0 or _labels[i - _w] != cid:
				var a0 := _corner(x + 1, y)
				_add_edge(adj, a0, _corner(x, y))
				edge_count += 1
				if start_key == -1:
					start_key = a0
			# left
			if x == 0 or _labels[i - 1] != cid:
				_add_edge(adj, _corner(x, y), _corner(x, y + 1))
				edge_count += 1
			# below
			if y == _h - 1 or _labels[i + _w] != cid:
				_add_edge(adj, _corner(x, y + 1), _corner(x + 1, y + 1))
				edge_count += 1
			# right
			if x == _w - 1 or _labels[i + 1] != cid:
				_add_edge(adj, _corner(x + 1, y + 1), _corner(x + 1, y))
				edge_count += 1
	var loop := PackedVector2Array()
	if start_key == -1:
		return loop
	var cur := start_key
	var steps := 0
	while true:
		loop.append(_key_to_grid(cur))
		var outs: Array = adj.get(cur, [])
		if outs.is_empty():
			break
		var nxt: int = outs.pop_back()  # consume one out-edge (array order = insertion order; deterministic)
		adj[cur] = outs
		cur = nxt
		steps += 1
		if cur == start_key or steps > edge_count + 2:
			break
	return loop


func _add_edge(adj: Dictionary, a: int, b: int) -> void:
	if adj.has(a):
		var arr: Array = adj[a]
		arr.append(b)
		adj[a] = arr
	else:
		adj[a] = [b]


func _corner(cx: int, cy: int) -> int:
	return cy * (_w + 1) + cx


func _key_to_grid(key: int) -> Vector2:
	var stride := _w + 1
	return Vector2(float(key % stride), float(key / stride))


## Douglas-Peucker simplify of a CLOSED loop: anchor at point 0 + the farthest point from it, DP each arc.
## Reduces the grid-stepped contour to a light polygon before smoothing. eps is in CELL units.
func _simplify_closed(pts: PackedVector2Array, eps: float) -> PackedVector2Array:
	var n := pts.size()
	if n < 4:
		return pts
	var far := 1
	var fardist := -1.0
	for i in range(1, n):
		var d := pts[0].distance_to(pts[i])
		if d > fardist:
			fardist = d
			far = i
	var keep := PackedByteArray()
	keep.resize(n)
	keep[0] = 1
	keep[far] = 1
	_dp(pts, 0, far, eps, keep)
	_dp(pts, far, n, eps, keep)  # second arc: far .. n (wraps to index 0)
	var out := PackedVector2Array()
	for i in n:
		if keep[i] == 1:
			out.append(pts[i])
	return out


## Recursive DP on the arc pts[i..j] (j may equal n, meaning the closing point is pts[0]). Marks kept indices.
func _dp(pts: PackedVector2Array, i: int, j: int, eps: float, keep: PackedByteArray) -> void:
	if j - i < 2:
		return
	var n := pts.size()
	var a := pts[i]
	var b := pts[j % n]
	var maxd := -1.0
	var maxi_idx := -1
	for m in range(i + 1, j):
		var d := _point_seg_dist(pts[m], a, b)
		if d > maxd:
			maxd = d
			maxi_idx = m
	if maxd > eps and maxi_idx != -1:
		keep[maxi_idx] = 1
		_dp(pts, i, maxi_idx, eps, keep)
		_dp(pts, maxi_idx, j, eps, keep)


func _point_seg_dist(p: Vector2, a: Vector2, b: Vector2) -> float:
	var ab := b - a
	var l2 := ab.length_squared()
	if l2 <= 0.0000001:
		return p.distance_to(a)
	var t := clampf((p - a).dot(ab) / l2, 0.0, 1.0)
	return p.distance_to(a + ab * t)


## One Chaikin corner-cutting pass on a CLOSED loop: each edge (P_i, P_i+1) → (0.75 P_i + 0.25 P_i+1) and
## (0.25 P_i + 0.75 P_i+1). Softens the district boundary into an organic hull.
func _chaikin_once(pts: PackedVector2Array) -> PackedVector2Array:
	var n := pts.size()
	if n < 3:
		return pts
	var out := PackedVector2Array()
	for i in n:
		var a := pts[i]
		var b := pts[(i + 1) % n]
		out.append(a * 0.75 + b * 0.25)
		out.append(a * 0.25 + b * 0.75)
	return out


## District fill colour: the species base colour, value brightened by the colony's mean fitness, hue shifted a
## bounded amount for a brushed variant (vid != 0) so the district reads as FAMILY (within-species), not a
## foreign species. Presentation only (a Color transform; no biology).
func _fill_color(base: Color, mean_fit: float, vid: int) -> Color:
	var h := base.h
	var s := base.s
	var v := clampf(0.40 + 0.55 * clampf(mean_fit, 0.0, 1.0), 0.25, 0.98)
	if vid != 0:
		h = fposmod(h + _variant_hue_shift(vid), 1.0)
	return Color.from_hsv(h, s, v, FILL_ALPHA)


## Bounded, deterministic per-variant hue offset in ±VARIANT_HUE_RANGE/2 (a presentation tint of the district).
func _variant_hue_shift(vid: int) -> float:
	var f := fposmod(float(vid) * 0.137, 1.0)  # spread successive ids around the cycle, deterministic
	return (f - 0.5) * VARIANT_HUE_RANGE


func _grid_to_world(cx: float, cy: float) -> Vector2:
	if _iso != null:
		return _iso.cell_to_screen(cx, cy, _cell)
	return Vector2(cx * _cell, cy * _cell)


func _draw() -> void:
	if _colony_draw.is_empty():
		return
	var font: Font = ThemeDB.fallback_font
	var fsize := 13
	for c in _colony_draw:
		if bool(c.get("haze", false)):
			var hp := _grid_to_world(c["centroid"].x, c["centroid"].y)
			var hcol: Color = c["color"]
			var hr := maxf(_cell * 0.45, _cell * 0.5 * sqrt(float(c["count"])))
			draw_circle(hp, hr, Color(hcol.r, hcol.g, hcol.b, HAZE_ALPHA))
			continue
		# Project the grid-corner contour into world space (ortho or iso) and draw fill + closed outline.
		var pts := PackedVector2Array()
		var grid: PackedVector2Array = c["points"]
		for gp in grid:
			pts.append(_grid_to_world(gp.x, gp.y))
		if pts.size() >= 3:
			draw_colored_polygon(pts, c["fill"])
			var outline := pts
			outline.append(pts[0])
			draw_polyline(outline, c["outline"], float(c["width"]), true)
		# Centred district label: species glyph (+ variant tag) + cell-footprint count.
		if font != null:
			var lp := _grid_to_world(c["centroid"].x, c["centroid"].y)
			var txt := str(c["label"])
			var tw: float = font.get_string_size(txt, HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x
			draw_string(font, lp - Vector2(tw * 0.5, -float(fsize) * 0.35), txt,
				HORIZONTAL_ALIGNMENT_LEFT, -1, fsize, Color(1.0, 1.0, 1.0, 0.94))
