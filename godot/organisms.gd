extends Node2D
## Read-only organism layer for the ecosystem view (S4.3 dots → P8 trait-driven plant sprites).
##
## Per non-empty snapshot cell it draws a small cluster of procedural PLANTS whose look reflects the per-cell
## channels the Rust core already derived: canopy hue from allele_freq, height/vigor/wilt from fitness, count
## from density, ground tint from soil moisture/nutrients. Each plant gets a small "activity pip" dot at its
## foot (the old dot marker, demoted). A renderer-only toggle (set_sprites_on / key 'S') falls back to plain
## full-size dots.
##
## INVARIANT #2 (STOP THE LINE if violated): PURE PRESENTATION. It reads derived per-cell aggregates the core
## computed and NEVER computes genotype→phenotype biology. The intra-cell scatter + plant morph are
## deterministic hash jitter for visual flavour only (NOT a spatial model — the core owns all placement).
## INVARIANT #3: every per-organism variation comes from _hash01(x,y,k) — never randf()/time — so a snapshot
## renders byte-identically each time; redraw fires only on set_snapshot/set_iso, never per frame.

const MAX_DOTS_PER_CELL := 5
const DOT_RADIUS_SCALE := 0.55  # demoted "activity pip" dots under each plant render at this × the old radius
const LOD_MIN_CELL := 5.0  # below this on-screen cell size, draw dots only (sprites would be sub-pixel clutter)

var _w: int = 0
var _h: int = 0
var _cell: float = 12.0
var _density: PackedFloat32Array
var _allele: PackedFloat32Array
var _fitness: PackedFloat32Array
var _moisture: PackedFloat32Array
var _nutrients: PackedFloat32Array
var _iso = null  # iso.gd instance; when set, markers are placed in isometric screen space + depth-sorted
var _sprites_on: bool = true  # renderer-only: plants vs plain dots (no biology — pure presentation state)


## Route placement through an isometric transform (P3). null = orthographic (the default).
func set_iso(iso) -> void:
	_iso = iso
	queue_redraw()


## Toggle trait-driven plant sprites vs plain dots (renderer-only presentation state). Returns the new state.
func set_sprites_on(on: bool) -> bool:
	_sprites_on = on
	queue_redraw()
	return _sprites_on


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
	queue_redraw()


func _draw() -> void:
	if _w == 0 or _h == 0:
		return
	var base_r := maxf(1.5, _cell * 0.16)
	var rim := Color(0.03, 0.05, 0.04, 0.92)
	var lod_dots_only := _cell < LOD_MIN_CELL

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
		var col := _organism_color(allele, fit)
		var morph_id := int(_hash01(x, y, 99) * 3.0)

		# Anchor: iso lifts onto the tile's terrain relief; ortho roots near the cell's lower edge. Both grow
		# toward screen-up (Vector2(0,-1)) as billboards, so the SAME plant math works in either mode.
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
				_draw_plant(p, col, fit, dens, morph_id, x, y, k)
				_draw_dot(p, col, rim, base_r * DOT_RADIUS_SCALE)  # demoted activity pip at the foot
			else:
				_draw_dot(p, col, rim, base_r)


## One procedural plant standing at foot `p`, coloured by genetics `col`, shaped by fitness/density (vigor) and
## a deterministic per-organism hash. Three morphs (forb / grass-tuft / shrub) for visual variety. Presentation
## only — no biology; all randomness is _hash01(x,y,k).
func _draw_plant(p: Vector2, col: Color, fit: float, dens: float, morph_id: int, x: int, y: int, k: int) -> void:
	var hh := _hash01(x, y, k * 7)
	var up := Vector2(0.0, -1.0)
	var nutrient := _soil_at(_nutrients, x, y)
	var moist := _soil_at(_moisture, x, y)
	var stem_h := minf(_cell * (0.35 + 0.85 * fit) * (0.85 + 0.3 * hh), _cell * 1.25)  # cap keeps depth order
	var stem_w := maxf(1.0, _cell * 0.05 * (0.6 + 0.8 * fit))
	var sway := (hh - 0.5) * _cell * 0.18 * (0.4 + (1.0 - fit))  # low fitness wilts/leans more
	var leaf := _cell * (0.10 + 0.18 * fit) * (1.0 + 0.25 * nutrient)  # rich soil → fuller canopy

	# Ground-contact shadow, tinted darker/bluer where the soil is wet (lush) vs pale where dry.
	var shadow := Color(0.0, 0.0, 0.0, 0.18).lerp(Color(0.02, 0.05, 0.12, 0.26), moist)
	draw_circle(p + Vector2(0.0, 1.0), stem_w * 1.6 + stem_h * 0.05, shadow)

	var tip := p + up * stem_h + Vector2(sway, 0.0)
	var stem_col := Color(0.30, 0.42, 0.18).lerp(Color(0.58, 0.60, 0.20), 1.0 - fit)  # green↔yellow = vigor
	draw_polyline(PackedVector2Array([p, p.lerp(tip, 0.5) + Vector2(sway * 0.5, 0.0), tip]), stem_col, stem_w, true)

	# Health glow behind the canopy — vigorous (high-fitness) cells get a soft halo.
	draw_circle(tip, leaf * 1.9, Color(col.r, col.g, col.b, 0.10 + 0.22 * fit))

	match morph_id:
		0:  # forb — a pair of leaves at mid-stem + a small flower head
			for j in 2:
				var side := float(j * 2 - 1)
				var lp := p.lerp(tip, 0.6) + Vector2(side * leaf * 0.9, 0.0)
				draw_colored_polygon(PackedVector2Array([
					lp, lp + Vector2(side * leaf, -leaf * 0.5),
					lp + Vector2(side * leaf * 1.6, 0.0), lp + Vector2(side * leaf, leaf * 0.5)]), col)
			draw_circle(tip, leaf * 0.7, col)
		1:  # grass tuft — a fan of blades from the foot, count scaled by density
			var blades := 2 + int(dens * 3.0)
			for j in blades:
				var f := (float(j) / float(maxi(1, blades - 1))) - 0.5
				var bh := stem_h * (0.7 + 0.3 * _hash01(x, y, k * 5 + j))
				draw_line(p, p + Vector2(f * _cell * 0.5, -bh), col, maxf(1.0, stem_w * 0.7))
		_:  # shrub — a round canopy body + a lighter highlight
			draw_circle(tip, leaf, col)
			draw_circle(tip - Vector2(leaf, leaf) * 0.3, leaf * 0.45, col.lightened(0.2))


## The old dot marker (rim + body + specular), reused at full size when sprites are off, or shrunk to an
## activity pip at each plant's foot when sprites are on.
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


## allele_freq → hue (cyan→blue→magenta→red, off the grass green), fitness → brightness/saturation.
## Presentation mapping only (no biology).
func _organism_color(allele: float, fitness: float) -> Color:
	var hue := 0.52 - 0.52 * clampf(allele, 0.0, 1.0)  # 0.52 cyan … 0.0 red
	var sat := 0.6 + 0.3 * fitness
	var val := 0.6 + 0.4 * fitness
	return Color.from_hsv(hue, sat, val, 0.97)


## Deterministic [0,1) hash for intra-cell jitter + per-organism variation (visual only).
func _hash01(x: int, y: int, k: int) -> float:
	var h := (x * 73856093) ^ (y * 19349663) ^ ((k + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
