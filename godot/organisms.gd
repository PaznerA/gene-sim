extends Node2D
## Read-only organism layer for the ecosystem view (S4.3 dots → P8 trait-driven sprites → multi-species lawn).
##
## Per non-empty snapshot cell it draws a small cluster of procedural ORGANISMS whose look reflects TWO layers:
##  • the SPECIES layer (constant across the field, from `_glyph_params`): the run's per-species phenotype
##    (branchiness → branch count + morph bias, stature → height, leaf size/hue/reflectance → canopy, growth →
##    thickness, drought → leaf aspect, fecundity → flowers; OR, for E. coli, rod length/width/tint/flagella/
##    granules from the 5 microbe traits). This sets the sprite TEMPLATE — the species' characteristic silhouette.
##  • the CELL layer (existing per-cell channels the Rust core derived): density → cluster count, fitness → vigor
##    (height/brightness/wilt/glow), allele_freq → a ±0.12 hue SHIFT around the species base hue (selection cline,
##    legible WITHIN the species palette), soil moisture/nutrients → shadow tint + canopy/granule fullness.
## A renderer-only toggle (set_sprites_on / key 'S') falls back to plain full-size dots.
##
## INVARIANT #2 (STOP THE LINE if violated): PURE PRESENTATION. It reads already-expressed scalars in [0,1] the
## core supplied (per-cell snapshot channels + per-species observe_species() phenotype) and NEVER computes
## genotype→phenotype biology. The intra-cell scatter + morph are deterministic hash jitter for visual flavour
## only (NOT a spatial model — the core owns all placement/expression).
## INVARIANT #3: every per-organism variation comes from _hash01(x,y,k) — never randf()/time — so a snapshot
## renders byte-identically; redraw fires only on set_snapshot/set_iso/set_species_traits, never per frame.

const MAX_DOTS_PER_CELL := 5
const DOT_RADIUS_SCALE := 0.55  # demoted "activity pip" dots under each plant render at this × the old radius
const LOD_MIN_CELL := 5.0  # below this on-screen cell size, draw dots only (sprites would be sub-pixel clutter)
const ALLELE_HUE_SHIFT := 0.12  # allele_freq shifts the hue ±this AROUND the species base hue (within-species cline)
const SpeciesVisualMap := preload("res://species_visual_map.gd")

var _w: int = 0
var _h: int = 0
var _cell: float = 12.0
var _density: PackedFloat32Array
var _allele: PackedFloat32Array
var _fitness: PackedFloat32Array
var _moisture: PackedFloat32Array
var _nutrients: PackedFloat32Array
# GSS5: per-cell dominant SpeciesId ordinal (row-major, w*h) the core exported. Drives the per-cell SIZE + base
# COLOR via _species_table so each cell's organisms render at their dominant species' real-cell scale (plant
# LARGE, microbes small, symbionts tiny) instead of one shared density-derived radius. Empty → all-default.
var _dominant: PackedFloat32Array
# species_id:int -> {size:float, color:Color, is_plant:bool}; built in main.gd via SpeciesVisualMap.build_table
# from observe_species(). Empty (file-replay / pre-feed) → every cell uses the default plant visual (graceful).
var _species_table: Dictionary = {}
var _iso = null  # iso.gd instance; when set, markers are placed in isometric screen space + depth-sorted
var _sprites_on: bool = true  # renderer-only: plants vs plain dots (no biology — pure presentation state)

# Species layer (set once per snapshot via set_species_traits, from the core's observe_species()/specimens.json).
var _species_key: String = "default"  # "ecoli-core" → microbe lawn; else plant meadow
var _traits: Dictionary = {}  # the focused/primary species' snake_cased phenotype (already expressed by the core)
var _glyph_params: Dictionary = {}  # species-level visual params precomputed ONCE (never per cell)


## Route placement through an isometric transform (P3). null = orthographic (the default).
func set_iso(iso) -> void:
	_iso = iso
	queue_redraw()


## Toggle trait-driven sprites vs plain dots (renderer-only presentation state). Returns the new state.
func set_sprites_on(on: bool) -> bool:
	_sprites_on = on
	queue_redraw()
	return _sprites_on


## Set the run-level species visual template from the core's already-expressed phenotype + species key.
## `traits` is the snake_cased phenotype (the SAME [0,1] scalars the specimen view reads — main.gd translates
## observe_species()'s Debug-cased keys); `key` routes plant vs microbe. The species-level glyph params are
## computed HERE, once per snapshot (not per cell), so the per-cell draw loop only multiplies/lerps. Pure
## presentation (inv #2): no biology — the genome→trait expression already ran in the Rust core. queue_redraw
## only when something actually changed so a static field doesn't thrash.
func set_species_traits(traits: Dictionary, key: String) -> void:
	if traits == _traits and key == _species_key and not _glyph_params.is_empty():
		return
	_traits = traits.duplicate()
	_species_key = key if key != "" else "default"
	_glyph_params = _compute_glyph_params(_traits, _species_key)
	queue_redraw()


## Precompute the species-level visual template ONCE (per snapshot). PLANT → canopy palette / morph bias /
## stroke counts; MICROBE → rod dims / tint / flagella. Everything is trait→pixels arithmetic (inv #2). The
## per-cell draw reads these constants and modulates by the per-cell channels — so the species silhouette is
## constant across the field, varied cell-to-cell by fitness/allele/density.
func _compute_glyph_params(t: Dictionary, key: String) -> Dictionary:
	if key == "ecoli-core":
		var growth := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
		var glucose := clampf(float(t.get("glucose_uptake", 0.5)), 0.0, 1.0)
		var respiration := clampf(float(t.get("respiration_mode", 0.5)), 0.0, 1.0)  # 0 aerobic … 1 fermentative
		var acetate := clampf(float(t.get("acetate_overflow", 0.0)), 0.0, 1.0)
		var ferment := clampf(float(t.get("fermentation_capacity", 0.0)), 0.0, 1.0)
		# Microbe base hue: cool blue-green (aerobic) → amber (fermentative) → red push (acetate overflow). This
		# arc is DISJOINT from the plant green/yellow-green arc so the two species never share colour space.
		var base_hue := lerpf(0.46, 0.10, respiration)  # 0.46 cyan-green … 0.10 amber
		base_hue = lerpf(base_hue, 0.02, acetate * 0.5)  # acetate overflow pulls toward red
		return {
			"is_microbe": true,
			"base_hue": base_hue,
			"rod_len": 0.6 + 0.8 * growth,  # growth → longer rod (× cell)
			"rod_width": 0.34 + 0.30 * glucose,  # glucose → fatter rod (× cell)
			"flagella": clampi(int(round(glucose * 2.0)), 0, 2),  # 0..2 whiskers
			"septum": growth,  # high growth → a fission septum tick
			"granule": ferment,  # fermentation → internal granule speck
			"acetate": acetate,  # acetate → excreted halo speck
			"respiration": respiration,
		}
	# PLANT template.
	var growth_p := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
	var stature := clampf(float(t.get("stature", 0.5)), 0.0, 1.0)
	var branchy := clampf(float(t.get("branchiness", 0.5)), 0.0, 1.0)
	var leaf := clampf(float(t.get("leaf_size", 0.5)), 0.0, 1.0)
	var hue := clampf(float(t.get("leaf_hue", 0.5)), 0.0, 1.0)
	var refl := clampf(float(t.get("reflectance", 0.5)), 0.0, 1.0)
	var fec := clampf(float(t.get("fecundity", 0.5)), 0.0, 1.0)
	var drought := clampf(float(t.get("drought_tolerance", 0.5)), 0.0, 1.0)
	var ksl := clampf(float(t.get("kill_switch_linkage", 0.0)), 0.0, 1.0)
	# Canopy base hue: yellow-green → deep green → blue-green (anchored by leaf_hue); allele_freq later shifts
	# ±0.12 AROUND this (the within-species selection cline). The plant arc sits off the grass-green backdrop.
	return {
		"is_microbe": false,
		"base_hue": 0.18 + hue * 0.30,
		"canopy_sat": 0.55 + drought * 0.25,
		"canopy_val": 0.55 + refl * 0.35,  # reflectance brightens the canopy
		# branchiness → morph BIAS (replaces the old random morph_id): low→grass-tuft(1), mid→forb(0), high→shrub(2)
		"morph": 1 if branchy < 0.34 else (0 if branchy < 0.67 else 2),
		"extra_branches": clampi(int(round(branchy * 2.0)), 0, 2),  # 0..2 extra branch strokes
		"clumps": 1 + int(round(branchy * 2.0)),  # canopy clump count
		"stem_h_mul": 0.7 + 0.6 * stature,  # stature → taller (capped in _draw so depth order holds)
		"stem_w_mul": 0.6 + 0.8 * growth_p,  # growth → thicker stem
		"leaf_mul": 0.6 + 0.8 * leaf,  # leaf_size → leaf radius
		"leaf_aspect": 0.42 + drought * 0.30,  # drought → narrower, sturdier leaves
		"sway_gain": 0.4 + ksl * 1.1,  # kill-switch linkage → per-stroke sway gain (instability cue)
		"fecundity": fec,  # → flower speck on high-fitness+high-density cells
	}


## Set the per-cell dominant SpeciesId plane (GSS5, from snap.dominant_species_id) + the species-id → visual
## lookup (built in main.gd from observe_species() via SpeciesVisualMap.build_table). Together they let the
## per-cell draw size + colour each cell's organisms by its DOMINANT species' real-cell scale, instead of one
## shared density radius. Pure presentation (inv #2): the core decided which species dominates each cell; this
## maps that id → pixels. Either argument may be empty (file-replay / older cdylib) → the draw falls back to the
## default plant visual gracefully. queue_redraw so a changed map repaints.
func set_dominant_species_ids(dominant: PackedFloat32Array, species_table: Dictionary) -> void:
	_dominant = dominant
	_species_table = species_table
	queue_redraw()


## The {size, color, is_plant} visual for the dominant species at cell index `i`. Reads _dominant (the GSS5
## plane) → species_table. Graceful: a missing plane / unknown id → the default plant visual (never a crash).
func _cell_visual(i: int) -> Dictionary:
	if i < 0 or i >= _dominant.size():
		return {"size": 1.0, "color": Color(0.36, 0.62, 0.24), "is_plant": true}
	var sid := int(round(_dominant[i]))
	if _species_table.has(sid):
		return _species_table[sid]
	# Unknown id (empty table on file-replay, or a species absent from observe_species) → a neutral default.
	return {"size": SpeciesVisualMap.SIZE_DEFAULT, "color": SpeciesVisualMap.COLOR_DEFAULT, "is_plant": true}


## Point the layer at a parsed snapshot (snapshot.gd instance) and a cell size in pixels, then redraw.
func set_snapshot(snap, cell: float) -> void:
	_w = snap.width
	_h = snap.height
	_cell = cell
	_density = snap.density
	_allele = snap.allele_freq
	_fitness = snap.fitness
	_moisture = snap.soil_moisture  # soil channels drive ground tint / canopy fullness (presentation only)
	_nutrients = snap.soil_nutrients
	# GSS5: pull the per-cell dominant-species plane straight off the snapshot so a freshly-fed snapshot is always
	# size-aware even if set_dominant_species_ids wasn't called separately (the table may still be empty → default).
	if "dominant_species_id" in snap:
		_dominant = snap.dominant_species_id
	if _glyph_params.is_empty():
		# File-replay or pre-feed: neutral 0.5s so the field still renders as a plausible green population.
		_glyph_params = _compute_glyph_params({}, _species_key)
	queue_redraw()


func _draw() -> void:
	if _w == 0 or _h == 0:
		return
	if _density.size() != _w * _h or _allele.size() != _w * _h or _fitness.size() != _w * _h:
		return  # short/truncated channels (snapshot.gd should have rejected them) — degrade gracefully, no crash
	var base_r := maxf(1.5, _cell * 0.16)
	var rim := Color(0.03, 0.05, 0.04, 0.92)
	var lod_dots_only := _cell < LOD_MIN_CELL
	var base_hue: float = float(_glyph_params.get("base_hue", 0.4))
	var is_microbe: bool = bool(_glyph_params.get("is_microbe", false))

	# Visit non-empty cells. Under iso, depth-sort (cx+cy ascending) so nearer cells overdraw farther.
	var order: Array = []
	for y in _h:
		for x in _w:
			if _density[y * _w + x] > 0.0:
				order.append([(x + y) if _iso != null else 0, x, y])
	if _iso != null:
		order.sort_custom(func(a, b): return a[0] < b[0])

	for cell in order:
		var x: int = cell[1]
		var y: int = cell[2]
		var i := y * _w + x
		var dens := clampf(_density[i], 0.0, 1.0)
		var fit := clampf(_fitness[i], 0.0, 1.0)
		var allele := clampf(_allele[i], 0.0, 1.0)
		var markers := clampi(int(ceil(dens * float(MAX_DOTS_PER_CELL))), 1, MAX_DOTS_PER_CELL)
		# GSS5 per-cell species sizing/colouring: the DOMINANT species of THIS cell (core-exported id → visual)
		# sets the radius (plant LARGE … symbiont tiny) and the base colour, so adjacent species read at their
		# real relative scale instead of one shared density radius. Empty plane/table → the default plant visual.
		var vis := _cell_visual(i)
		var cell_r := base_r * float(vis.get("size", 1.0))
		var cell_is_plant: bool = bool(vis.get("is_plant", not is_microbe))
		# Per-cell colour: blend the SPECIES base colour (from the visual table) toward the fitness-brightened
		# allele cline so selection stays legible WITHIN the species palette. Same for the LOD dot so no scope
		# shows a flat marker. _cell_base_hue derives a hue from the table colour, falling back to the run hue.
		var cell_hue := _hue_of(vis.get("color", null), base_hue)
		var col := _organism_color(allele, fit, cell_hue)

		# Anchor: iso lifts onto the tile's terrain relief; ortho roots near the cell's lower edge. Both grow
		# toward screen-up (Vector2(0,-1)) as billboards, so the SAME sprite math works in either mode.
		var base: Vector2
		if _iso != null:
			var lift: float = _cell * 0.7 * _iso.terrain_height(x, y)  # match iso_ground HEIGHT_MAX
			base = _iso.cell_to_screen(float(x), float(y), _cell) + Vector2(0.0, _cell * 0.25 - lift)
		else:
			base = Vector2(float(x) * _cell, float(y) * _cell) + Vector2(_cell * 0.5, _cell * 0.7)

		for k in markers:
			var jx := _hash01(x, y, k * 2) - 0.5
			var jy := _hash01(x, y, k * 2 + 1) - 0.5
			var p: Vector2
			if _iso != null:
				p = base + Vector2(jx * _cell * 0.7, jy * _cell * 0.35)  # 2:1 squashed footprint
			else:
				p = base + Vector2(jx * _cell * 0.7, jy * _cell * 0.5)
			if _sprites_on and not lod_dots_only:
				# Route the per-cell draw on the cell's DOMINANT species (GSS5): a plant L-sprite for autotrophs,
				# else a microbe rod-blob — sized by the species' real-cell SIZE multiplier (vis.size). Falls back
				# to the run-level is_microbe flag when no dominant plane/table is present (file-replay).
				var size_scale := float(vis.get("size", 1.0))
				if cell_is_plant:
					_draw_plant(p, col, fit, dens, x, y, k, size_scale)
				else:
					_draw_microbe_sprite(p, col, fit, dens, x, y, k, size_scale)
				_draw_dot(p, col, rim, cell_r * DOT_RADIUS_SCALE)  # demoted activity pip at the foot, species-sized
			else:
				# LOD / sprites-off: a TRAIT-TINTED dot at the species' real-cell radius (cell_r) — even the
				# zoomed-out field scope is species-distinct by SIZE + palette, not one generic uniform marker.
				_draw_dot(p, col, rim, cell_r)


## One procedural plant standing at foot `p`. The SPECIES template (`_glyph_params`: stature→height, branchiness→
## morph bias + extra strokes + clumps, leaf_size→canopy, growth→thickness, leaf_hue+reflectance→palette via
## `col`, drought→leaf aspect, fecundity→flower, ksl→sway) sets the shape; the per-CELL fitness/density/soil
## modulate vigor on top. Presentation only — no biology; all randomness is _hash01(x,y,k).
func _draw_plant(p: Vector2, col: Color, fit: float, dens: float, x: int, y: int, k: int, size_scale: float = 1.0) -> void:
	var hh := _hash01(x, y, k * 7)
	var up := Vector2(0.0, -1.0)
	var nutrient := _soil_at(_nutrients, x, y)
	var moist := _soil_at(_moisture, x, y)
	# GSS5: the per-cell effective cell metric scales the whole sprite by the dominant species' real-cell size.
	var ec := _cell * maxf(0.2, size_scale)
	var stem_h_mul: float = float(_glyph_params.get("stem_h_mul", 1.0))
	var stem_w_mul: float = float(_glyph_params.get("stem_w_mul", 1.0))
	var leaf_mul: float = float(_glyph_params.get("leaf_mul", 1.0))
	var leaf_aspect: float = float(_glyph_params.get("leaf_aspect", 0.6))
	var sway_gain: float = float(_glyph_params.get("sway_gain", 1.0))
	var morph_id: int = int(_glyph_params.get("morph", 0))
	var extra_branches: int = int(_glyph_params.get("extra_branches", 0))
	var clumps: int = int(_glyph_params.get("clumps", 1))
	var fec: float = float(_glyph_params.get("fecundity", 0.0))
	# Height = species template (stature/growth) × per-cell fitness vigor, capped so depth order holds.
	var stem_h := minf(ec * (0.35 + 0.85 * fit) * stem_h_mul * (0.85 + 0.3 * hh), ec * 1.4)
	var stem_w := maxf(1.0, ec * 0.05 * stem_w_mul)
	var sway := (hh - 0.5) * ec * 0.18 * (0.4 + (1.0 - fit)) * sway_gain  # ksl + low fitness lean more
	var leaf := ec * (0.10 + 0.18 * fit) * leaf_mul * (1.0 + 0.25 * nutrient)  # rich soil → fuller canopy

	# Ground-contact shadow, tinted darker/bluer where the soil is wet (lush) vs pale where dry.
	var shadow := Color(0.0, 0.0, 0.0, 0.18).lerp(Color(0.02, 0.05, 0.12, 0.26), moist)
	draw_circle(p + Vector2(0.0, 1.0), stem_w * 1.6 + stem_h * 0.05, shadow)

	var tip := p + up * stem_h + Vector2(sway, 0.0)
	var stem_col := Color(0.30, 0.42, 0.18).lerp(Color(0.58, 0.60, 0.20), 1.0 - fit)  # green↔yellow = vigor
	draw_polyline(PackedVector2Array([p, p.lerp(tip, 0.5) + Vector2(sway * 0.5, 0.0), tip]), stem_col, stem_w, true)

	# branchiness → extra side branch strokes off the main stem (0..2), each swaying by the same gain.
	for b in extra_branches:
		var frac := 0.45 + 0.2 * float(b)
		var anchor := p.lerp(tip, frac)
		var side := (1.0 if (b % 2 == 0) else -1.0)
		var bt := anchor + Vector2(side * leaf * 1.8, -stem_h * 0.22) + Vector2(sway * 0.4, 0.0)
		draw_line(anchor, bt, stem_col, maxf(1.0, stem_w * 0.7))

	# Health glow behind the canopy — vigorous (high-fitness) cells get a soft halo.
	draw_circle(tip, leaf * 1.9, Color(col.r, col.g, col.b, 0.10 + 0.22 * fit))

	match morph_id:
		0:  # forb — leaf pairs at mid-stem (count = clumps) + a small flower head; drought narrows the leaves
			for c in clumps:
				var cf := 0.5 + 0.12 * float(c)
				for j in 2:
					var side2 := float(j * 2 - 1)
					var lp := p.lerp(tip, cf) + Vector2(side2 * leaf * 0.9, 0.0)
					draw_colored_polygon(PackedVector2Array([
						lp, lp + Vector2(side2 * leaf, -leaf * leaf_aspect),
						lp + Vector2(side2 * leaf * 1.6, 0.0), lp + Vector2(side2 * leaf, leaf * leaf_aspect)]), col)
			draw_circle(tip, leaf * 0.7, col)
		1:  # grass tuft — a fan of blades from the foot, count scaled by density + branchiness clumps
			var blades := 2 + int(dens * 3.0) + clumps
			for j in blades:
				var f := (float(j) / float(maxi(1, blades - 1))) - 0.5
				var bh := stem_h * (0.7 + 0.3 * _hash01(x, y, k * 5 + j))
				draw_line(p, p + Vector2(f * ec * 0.5, -bh), col, maxf(1.0, stem_w * 0.7))
		_:  # shrub — a round canopy body (clump lobes) + a lighter highlight
			for c in clumps:
				var off := Vector2((_hash01(x, y, c * 11) - 0.5) * leaf, -(_hash01(x, y, c * 13)) * leaf * 0.6)
				draw_circle(tip + off, leaf * (0.7 + 0.3 * _hash01(x, y, c * 17)), col)
			draw_circle(tip - Vector2(leaf, leaf) * 0.3, leaf * 0.45, col.lightened(0.2))

	# fecundity → a flower speck, only on vigorous + dense cells (so blooms read as a thriving patch).
	if fec > 0.5 and fit > 0.6 and dens > 0.5:
		draw_circle(tip + Vector2(leaf * 0.4, -leaf * 0.3), leaf * 0.3, Color(0.97, 0.86, 0.42, 0.95))


## One procedural E. coli cell lying on the substrate at `p` — the field-scale echo of microbe.gd. A horizontal
## rod/capsule: length ← growth, width ← glucose, tint ← respiration (via `col`), 0–2 flagella ← glucose,
## granule speck ← fermentation, acetate halo speck ← acetate overflow. Per-cell fitness/density modulate vigor.
## Presentation only (inv #2) — the 5 microbe traits were expressed by the Rust core; this is trait→pixels.
func _draw_microbe_sprite(p: Vector2, col: Color, fit: float, dens: float, x: int, y: int, k: int, size_scale: float = 1.0) -> void:
	var hh := _hash01(x, y, k * 7)
	# GSS5: scale the rod by the dominant species' real-cell size (a Bdellovibrio speck is far smaller than a rod).
	var ec := _cell * maxf(0.2, size_scale)
	var rod_len: float = float(_glyph_params.get("rod_len", 1.0))
	var rod_width: float = float(_glyph_params.get("rod_width", 0.4))
	var flagella: int = int(_glyph_params.get("flagella", 0))
	var septum: float = float(_glyph_params.get("septum", 0.0))
	var granule: float = float(_glyph_params.get("granule", 0.0))
	var acetate: float = float(_glyph_params.get("acetate", 0.0))
	var moist := _soil_at(_moisture, x, y)
	# Rod dimensions: species template × per-cell fitness vigor. Lies horizontal (substrate lawn), slight tilt.
	var half_len := ec * 0.5 * rod_len * (0.55 + 0.45 * fit)
	var width := maxf(1.5, ec * 0.5 * rod_width * (0.7 + 0.3 * fit))
	var tilt := (hh - 0.5) * 0.5  # small per-cell tilt so the lawn isn't a grid of identical rods
	var axis := Vector2(cos(tilt), sin(tilt))
	var a := p - axis * half_len
	var b := p + axis * half_len

	# Ground-contact shadow under the rod (wet soil = darker).
	var shadow := Color(0.0, 0.0, 0.0, 0.16).lerp(Color(0.02, 0.05, 0.12, 0.24), moist)
	draw_circle(p + Vector2(0.0, width * 0.5), half_len * 0.8, shadow)

	# Flagella whiskers trailing one end (count ← glucose).
	var fcol := Color(col.r, col.g, col.b, 0.5)
	for f in flagella:
		var spread := (float(f) - 0.5 * float(maxi(1, flagella - 1))) * 0.5
		var wv := a - axis.rotated(spread) * (half_len * 0.8)
		draw_line(a, wv, fcol, maxf(1.0, width * 0.18))

	# Capsule body: a thick rounded line + rounded caps (read as a rod even at small cell sizes).
	var perp := axis.orthogonal()
	draw_line(a, b, col, width, true)
	draw_circle(a, width * 0.5, col)
	draw_circle(b, width * 0.5, col)
	# Membrane outline glow (vigor).
	draw_circle(p, width * 0.5 + half_len * 0.04, Color(col.r, col.g, col.b, 0.10 + 0.18 * fit))

	# High growth → a fission septum tick across the waist (dividing cell).
	if septum > 0.7:
		draw_line(p - perp * width * 0.5, p + perp * width * 0.5, Color(0.96, 0.99, 1.0, 0.7), maxf(1.0, width * 0.12))
	# Fermentation → an internal granule speck.
	if granule > 0.5:
		draw_circle(p + axis * half_len * 0.3, width * 0.22, Color(0.97, 0.88, 0.46, 0.9))
	# Acetate overflow → an excreted halo speck beside the cell.
	if acetate > 0.5 and dens > 0.4:
		draw_circle(b + perp * width * 0.6, width * 0.18, Color(0.93, 0.58, 0.30, 0.6))


## The old dot marker (rim + body + specular), reused at full size when sprites are off / at LOD, or shrunk to
## an activity pip at each organism's foot when sprites are on. Trait-tinted via `col` (set in _draw).
func _draw_dot(p: Vector2, col: Color, rim: Color, radius: float) -> void:
	draw_circle(p, radius * 1.6, Color(col.r, col.g, col.b, 0.14))  # soft halo
	draw_circle(p, radius + 1.0, rim)  # dark rim so it reads on grass
	draw_circle(p, radius, col)  # body, coloured by genetics
	draw_circle(p - Vector2(radius, radius) * 0.32, radius * 0.34, Color(1, 1, 1, 0.7))  # specular core


## Read a soil channel at (x,y), tolerating an unset (empty) array. Clamped to [0,1]. Presentation only.
func _soil_at(arr: PackedFloat32Array, x: int, y: int) -> float:
	var i := y * _w + x
	if i < 0 or i >= arr.size():
		return 0.0
	return clampf(arr[i], 0.0, 1.0)


## The HUE of a species visual-table colour (GSS5), so the per-cell allele/fitness cline shifts WITHIN that
## species' palette. `fallback` (the run-level base hue) is used when no table colour is present (file-replay /
## unknown id) — graceful. Pure presentation: a Color→hue read, no biology.
func _hue_of(color, fallback: float) -> float:
	if color is Color:
		return (color as Color).h
	return fallback


## (allele_freq, fitness, species base_hue) → Color. allele_freq SHIFTS the hue ±0.12 AROUND the species base
## hue (so the selection cline stays legible as a colour cline WITHIN the species palette, never spanning the
## whole wheel); fitness drives brightness/saturation. Presentation mapping only (no biology, inv #2).
func _organism_color(allele: float, fitness: float, base_hue: float) -> Color:
	var hue := fposmod(base_hue + (clampf(allele, 0.0, 1.0) - 0.5) * 2.0 * ALLELE_HUE_SHIFT, 1.0)
	var sat := 0.6 + 0.3 * fitness
	var val := 0.6 + 0.4 * fitness
	return Color.from_hsv(hue, sat, val, 0.97)


## Deterministic [0,1) hash for intra-cell jitter + per-organism variation (visual only).
func _hash01(x: int, y: int, k: int) -> float:
	var h := (x * 73856093) ^ (y * 19349663) ^ ((k + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
