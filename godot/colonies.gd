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

# ADR-029 S3 — the LOD POP ladder, keyed on the on-screen FOOTPRINT (§4.1: footprint_px = _cell * zoom *
# size_scale). The rung is a PURE FUNCTION of footprint — NO per-frame timer (inv #3 renderer discipline).
const STIPPLE_MIN_PX := 3.0        # below this footprint a district is polygon-ONLY (individuals would be sub-pixel)
const POP_LO_PX := 6.0             # crossfade band start: the fill begins fading; organisms.gd begins fading sprites in
const POP_HI_PX := 8.0             # crossfade band end: district fully POPPED → only a thin ghost fill + outline remain
const GHOST_FILL_FACTOR := 0.15    # fully-popped fill-alpha factor (the district frame survives as a faint ghost)
const STIPPLE_MAX_ALPHA := 0.5     # peak per-cell density-stipple alpha across the mid band

# ADR-029 S5 — PLANT REALISM (renderer-only, inv #2 presentation). A PLANT colony (is_plant from the core species
# table) is the always-visible, most-realistic aggregate: it never decays into background haze and reads as a soft
# vegetation CANOPY mass rather than an abstract hard-edged zone. These knobs branch the plant path from the microbe
# path; microbe behaviour is byte-for-byte unchanged. All are presentation constants — no biology (the canopy SHAPE
# driver, branchiness/reflectance, already came from the core species traits via organisms.gd's template).
const PLANT_GHOST_FILL_FACTOR := 0.40  # plant popped-fill floor — ABOVE the microbe GHOST_FILL_FACTOR (0.15) so the canopy frame stays visible at EVERY zoom
const PLANT_CHAIKIN_PASSES := 2        # the plant contour gets EXTRA corner-cutting → a rounder, organic canopy hull (microbe = 1 hard pass)
const PLANT_MIN_OUTLINE_WIDTH := 2.0   # plant outline width FLOOR — the always-visible SIZE floor (the canopy frame never thins to nothing)
const PLANT_CANOPY_CORE_LIGHTEN := 0.18  # canopy gradient: lighten toward the dense centroid core
const PLANT_CANOPY_RIM_DARKEN := 0.22    # canopy gradient: darken toward the soft rim
const PLANT_CANOPY_RIM_ALPHA := 0.72     # canopy gradient: the rim fades a touch → a soft mass, not a flat zone

# ADR-029 S6 — LABEL DECLUTTER (renderer-only presentation; inv #2/#3). A label under EVERY district turns the
# Field scope back into noise. Only the SELECTED district + districts at/above LABEL_MIN_CELLS earn a label, and a
# deterministic de-overlap pass then drops any label whose centroid sits within LABEL_MIN_SEP_CELLS of an
# already-placed HIGHER-priority one (priority = selected first, then larger cell-count, then row-major seq). The
# whole pass is ordered + closed-form — no randf()/time, no hash-order iteration — so the labelling is reproducible.
const LABEL_MIN_CELLS := 6        # a district below this cell-count is UNLABELED unless it is the selected district
const LABEL_MIN_SEP_CELLS := 4.0  # two label anchors closer than this (in CELL units) → keep only the higher-priority one

# Per-cell identity ordinals the core exported (row-major, w*h), the two INERT integers the polygons group by.
var _w: int = 0
var _h: int = 0
var _cell: float = 12.0
var _zoom: float = 1.0                   # live cam.zoom.x (threaded from main.gd) → per-district footprint = _cell*zoom*size_scale
var _density: PackedFloat32Array        # the non-empty test: density <= 0 → background (not in any colony)
var _fitness: PackedFloat32Array        # per-cell fitness → district brightness (value channel) by colony mean
var _dominant_species: PackedFloat32Array
var _dominant_variant: PackedFloat32Array  # may be EMPTY on an older (pre-GSS6) snapshot → treated as all-0
var _species_table: Dictionary = {}     # species_id:int -> {size, color, is_plant, morph}; keyed lookups only
var _iso = null                         # iso.gd; when set, district polygons project into the 2:1 diamond field

# ADR-029 S4 — renderer-side COLONY REGISTRY: variant_id:int -> {species, label, color, gen_created, parent}.
# Assembled in main.gd (ordered passes) from observe_species() + the journaled ApplyEditRegion brush strokes; used
# here for a brushed district's STABLE family name + colour (a brushed child reads as the same species, not a foreign
# one). KEYED access only (has/get) — NEVER iterated for any order-sensitive output (inv #3).
var _registry: Dictionary = {}
# ADR-029 S4 — the SELECTED colony key (species_id*VARIANT_KEY_BASE+variant_id), set on a world-click in main.gd.
# A selected district forces its fill to the popped GHOST so organisms.gd can explode it to members (the selected-pop)
# regardless of zoom while neighbours stay aggregated. -1 = nothing selected. Presentation-only (inv #2).
var _selected_key: int = -1

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


## ADR-029 S3: thread the live camera magnification (cam.zoom.x) in from main.gd so each district can pick its LOD
## rung from its on-screen FOOTPRINT (§4.1: footprint_px = _cell * zoom * size_scale) — NOT from a per-frame timer.
## main.gd calls this from _set_zoom / _update_scope_layers, so a wheel/scope event re-pops the whole ladder with a
## single state-change redraw. inv #3: redraw fires only on a zoom/snapshot change, never from _process. Guarded so
## an unchanged zoom (e.g. a same-zoom _show during live play) does not thrash a redundant redraw.
func set_zoom(zoom: float) -> void:
	if is_equal_approx(zoom, _zoom):
		return
	_zoom = zoom
	queue_redraw()


## ADR-029 S4: hand in the renderer-side colony registry (main.gd owns its assembly). Stores it for the NEXT
## set_snapshot rebuild (which reads it for a brushed district's stable family name + colour) and redraws. No
## rebuild here — main.gd calls this BEFORE set_snapshot. Keyed reads only downstream (inv #3).
func set_colony_registry(reg: Dictionary) -> void:
	_registry = reg
	queue_redraw()


## ADR-029 S4: the SELECTED colony (packed species*BASE+variant key, or -1). A district whose key matches forces
## its fill to the popped ghost so organisms.gd explodes it to members regardless of zoom. Guarded so a no-op
## reselect doesn't thrash a redundant redraw (inv #3: redraw only on a real state change).
func set_selected_colony(key: int) -> void:
	if key == _selected_key:
		return
	_selected_key = key
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
		# ADR-029 S5: is this colony a PLANT (the core species table's is_plant flag)? Plants get the always-visible
		# district floor (no haze speck) + the soft canopy hull + the higher ghost-fill floor; microbes keep the
		# hard district. Branch on this one read — the biology decision (autotroph vs microbe) was made in the core.
		var is_plant := bool(meta.get("is_plant", false))
		# S3: the district's species SIZE multiplier (plant 2.2 … symbiont 0.34) — the §4.1 footprint term carried
		# into the draw entry so plant districts cross the pop threshold FIRST (do NOT clamp it away).
		var size_scale := float(meta.get("size", SpeciesVisualMap.SIZE_DEFAULT))
		var key := sid * VARIANT_KEY_BASE + vid
		# ADR-029 S5 always-visible FLOOR: a PLANT colony is NEVER a haze speck — it always renders as a full
		# district (fill + outline) even at 1-2 cells, so it can never decay into background haze (the brief's
		# "plants always visible"). MICROBES keep the sub-MIN_COLONY_CELLS haze speck (anti-flicker; risk #2) —
		# microbe behaviour is unchanged.
		if count < MIN_COLONY_CELLS and not is_plant:
			_colony_draw.append({"haze": true, "centroid": centroid, "color": base_col, "count": count})
			continue
		# ADR-029 S4 — trace ALL boundary loops: loops[0] = the outer hull, loops[1..] = interior HOLES. A brushed
		# child district is its OWN (species,variant) colony, so the parent species territory has a HOLE punched
		# where the child sits → the parent renders as a FRAME around the child (the S2 deferred hole-cut). Each
		# loop is simplified + smoothed; the row-major tracer + sorted hole seeds keep this deterministic (inv #3).
		var raw_loops := _trace_boundaries(cid, agg_minx[cid], agg_miny[cid], agg_maxx[cid], agg_maxy[cid])
		if raw_loops.is_empty() or (raw_loops[0] as PackedVector2Array).size() < 3:
			_colony_draw.append({"haze": true, "centroid": centroid, "color": base_col, "count": count})
			continue
		var loops: Array = []
		for rl in raw_loops:
			var s := _simplify_closed(rl, SIMPLIFY_EPS)
			if is_plant:
				# ADR-029 S5 soft CANOPY HULL: never let over-simplification collapse a tiny plant loop below a
				# triangle (it must stay a DISTRICT, not haze — the always-visible floor), THEN give it EXTRA
				# Chaikin passes so the boundary reads as a rounder, organic canopy mass rather than a hard edge.
				if s.size() < 3:
					s = rl
				for _pass in PLANT_CHAIKIN_PASSES:
					s = _chaikin_once(s)
			else:
				s = _chaikin_once(s)  # microbe: ONE pass → the clean hard-edged Cities-Skylines district (unchanged)
			if s.size() >= 3:
				loops.append(s)
		if loops.is_empty():
			_colony_draw.append({"haze": true, "centroid": centroid, "color": base_col, "count": count})
			continue
		# S4 registry: a brush-minted child (vid != 0) takes its STABLE family colour from the renderer-side colony
		# registry (so the district colour does not drift with mean fitness across frames). Keyed lookup only
		# (inv #3). Absent (wild-type vid 0 / unregistered) → the species base colour + the bounded computed hue shift.
		var entry: Dictionary = _registry.get(vid, {}) if vid != 0 else {}
		var fill: Color
		if not entry.is_empty():
			fill = _fill_color(entry.get("color", base_col), mean_fit, 0)  # registry colour already carries the family hue
		else:
			fill = _fill_color(base_col, mean_fit, vid)
		var outline := fill.darkened(0.45)
		outline.a = 0.92
		var width := clampf(1.0 + 0.45 * sqrt(float(count)), 1.5, 6.0)
		var label := _district_label(entry, morph, vid, count)
		_colony_draw.append({
			"haze": false, "points": loops[0], "loops": loops, "has_holes": loops.size() > 1,
			"fill": fill, "outline": outline, "width": width,
			"label": label, "centroid": centroid, "depth": centroid.x + centroid.y, "seq": cid,
			# S3 LOD ladder + S4 selection: the species size term, the colony id/bbox (for the bounded row-major
			# stipple + hole-cut cell-quad fill), and the packed (species,variant) key the selected-pop matches on.
			# S6: `count` (the colony's live cell/pop count) for the label-declutter threshold + the inspect card.
			"cid": cid, "key": key, "size_scale": size_scale, "is_plant": is_plant, "count": count,
			"minx": agg_minx[cid], "miny": agg_miny[cid], "maxx": agg_maxx[cid], "maxy": agg_maxy[cid],
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


## ADR-029 S4: trace ALL boundary loops of colony `cid`'s cell mask (a marching-squares-equivalent edge walk at
## cell-edge resolution). loops[0] = the OUTER hull (seeded from the FIRST edge of the topmost-leftmost cell —
## deterministic); loops[1..] = interior HOLES (e.g. a brushed child district nested inside its parent territory).
## Each filled cell contributes a directed boundary half-edge per neighbour NOT in the colony, wound so the edges
## chain into loops. The outer loop walks first; remaining loops are seeded from the SMALLEST unconsumed corner key
## (adj.keys() SORTED → ordered, never a hash-order walk; inv #3). Grid-corner coords: corner (cx,cy) = top-left of
## cell (cx,cy), range 0.._w / 0.._h.
func _trace_boundaries(cid: int, minx: int, miny: int, maxx: int, maxy: int) -> Array:
	var adj: Dictionary = {}  # start_corner_key -> Array[end_corner_key]; KEYED access only (inv #3)
	var start_key := -1
	var edge_count := 0
	for y in range(miny, maxy + 1):
		for x in range(minx, maxx + 1):
			var i := y * _w + x
			if _labels[i] != cid:
				continue
			# Emit a half-edge per side whose neighbour is outside the colony (or off-grid). Winding (top→left→
			# bottom→right, each toward the next corner) makes the per-cell edges chain into closed loops.
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
	var loops: Array = []
	if start_key == -1:
		return loops
	# Outer hull first (deterministic seed = topmost-leftmost cell's top edge).
	var outer := _walk_loop(adj, start_key, edge_count)
	if outer.size() >= 3:
		loops.append(outer)
	# Remaining loops = interior holes. Seed each from the smallest unconsumed corner key (SORTED keys → inv #3).
	var rem: Array = adj.keys()
	rem.sort()
	for k in rem:
		var outs: Array = adj.get(k, [])
		if outs.is_empty():
			continue
		var hole := _walk_loop(adj, k, edge_count)
		if hole.size() >= 3:
			loops.append(hole)
	return loops


## Walk one closed boundary loop from `start`, consuming out-edges (pop_back = insertion order, deterministic).
## Stops on return to `start` or after edge_count+2 steps (safety). Returns grid-corner points.
func _walk_loop(adj: Dictionary, start: int, edge_count: int) -> PackedVector2Array:
	var loop := PackedVector2Array()
	var cur := start
	var steps := 0
	while true:
		loop.append(_key_to_grid(cur))
		var outs: Array = adj.get(cur, [])
		if outs.is_empty():
			break
		var nxt: int = outs.pop_back()
		adj[cur] = outs
		cur = nxt
		steps += 1
		if cur == start or steps > edge_count + 2:
			break
	return loop


## ADR-029 S4: a district's label — registry name · variant · count for a brush-minted child (so it reads as a
## NAMED family district), else the species morph glyph (+ variant tag) + cell-footprint count. Keyed lookup only.
func _district_label(entry: Dictionary, morph: String, vid: int, count: int) -> String:
	if not entry.is_empty():
		return "%s v%d · %d" % [str(entry.get("label", "?")), vid, count]
	return str(GLYPH_BY_MORPH.get(morph, "?")) + (" v%d" % vid if vid != 0 else "") + " " + str(count)


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
		# S3 LOD ladder — pick this district's rung from its on-screen FOOTPRINT (§4.1: _cell * zoom * size_scale).
		# Because size_scale is IN the footprint, PLANT districts (2.2) cross the band first while microbe districts
		# (rod 0.9 … symbiont 0.34) stay solid polygons — the brief's "by organism size, pop open", for free. The
		# whole transition is a PURE FUNCTION of footprint (no per-frame timer — inv #3): the fill alpha ramps
		# 1.0 → GHOST_FILL_FACTOR across the POP_LO..POP_HI crossfade band while organisms.gd ramps its per-cell
		# sprites 0 → 1 over the SAME band; the outline + label always draw so the district frame survives the pop.
		var size_scale: float = float(c.get("size_scale", 1.0))
		var is_plant: bool = bool(c.get("is_plant", false))
		var foot := _cell * _zoom * size_scale
		var pop_t := clampf((foot - POP_LO_PX) / (POP_HI_PX - POP_LO_PX), 0.0, 1.0)
		# S4 selected-pop: a clicked district forces FULL pop (fill → ghost) regardless of zoom, so organisms.gd
		# explodes it to members while neighbours keep their footprint-driven rung. Pure key match (inv #2/#3).
		if _selected_key >= 0 and int(c.get("key", -1)) == _selected_key:
			pop_t = 1.0
		# ADR-029 S5 always-visible FLOOR: a PLANT district's popped ghost fill floors at PLANT_GHOST_FILL_FACTOR
		# (> the microbe GHOST_FILL_FACTOR) so the canopy frame stays visible at EVERY zoom — it never fades into
		# background haze the way a fully-popped microbe district does. The microbe ghost factor is unchanged.
		var ghost: float = PLANT_GHOST_FILL_FACTOR if is_plant else GHOST_FILL_FACTOR
		var fill_factor := lerpf(1.0, ghost, pop_t)  # 1.0 full district → ghost floor when popped (thin ghost + outline)
		# Project the OUTER grid-corner contour into world space (ortho or iso).
		var pts := PackedVector2Array()
		var grid: PackedVector2Array = c["points"]
		for gp in grid:
			pts.append(_grid_to_world(gp.x, gp.y))
		var has_holes: bool = bool(c.get("has_holes", false))
		if pts.size() >= 3:
			var fill: Color = c["fill"]
			fill.a *= fill_factor
			if has_holes:
				# S4 hole-cut: this parent territory encloses a brushed child district → CUT the child region out
				# of the parent fill so the parent reads as a FRAME around the child (the child draws its own
				# family-tinted fill in the hole). Bridge+triangulate for one hole; cell-quad masked fill for many.
				_draw_holed_fill(c, fill)
			elif is_plant:
				# ADR-029 S5: a plant aggregate is a SOFT CANOPY MASS, not a flat zone — fill with a green radial
				# gradient (brighter/denser at the centroid core, softening to the rim) so it reads as vegetation.
				_draw_canopy_fill(pts, _grid_to_world(c["centroid"].x, c["centroid"].y), fill)
			else:
				draw_colored_polygon(pts, fill)  # microbe: a flat hard-edged district (unchanged)
			# Mid-band internal heat: a per-cell density stipple that grows across [STIPPLE_MIN..POP_LO] and fades as
			# the colony pops [POP_LO..POP_HI] (organisms.gd's sprites take over). Pure function of footprint.
			var stipple_a := clampf((foot - STIPPLE_MIN_PX) / (POP_LO_PX - STIPPLE_MIN_PX), 0.0, 1.0) * (1.0 - pop_t)
			if stipple_a > 0.01:
				_draw_density_stipple(c, stipple_a)
			# Outline: the outer hull always; for a holed parent, ALSO each hole loop so the inner frame edge
			# around the child reads as a clean district boundary. ADR-029 S5: a PLANT outline width floors at
			# PLANT_MIN_OUTLINE_WIDTH (the always-visible SIZE floor) so the canopy frame never thins to nothing.
			var ow: float = maxf(float(c["width"]), PLANT_MIN_OUTLINE_WIDTH) if is_plant else float(c["width"])
			if has_holes:
				for lp in c.get("loops", []):
					var wl := _project_loop(lp)
					if wl.size() >= 2:
						wl.append(wl[0])
						draw_polyline(wl, c["outline"], ow, true)
			else:
				var outline := pts
				outline.append(pts[0])
				draw_polyline(outline, c["outline"], ow, true)

	# ADR-029 S6 LABEL DECLUTTER: draw labels in a SEPARATE, deterministic second pass over only the decluttered
	# subset (_label_plan: the selected district + districts >= LABEL_MIN_CELLS, de-overlapped by centroid distance,
	# highest-priority first) so the Field scope reads clean instead of a fog of overlapping names. Ordered, no
	# randf/time (inv #3). Centred label: registry family name (or species glyph) + variant tag + cell-footprint count.
	if font != null:
		for c in _label_plan():
			var lp := _grid_to_world(c["centroid"].x, c["centroid"].y)
			var txt := str(c["label"])
			var tw: float = font.get_string_size(txt, HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x
			draw_string(font, lp - Vector2(tw * 0.5, -float(fsize) * 0.35), txt,
				HORIZONTAL_ALIGNMENT_LEFT, -1, fsize, Color(1.0, 1.0, 1.0, 0.94))


## ADR-029 S6 LABEL DECLUTTER plan: the deterministic subset of DISTRICT entries (non-haze) that earn a label this
## frame. First a THRESHOLD filter (keep the selected district + any district with count >= LABEL_MIN_CELLS), then a
## PRIORITY de-overlap (selected first, then larger count, then row-major seq) that greedily drops any label whose
## centroid is within LABEL_MIN_SEP_CELLS of an already-placed higher-priority one. Pure + ordered (inv #3): a total
## sort key + a row-major greedy placement, no randf/time, no hash-order iteration. Also the test surface for the
## declutter (a tiny district is suppressed; the selected district is always kept). Read-only (inv #2).
func _label_plan() -> Array:
	var sel := _selected_key
	var cand: Array = []
	for c in _colony_draw:
		if bool(c.get("haze", false)):
			continue
		var is_sel := sel >= 0 and int(c.get("key", -1)) == sel
		if not is_sel and int(c.get("count", 0)) < LABEL_MIN_CELLS:
			continue
		cand.append(c)
	cand.sort_custom(func(a, b):
		var asel: bool = sel >= 0 and int(a.get("key", -1)) == sel
		var bsel: bool = sel >= 0 and int(b.get("key", -1)) == sel
		if asel != bsel:
			return asel  # the selected district sorts first → it is always placed
		var ac := int(a.get("count", 0))
		var bc := int(b.get("count", 0))
		if ac != bc:
			return ac > bc  # larger districts win an overlap
		return int(a.get("seq", 0)) < int(b.get("seq", 0)))  # row-major tiebreak → total order
	var placed: Array = []  # centroids (cell coords) of labels already accepted this frame
	var out: Array = []
	for c in cand:
		var ctr: Vector2 = c["centroid"]
		var is_sel := sel >= 0 and int(c.get("key", -1)) == sel
		var ok := true
		if not is_sel:
			for pc in placed:
				if (pc as Vector2).distance_to(ctr) < LABEL_MIN_SEP_CELLS:
					ok = false
					break
		if ok:
			placed.append(ctr)
			out.append(c)
	return out


## ADR-029 S6 PERF-LEVER accessor: the number of DISTRICT draw entries (non-haze polygons) the Field-scope colony
## layer built for the current snapshot. This is O(#colonies) — bounded by the connected-region count and
## INDEPENDENT of cells × MAX_DOTS_PER_CELL — so it stays small (tens) for a mostly-single-species field even as the
## cell count grows 4×. Deterministic read of the cached draw list (inv #3). colony_s6_test.gd asserts it stays flat.
func district_count() -> int:
	var n := 0
	for c in _colony_draw:
		if not bool(c.get("haze", false)):
			n += 1
	return n


## ADR-029 S6: the TOTAL cached Field-scope draw primitives (districts + sub-MIN_COLONY_CELLS haze specks) — also
## O(#colonies), the figure that replaces the old O(cells × MAX_DOTS_PER_CELL) per-organism dot count. Read-only.
func draw_entry_count() -> int:
	return _colony_draw.size()


## ADR-029 S6 DISTRICT INSPECT: the selected district's summary for main.gd's inspect card — the renderer-side
## registry fields {species, variant, label, gen_created, parent} joined with the LIVE cell/pop count from the cached
## draw entry. {} when nothing is selected or the selected key is not a current district. KEYED reads only (inv #3);
## a pure read of already-built render state (inv #2 — no genotype→phenotype). Cleared by main.gd on deselect.
func selected_colony_summary() -> Dictionary:
	if _selected_key < 0:
		return {}
	for c in _colony_draw:
		if bool(c.get("haze", false)):
			continue
		if int(c.get("key", -1)) != _selected_key:
			continue
		var vid := _selected_key % VARIANT_KEY_BASE
		var reg: Dictionary = _registry.get(vid, {})
		return {
			"species": _selected_key / VARIANT_KEY_BASE,
			"variant": vid,
			"key": _selected_key,
			"count": int(c.get("count", 0)),
			"label": str(reg.get("label", "")),
			"gen_created": int(reg.get("gen_created", -1)),
			"parent": int(reg.get("parent", -1)),
			"is_plant": bool(c.get("is_plant", false)),
		}
	return {}


## S3 mid-band INTERNAL HEAT: a per-cell density stipple over district `c`'s own cells, alpha scaled by `alpha`
## (the closed-form mid-band weight from _draw) × the per-cell density. Iterates THIS colony's bounding box in
## ROW-MAJOR order, keyed on the member _labels image (inv #3: no hash-order iteration, no randf/time). Bounded to
## the colony bbox and only invoked while the district sits in the ~3-7px stipple band, so it never re-spams.
func _draw_density_stipple(c: Dictionary, alpha: float) -> void:
	var cid: int = int(c.get("cid", -1))
	if cid < 0 or _labels.size() != _w * _h:
		return
	var minx: int = int(c.get("minx", 0))
	var miny: int = int(c.get("miny", 0))
	var maxx: int = int(c.get("maxx", 0))
	var maxy: int = int(c.get("maxy", 0))
	var base: Color = c["fill"]
	var heat := base.lightened(0.28)  # the district colour, brightened → reads as internal density heat
	var r := maxf(0.8, _cell * 0.18)
	for y in range(miny, maxy + 1):
		for x in range(minx, maxx + 1):
			var i := y * _w + x
			if i < 0 or i >= _labels.size() or _labels[i] != cid:
				continue
			var d := clampf(_density[i], 0.0, 1.0) if i < _density.size() else 0.0
			if d <= 0.0:
				continue
			var p := _grid_to_world(float(x) + 0.5, float(y) + 0.5)
			draw_circle(p, r * (0.4 + 0.6 * d), Color(heat.r, heat.g, heat.b, alpha * STIPPLE_MAX_ALPHA * d))


## Project a grid-corner loop into world space (ortho or iso). Presentation geometry only.
func _project_loop(grid: PackedVector2Array) -> PackedVector2Array:
	var out := PackedVector2Array()
	for gp in grid:
		out.append(_grid_to_world(gp.x, gp.y))
	return out


## ADR-029 S4 hole-cut: fill the parent district MINUS its holes. ONE hole (the common single-brush case) →
## bridge the hole into the outer ring (a zero-width keyhole seam) → triangulate the weakly-simple ring → draw the
## frame as a triangle fan (the hole region triangulates to nothing). MANY holes (or a degenerate bridge) → a
## robust per-cell masked fill: draw each parent CELL's quad (the hole cells are simply NOT in this colony's mask,
## so they are never drawn). Both paths are deterministic (no randf/time, row-major / sorted iteration; inv #3).
func _draw_holed_fill(c: Dictionary, fill: Color) -> void:
	var loops: Array = c.get("loops", [])
	if loops.size() < 2:
		return
	var outer := _project_loop(loops[0])
	var holes: Array = []
	for hi in range(1, loops.size()):
		holes.append(_project_loop(loops[hi]))
	if holes.size() == 1:
		var ring := _eliminate_holes(outer, holes)
		var tri := Geometry2D.triangulate_polygon(ring)
		if tri.size() >= 3:
			for t in range(0, tri.size(), 3):
				draw_colored_polygon(PackedVector2Array([ring[tri[t]], ring[tri[t + 1]], ring[tri[t + 2]]]), fill)
			return
	_draw_cell_quads(c, fill)


## Robust hole-respecting fill fallback: draw each of district `c`'s OWN cells as a quad (the holes are cells not
## in this colony's mask → never drawn). Bounded to the colony bbox, row-major, keyed on _labels (inv #3).
func _draw_cell_quads(c: Dictionary, fill: Color) -> void:
	var cid: int = int(c.get("cid", -1))
	if cid < 0 or _labels.size() != _w * _h:
		return
	var minx: int = int(c.get("minx", 0))
	var miny: int = int(c.get("miny", 0))
	var maxx: int = int(c.get("maxx", 0))
	var maxy: int = int(c.get("maxy", 0))
	for y in range(miny, maxy + 1):
		for x in range(minx, maxx + 1):
			var i := y * _w + x
			if i < 0 or i >= _labels.size() or _labels[i] != cid:
				continue
			draw_colored_polygon(PackedVector2Array([
				_grid_to_world(float(x), float(y)), _grid_to_world(float(x + 1), float(y)),
				_grid_to_world(float(x + 1), float(y + 1)), _grid_to_world(float(x), float(y + 1))]), fill)


## ADR-029 S5 plant CANOPY fill — a SOFT canopy mass (vs the microbe district's flat zone). Fills the hull with a
## green radial gradient: brighter/denser at the colony centroid core, softening (darker + a touch more translucent)
## toward the rim, so a plant aggregate reads as vegetation rather than an abstract zone. The mean-fitness/density
## value is already baked into `fill` (the species canopy palette via _fill_color); this adds the centroid→rim
## falloff. PURE PRESENTATION (inv #2 — a per-vertex Color lerp, no biology). DETERMINISTIC (inv #3): a closed-form
## distance lerp in row order, no randf()/time. draw_polygon takes per-vertex colours (same triangulation path as
## the flat draw_colored_polygon the microbe districts use, so concave hulls render identically).
func _draw_canopy_fill(pts: PackedVector2Array, center: Vector2, fill: Color) -> void:
	var n := pts.size()
	if n < 3:
		return
	var core := fill.lightened(PLANT_CANOPY_CORE_LIGHTEN)
	core.a = fill.a
	var rim := fill.darkened(PLANT_CANOPY_RIM_DARKEN)
	rim.a = fill.a * PLANT_CANOPY_RIM_ALPHA
	var maxd := 0.0001
	for p in pts:
		maxd = maxf(maxd, p.distance_to(center))
	var colors := PackedColorArray()
	colors.resize(n)
	for i in n:
		var t := clampf(pts[i].distance_to(center) / maxd, 0.0, 1.0)
		colors[i] = core.lerp(rim, t)
	draw_polygon(pts, colors)


## Signed area of a closed polygon (shoelace; this y-down convention: >0 = clockwise on screen). Pure geometry.
func _signed_area(p: PackedVector2Array) -> float:
	var a := 0.0
	var n := p.size()
	for i in n:
		var q := p[(i + 1) % n]
		a += (q.x - p[i].x) * (q.y + p[i].y)
	return a


## Bridge each hole into the outer ring (earcut "eliminateHoles") → one weakly-simple ring fillable as a frame.
## The outer is oriented CCW and each hole CW (opposite winding = a hole); each hole is spliced into the ring at a
## bridge vertex found by a +x ray from the hole's rightmost vertex. Holes are processed by descending max-x (the
## earcut order). Pure deterministic geometry (no RNG/time; inv #3). Triangulating the result fills outer MINUS holes.
func _eliminate_holes(outer: PackedVector2Array, holes: Array) -> PackedVector2Array:
	var ring := outer.duplicate()
	if _signed_area(ring) > 0.0:  # >0 = CW here → reverse to CCW outer
		ring.reverse()
	var order: Array = []
	for hi in holes.size():
		var mx := -INF
		for v in (holes[hi] as PackedVector2Array):
			mx = maxf(mx, v.x)
		order.append([mx, hi])
	order.sort_custom(func(a, b): return a[0] > b[0])
	for o in order:
		var h: PackedVector2Array = (holes[o[1]] as PackedVector2Array).duplicate()
		if _signed_area(h) < 0.0:  # <0 = CCW here → reverse to CW hole
			h.reverse()
		var hvi := 0
		var hvx := -INF
		for i in h.size():
			if h[i].x > hvx:
				hvx = h[i].x
				hvi = i
		var bi := _find_bridge_index(ring, h[hvi])
		var out := PackedVector2Array()
		for i in range(0, bi + 1):
			out.append(ring[i])
		for n in range(h.size() + 1):
			out.append(h[(hvi + n) % h.size()])
		for i in range(bi, ring.size()):
			out.append(ring[i])
		ring = out
	return ring


## Find the ring vertex to bridge a hole vertex `hv` to: cast a +x ray from `hv`, pick the ring vertex on the
## straddling edge with the smallest x >= hv.x; fall back to the nearest vertex by x. Deterministic.
func _find_bridge_index(ring: PackedVector2Array, hv: Vector2) -> int:
	var best := -1
	var bestx := INF
	var n := ring.size()
	for i in n:
		var a := ring[i]
		var b := ring[(i + 1) % n]
		if (a.y <= hv.y and b.y >= hv.y) or (b.y <= hv.y and a.y >= hv.y):
			var t := 0.0 if is_equal_approx(b.y, a.y) else (hv.y - a.y) / (b.y - a.y)
			var ix := a.x + t * (b.x - a.x)
			if ix >= hv.x - 0.001 and ix < bestx:
				bestx = ix
				best = i if a.x > b.x else (i + 1) % n
	if best == -1:
		for i in n:
			if ring[i].x < bestx:
				bestx = ring[i].x
				best = i
	return best
