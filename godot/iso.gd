extends RefCounted
## Isometric transform helper for the ecosystem view (P3 / R8-iso). PRESENTATION ONLY.
##
## INVARIANT #2 (STOP THE LINE if violated): this is a pure coordinate/geometry helper for the read-only
## renderer. It computes NO biology / genotype→phenotype — it maps integer grid cells (cx, cy) to screen
## pixels and back, and emits diamond corner points for CPU ground rendering. All genome → trait logic lives
## in the Rust core; this file only changes WHERE cells are drawn, never WHAT a cell means.
##
## Math is multiply/add only — no sin/cos/sqrt/pow (no transcendentals). The forward map is a standard 2:1
## "diamond" isometric projection; screen_to_cell is its EXACT algebraic inverse, so mouse picking through it
## round-trips a screen point clicked at the centre of a cell back to that cell (see the self-test below).
##
## WHY CPU DIAMONDS, NOT A NATIVE ISO TILESET:
##   Ground/soil must be rendered as CPU `draw_colored_polygon` diamonds in ONE ordered `_draw()` pass, walked
##   in depth_key() (cx+cy) order back-to-front — NOT with a Godot TileMapLayer whose TileSet uses
##   TILE_SHAPE_ISOMETRIC. Godot bug #89423 makes isometric `local_to_map()` picking unreliable, so a native
##   iso TileSet would silently break click/hover inspection. Owning the transform here keeps picking exact
##   (screen_to_cell is the algebraic inverse) and keeps draw order explicit.
##
## NOTE: deliberately NO `class_name` global (mirrors snapshot.gd): that registry is only populated by an
## editor import pass, so a bare `Iso` identifier is unresolved under a fresh `--headless` run (CI / the gate).
## Consumers `preload("res://iso.gd")` and call these methods on an instance. No `.godot/` cache needed.
##
## ── Coordinate conventions ────────────────────────────────────────────────────────────────────────────
##  cx, cy : grid cell coordinates (cx = column / world-x cell, cy = row / world-y cell), as in the
##           orthographic path where a cell is drawn at pixel (cx*cell, cy*cell).
##  cell   : the orthographic cell edge length in pixels (main.gd `_cell`). Reused as the iso scale so zoom
##           scopes keep working unchanged.
##  origin : a screen-space offset (Vector2) added to every projected point so the field can be framed away
##           from (0,0). Defaults to ZERO. screen_to_cell subtracts the SAME origin first, preserving the
##           exact inverse.
##
## Forward (standard 2:1 iso; tile is twice as wide as tall):
##   screen.x = (cx - cy) * cell * 0.5  + origin.x
##   screen.y = (cx + cy) * cell * 0.25 + origin.y
##
## Inverse (solve the 2x2 system; det = cell*cell*0.125, so the closed form below is exact, no trig):
##   let u = (p.x - origin.x) / cell      (= (cx - cy) * 0.5)
##   let v = (p.y - origin.y) / cell      (= (cx + cy) * 0.25)
##   cx = u + 2*v        ( (cx-cy)*0.5 + (cx+cy)*0.5 = cx )
##   cy = 2*v - u        ( (cx+cy)*0.5 - (cx-cy)*0.5 = cy )

var origin: Vector2 = Vector2.ZERO  # screen-space offset added to every projected point (field framing)

const HILL_SCALE := 6.0  # cells per terrain-height lattice step (gentle rolling hills)


## Deterministic terrain height in [0, 1] for cell (cx, cy) — smooth bilinear value-noise giving gentle
## rolling hills, so the iso ground reads as 3D relief, not a flat rhombus. PRESENTATION ONLY (inv #2): a
## visual backdrop height, NOT biology / data. Multiply/add only (deterministic, no trig).
func terrain_height(cx: int, cy: int) -> float:
	var fx := float(cx) / HILL_SCALE
	var fy := float(cy) / HILL_SCALE
	var x0 := int(floor(fx))
	var y0 := int(floor(fy))
	var tx := fx - float(x0)
	var ty := fy - float(y0)
	var top := lerpf(_h(x0, y0), _h(x0 + 1, y0), tx)
	var bot := lerpf(_h(x0, y0 + 1), _h(x0 + 1, y0 + 1), tx)
	return clampf(lerpf(top, bot, ty), 0.0, 1.0)


func _h(a: int, b: int) -> float:
	var h := (a * 73856093) ^ ((b + 1) * 19349663) ^ 83492791
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0


## cell (cx, cy) → screen pixel of the cell's CENTRE-TOP anchor in the iso projection. Standard 2:1 diamond.
## Multiply/add only. cx/cy are floats so callers can project fractional positions (e.g. organism jitter).
func cell_to_screen(cx: float, cy: float, cell: float) -> Vector2:
	return Vector2(
		(cx - cy) * cell * 0.5 + origin.x,
		(cx + cy) * cell * 0.25 + origin.y)


## screen pixel → fractional cell (cx, cy). EXACT inverse of cell_to_screen (same `origin`, same `cell`), so
## mouse picking via floor(screen_to_cell(...)) lands on the cell whose diamond contains the point. No trig.
func screen_to_cell(p: Vector2, cell: float) -> Vector2:
	var u := (p.x - origin.x) / cell  # = (cx - cy) * 0.5
	var v := (p.y - origin.y) / cell  # = (cx + cy) * 0.25
	return Vector2(u + 2.0 * v, 2.0 * v - u)


## Back-to-front painter's-order key for one ordered `_draw()` pass: smaller cx+cy is farther "up/back" and
## must be drawn first. Caller sorts cells by this ascending, then draws ground diamonds (and per-cell sprites)
## in that order so nearer tiles overdraw farther ones.
func depth_key(cx: int, cy: int) -> int:
	return cx + cy


## The 4 corners of the iso tile diamond for cell (cx, cy), as a PackedVector2Array ready for
## `draw_colored_polygon`. Order is top, right, bottom, left (clockwise), forming the rhombus whose centre row
## sits a half-cell-height band around the cell anchor. cx/cy are floats for fractional placement.
##   top    = (cx,   cy-1) anchor → one tile-height above
##   ...but computed directly from the anchor so it stays self-contained and trig-free.
func diamond_points(cx: float, cy: float, cell: float) -> PackedVector2Array:
	var c := cell_to_screen(cx, cy, cell)  # cell anchor (top-centre of the diamond)
	var hw := cell * 0.5   # half diamond width
	var hh := cell * 0.25  # half diamond height (2:1 ratio → height is half the width)
	# top, right, bottom, left around the diamond centre, which is half a tile-height below the anchor.
	var cy_mid := c.y + hh
	return PackedVector2Array([
		Vector2(c.x, c.y),            # top
		Vector2(c.x + hw, cy_mid),    # right
		Vector2(c.x, cy_mid + hh),    # bottom
		Vector2(c.x - hw, cy_mid),    # left
	])


## Screen-space bounding box (Rect2) of the whole w×h field under the current `origin`. Accounts for the fact
## that the iso projection makes cx-cy go NEGATIVE on the left edge: the min-x corner is the bottom-left grid
## cell (0, h-1) and the max-x corner is the top-right grid cell (w-1, 0). Walking the four grid corners is
## enough because the projection is affine (linear), so extrema occur at grid corners. Lets the camera frame
## the diamond field exactly (replaces the orthographic `_field_px` rectangle).
func field_bounds(w: int, h: int, cell: float) -> Rect2:
	if w <= 0 or h <= 0:
		return Rect2(origin, Vector2.ZERO)
	# Project the 4 grid corners; also include the bottom diamond tips so the box covers the full tiles.
	var lo := Vector2(INF, INF)
	var hi := Vector2(-INF, -INF)
	var corners := [
		Vector2(0, 0), Vector2(w, 0), Vector2(0, h), Vector2(w, h),
	]
	for cxy in corners:
		var p: Vector2 = cell_to_screen(cxy.x, cxy.y, cell)
		lo.x = minf(lo.x, p.x)
		lo.y = minf(lo.y, p.y)
		hi.x = maxf(hi.x, p.x)
		hi.y = maxf(hi.y, p.y)
	# cell_to_screen returns each cell's TOP anchor; the diamond extends half a tile-height below the (w,h)
	# anchor, so pad the bottom by cell*0.5 to enclose the lowest tiles' bottom tips. Pad the TOP by the max
	# terrain lift (HEIGHT_MAX≈0.7) so raised hills are not clipped, and the bottom by the block depth.
	lo.y -= cell * 0.7
	hi.y += cell * (0.5 + 0.22)
	return Rect2(lo, hi - lo)


## ── Self-test (documentation; mirrored by the headless probe that ran in the slice) ──────────────────────
## Round-trip: a point at a cell's anchor maps back to that exact cell; a point nudged toward the diamond
## centre floors to the same cell. With origin = ZERO, cell = 16:
##   iso.cell_to_screen(3, 5, 16)            -> ((3-5)*8, (3+5)*4) = Vector2(-16, 32)
##   iso.screen_to_cell(Vector2(-16, 32), 16):
##       u = -16/16 = -1.0   v = 32/16 = 2.0
##       cx = u + 2v = -1 + 4 = 3            cy = 2v - u = 4 + 1 = 5     -> Vector2(3, 5)  ✓ exact inverse
##   A click 1px right + a quarter-cell down of the anchor still floors to (3, 5) because the diamond centre
##   is at anchor + (0, cell*0.25). depth_key(3,5) == 8 orders it behind cells with a larger cx+cy.
