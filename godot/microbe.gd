extends Node2D
## Parametric E. coli "microbe" specimen glyph, drawn with vector art (the rod-shaped cell, flagella, granules,
## acetate-overflow halo). The sibling of lsystem.gd for the MICROBE species: same Node2D + build(Dictionary) +
## bounds()->Rect2 contract, so main.gd's specimen row/label/focus/framing machinery reuses it verbatim.
##
## INVARIANT #2 (STOP THE LINE if violated): this is PURE PRESENTATION. It maps NUMERIC, already-expressed
## phenotype scalars (the 5 microbe traits from LiveSim.observe().phenotype — growth_rate, glucose_uptake,
## respiration_mode, acetate_overflow, fermentation_capacity, each in [0,1]) to pixels. It computes NO
## genotype->phenotype biology; the genome->trait map lives in the Rust core. Mapping trait values to these
## visual params (done by the caller, main.gd::_microbe_params_from_traits) is presentation, not biology —
## the microbe analogue of the L-system's _plant_params_from_traits role.
##
## All geometry (capsule outline, flagella polylines, granule/halo positions) is precomputed in build(); _draw()
## only iterates and emits draw_* calls. This matters because the headless gate (--check) runs build() but never
## _draw() — so a malformed polygon must surface at build time, not only under a GPU. Jitter is a deterministic
## hash of the specimen index (no global RNG — inv #3 hygiene), so the view is stable + hash-irrelevant.

var _body_poly := PackedVector2Array()  # the capsule (rod) outline, filled with _body_color
var _body_color := Color(0.55, 0.78, 0.85)
var _outline_color := Color(0.85, 0.95, 0.98, 0.9)
var _septum: Array = []  # [{a:Vector2, b:Vector2, w:float}] the graded fission septum pinch line(s)
var _flagella: Array = []  # [{points:PackedVector2Array, color:Color, width:float}]
var _granules: Array = []  # [{pos:Vector2, r:float, color:Color}]  internal fermentation granules
var _halo: Array = []  # [{pos:Vector2, r:float, color:Color}]  excreted acetate-overflow dots around the cell
# Respiration cytoplasm texture (its OWN channel, independent of acetate tint): aerobic → crisp O2 dots,
# fermentative → striped cytoplasm. Precomputed at build() so --check catches a malformed shape, not only the GPU.
var _o2_dots: Array = []  # [{pos:Vector2, r:float}]  faint O2 dots (aerobic, crisp membrane)
var _stripes: Array = []  # [{a:Vector2, b:Vector2}]  cytoplasm stripes (fermentative)
var _cyto_color := Color(0.0, 0.0, 0.0, 0.0)  # stripe/O2 tint, derived from respiration
var _bounds := Rect2()


## Build the cell geometry from visual params, then request a redraw. Keys (all optional, with defaults):
##   length:float  width:float  septum:bool  flagella_count:int  flagella_len:float  granule_count:int
##   halo_count:int  seed:int  body_color:Color  outline_color:Color  granule_color:Color  halo_color:Color
func build(p: Dictionary) -> void:
	var length: float = maxf(20.0, float(p.get("length", 90.0)))
	var width: float = maxf(10.0, float(p.get("width", 34.0)))
	# Graded fission-septum pinch in [0,1] (deeper waist = more divided). Back-compat: a legacy bool `septum`
	# maps to a full pinch so an older caller still shows a dividing cell.
	var septum_pinch: float = clampf(float(p.get("septum_pinch", 1.0 if bool(p.get("septum", false)) else 0.0)), 0.0, 1.0)
	# Respiration in [0,1]: 0 aerobic (crisp membrane + O2 dots) … 1 fermentative (striped cytoplasm). Its OWN
	# channel — independent of the acetate-overflow tint already folded into body_color by the caller.
	var respiration: float = clampf(float(p.get("respiration", 0.5)), 0.0, 1.0)
	var flagella_count: int = clampi(int(p.get("flagella_count", 3)), 0, 8)
	var flagella_len: float = maxf(0.0, float(p.get("flagella_len", 60.0)))
	var granule_count: int = clampi(int(p.get("granule_count", 0)), 0, 14)
	var halo_count: int = clampi(int(p.get("halo_count", 0)), 0, 16)
	var seed_val: int = int(p.get("seed", 1))
	_body_color = p.get("body_color", Color(0.55, 0.78, 0.85))
	_outline_color = p.get("outline_color", Color(0.85, 0.95, 0.98, 0.9))
	var granule_color: Color = p.get("granule_color", Color(0.96, 0.86, 0.45, 0.85))
	var halo_color: Color = p.get("halo_color", Color(0.92, 0.62, 0.32, 0.6))

	_body_poly = PackedVector2Array()
	_septum = []
	_flagella = []
	_granules = []
	_halo = []
	_o2_dots = []
	_stripes = []

	# The cell is drawn HORIZONTALLY (long axis = x), centred on the origin, pointing up like the plant is not
	# needed — the specimen view frames by bounds(). half = half-length along x; r = the capsule radius (width/2).
	var half := length * 0.5
	var r := width * 0.5
	var straight := maxf(0.0, half - r)  # length of the straight body section (capsule = rect + two semicircle caps)

	# ── Capsule (rod) outline: top edge L→R, right cap, bottom edge R→L, left cap. A stadium shape.
	var seg := 10  # arc resolution per cap
	# Top straight edge.
	_body_poly.append(Vector2(-straight, -r))
	_body_poly.append(Vector2(straight, -r))
	# Right semicircle cap (from -90° to +90°, centre at +straight).
	for k in range(1, seg):
		var a := -PI / 2.0 + PI * float(k) / float(seg)
		_body_poly.append(Vector2(straight, 0.0) + Vector2(cos(a), sin(a)) * r)
	# Bottom straight edge.
	_body_poly.append(Vector2(straight, r))
	_body_poly.append(Vector2(-straight, r))
	# Left semicircle cap (from +90° to +270°, centre at -straight).
	for k in range(1, seg):
		var a := PI / 2.0 + PI * float(k) / float(seg)
		_body_poly.append(Vector2(-straight, 0.0) + Vector2(cos(a), sin(a)) * r)

	# ── Septum (binary-fission constriction): a GRADED vertical pinch across the centre, read as a dividing cell.
	# Depth = septum_pinch: shallow pinch at the low end → a near-full waist line at the high end (so growth reads
	# across the whole range, not just a binary toggle). Thickness + inset both scale with the pinch.
	if septum_pinch > 0.02:
		var inset := r * (1.0 - 0.85 * septum_pinch)  # the line shrinks toward the centre as the pinch deepens
		_septum.append({
			"a": Vector2(0.0, -inset),
			"b": Vector2(0.0, inset),
			"w": maxf(1.5, r * (0.04 + 0.08 * septum_pinch)),
		})

	# ── Respiration cytoplasm texture (its OWN channel): aerobic (low) → faint O2 dots + a crisp membrane;
	# fermentative (high) → striped cytoplasm. Blend across the range so respiration reads independently of the
	# acetate tint. All positions precomputed here (so --check surfaces a bad shape, not only the GPU).
	_cyto_color = Color(0.55, 0.85, 0.95, 0.35).lerp(Color(0.80, 0.55, 0.25, 0.4), respiration)
	if respiration < 0.6:
		# Aerobic: a scatter of faint O2 dots inside the body (more dots the more aerobic).
		var o2 := int(round((1.0 - respiration) * 7.0))
		for oi in o2:
			var ox := (_hash01(seed_val, oi, 91) - 0.5) * 2.0 * straight * 0.9
			var oy := (_hash01(seed_val, oi, 97) - 0.5) * 2.0 * r * 0.6
			_o2_dots.append({"pos": Vector2(ox, oy), "r": maxf(1.0, r * 0.07)})
	if respiration > 0.4:
		# Fermentative: vertical cytoplasm stripes across the straight body (more stripes the more fermentative).
		var n := 1 + int(round(respiration * 4.0))
		for si in n:
			var sx := (float(si + 1) / float(n + 1) - 0.5) * 2.0 * straight * 0.85
			var sh := r * (0.55 + 0.3 * _hash01(seed_val, si, 103))
			_stripes.append({"a": Vector2(sx, -sh), "b": Vector2(sx, sh)})

	# ── Flagella: wavy polylines streaming off the LEFT pole (trailing), count/length from glucose_uptake/motility.
	for fi in flagella_count:
		var pts := PackedVector2Array()
		# Fan the flagella across the left cap by a small angular spread (deterministic per index, no RNG).
		var spread := (float(fi) / maxf(1.0, float(flagella_count - 1)) - 0.5) * 0.9  # -0.45..0.45 rad
		var root := Vector2(-half, 0.0) + Vector2(-r * 0.1, sin(spread) * r * 0.8)
		var steps := 9
		for k in steps + 1:
			var u := float(k) / float(steps)
			var x := root.x - u * flagella_len
			# A sine wave whose amplitude grows toward the tip; phase jittered per flagellum (stable hash).
			var phase := _hash01(seed_val, fi, k) * TAU
			var amp := 3.5 + u * (r * 0.55)
			var y := root.y + sin(u * PI * 2.4 + phase) * amp + spread * u * flagella_len * 0.25
			pts.append(Vector2(x, y))
		_flagella.append({
			"points": pts,
			"color": Color(_body_color.r, _body_color.g, _body_color.b, 0.7),
			"width": maxf(1.0, r * 0.08),
		})

	# ── Internal granules (fermentation_capacity): small dots scattered inside the body, density-driven.
	for gi in granule_count:
		var gx := (_hash01(seed_val, gi, 17) - 0.5) * 2.0 * straight * 0.92
		var gy := (_hash01(seed_val, gi, 31) - 0.5) * 2.0 * r * 0.62
		_granules.append({
			"pos": Vector2(gx, gy),
			"r": maxf(1.5, r * 0.14),
			"color": granule_color,
		})

	# ── Acetate-overflow halo (acetate_overflow): excreted dots in a ring just outside the membrane.
	for hi in halo_count:
		var ang := TAU * float(hi) / maxf(1.0, float(halo_count)) + _hash01(seed_val, hi, 53)
		var rad := r + 8.0 + _hash01(seed_val, hi, 71) * 14.0
		_halo.append({
			"pos": Vector2(cos(ang) * (straight + rad), sin(ang) * rad),
			"r": maxf(1.5, r * 0.10),
			"color": halo_color,
		})

	_recompute_bounds()
	queue_redraw()


## Local-space bounding box of the cell BODY only (the capsule), so the specimen row spacing + auto-fit stay
## stable regardless of flagella/halo extent (parallels lsystem.gd::bounds returning the branch-only box).
func bounds() -> Rect2:
	return _bounds


func _recompute_bounds() -> void:
	if _body_poly.is_empty():
		_bounds = Rect2()
		return
	var mn := Vector2.INF
	var mx := -Vector2.INF
	for v in _body_poly:
		mn = mn.min(v)
		mx = mx.max(v)
	_bounds = Rect2(mn, mx - mn)


# ──────────────────────────── drawing (no geometry decisions here) ────────────────────────────

func _draw() -> void:
	# Flagella behind the body.
	for fl in _flagella:
		var pts: PackedVector2Array = fl["points"]
		if pts.size() >= 2:
			draw_polyline(pts, fl["color"], fl["width"], true)
	# Acetate halo dots (behind the membrane outline so they read as excreted/around).
	for h in _halo:
		draw_circle(h["pos"], h["r"], h["color"])
	# Body fill + membrane outline.
	if not _body_poly.is_empty():
		draw_colored_polygon(_body_poly, _body_color)
		# Closed outline: re-emit the first point so the membrane reads as a continuous ring. Aerobic cells get a
		# crisper (slightly thicker, brighter) membrane via _cyto_color's alpha being lower at the aerobic end.
		var ring := PackedVector2Array(_body_poly)
		ring.append(_body_poly[0])
		draw_polyline(ring, _outline_color, maxf(1.5, _bounds.size.y * 0.04), true)
	# Respiration cytoplasm texture: fermentative stripes (under the granules), then aerobic O2 dots.
	for st in _stripes:
		draw_line(st["a"], st["b"], _cyto_color, maxf(1.0, _bounds.size.y * 0.03), true)
	for o in _o2_dots:
		draw_circle(o["pos"], o["r"], _cyto_color)
	# Internal granules.
	for g in _granules:
		draw_circle(g["pos"], g["r"], g["color"])
	# Septum (graded dividing-cell pinch).
	for s in _septum:
		draw_line(s["a"], s["b"], _outline_color, float(s.get("w", maxf(1.5, _bounds.size.y * 0.05))), true)


## Deterministic [0,1) hash for per-glyph jitter (no global RNG — inv #3 hygiene). Mirrors lsystem.gd::_hash01.
func _hash01(a: int, b: int, c: int) -> float:
	var h := (a * 73856093) ^ ((b + 1) * 19349663) ^ ((c + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
