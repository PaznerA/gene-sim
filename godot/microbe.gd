extends Node2D
## Parametric bacterial "microbe" specimen glyph, drawn with vector art. GENERALIZED (SP-4) from the original
## E. coli rod into a multi-morphotype body: rod / coccus / vibrioid / wall-less, with flagella layout, a
## biofilm-matrix halo, and an inset endospore — so rods, cocci, vibrioids, spore-formers, wall-less
## mycoplasma, and the symbiont speck all read distinctly. The sibling of lsystem.gd / mold.gd: same Node2D +
## build(Dictionary) + bounds()->Rect2 contract, so main.gd's specimen row/label/focus/framing machinery
## (_render_specimens / _frame_focused_specimen / _emphasise_focus) reuses it verbatim.
##
## INVARIANT #2 (STOP THE LINE if violated): this is PURE PRESENTATION. It maps NUMERIC, already-expressed
## phenotype scalars (the microbe traits from the core observe()) + the species role/key to pixels. It computes
## NO genotype->phenotype biology; the genome->trait map lives in the Rust core. Mapping trait values to these
## visual params (done by the caller, glyph_factory.gd) is presentation, not biology.
##
## All geometry (body outline, flagella polylines, granule/halo/endospore positions) is precomputed in build();
## _draw() only iterates and emits draw_* calls. This matters because the headless gate (--check) runs build()
## but never _draw() — so a malformed polygon must surface at build time, not only under a GPU. Jitter is a
## deterministic hash of the specimen index (no global RNG — inv #3), so the view is stable + hash-irrelevant.

var _body_polys: Array = []  # [PackedVector2Array] the body outline(s) — one per cell (cocci cluster = several)
var _body_color := Color(0.55, 0.78, 0.85)
var _outline_color := Color(0.85, 0.95, 0.98, 0.9)
var _outline_width := 3.0  # membrane ring thickness (set per shape; wall-less → 0 = no crisp ring)
var _outline_dashed := false  # wall-less → a faint, fuzzy (no crisp) edge
var _septum: Array = []  # [{a:Vector2, b:Vector2, w:float}] the graded fission septum pinch line(s)
var _flagella: Array = []  # [{points:PackedVector2Array, color:Color, width:float}]
var _granules: Array = []  # [{pos:Vector2, r:float, color:Color}]  internal fermentation granules
var _halo: Array = []  # [{pos:Vector2, r:float, color:Color}]  excreted acetate-overflow dots around the cell
var _biofilm_poly := PackedVector2Array()  # translucent biofilm-matrix ground halo (like the plant shadow)
var _biofilm_color := Color(0, 0, 0, 0)
var _endospore: Dictionary = {}  # {center:Vector2, rx:float, ry:float, color:Color} the refractile endospore oval
var _host_ring_r := 0.0  # symbiont host-containment ring radius (0 = none)
var _host_ring_color := Color(0, 0, 0, 0)
var _host_tether: Array = []  # [Vector2] a host-coupling tether polyline (symbiont)
# Respiration cytoplasm texture (its OWN channel, independent of acetate tint): aerobic → crisp O2 dots,
# fermentative → striped cytoplasm. Precomputed at build() so --check catches a malformed shape, not only the GPU.
var _o2_dots: Array = []  # [{pos:Vector2, r:float}]  faint O2 dots (aerobic, crisp membrane)
var _stripes: Array = []  # [{a:Vector2, b:Vector2}]  cytoplasm stripes (fermentative)
var _cyto_color := Color(0.0, 0.0, 0.0, 0.0)  # stripe/O2 tint, derived from respiration
var _bounds := Rect2()


## Build the cell geometry from visual params, then request a redraw. Keys (all optional, with defaults):
##   shape:String("rod"/"coccus"/"vibrioid"/"wall_less")  length:float  width:float  curvature:float
##   septum_pinch:float  respiration:float  flagella_count:int  flagella_layout:String  flagella_len:float
##   granule_count:int  halo_count:int  biofilm:float  endospore:float  scale:float  host_ring:float
##   host_tether:float  seed:int  body_color/outline_color/granule_color/halo_color:Color
func build(p: Dictionary) -> void:
	var shape: String = str(p.get("shape", "rod"))
	var sc: float = maxf(0.05, float(p.get("scale", 1.0)))  # overall down/up-scale (symbiont speck = small)
	var length: float = maxf(8.0, float(p.get("length", 90.0))) * sc
	var width: float = maxf(6.0, float(p.get("width", 34.0))) * sc
	var curvature: float = clampf(float(p.get("curvature", 0.0)), 0.0, 1.0)  # vibrioid comma bend
	var septum_pinch: float = clampf(float(p.get("septum_pinch", 1.0 if bool(p.get("septum", false)) else 0.0)), 0.0, 1.0)
	var respiration: float = clampf(float(p.get("respiration", 0.5)), 0.0, 1.0)
	var flagella_count: int = clampi(int(p.get("flagella_count", 3)), 0, 8)
	var flagella_layout: String = str(p.get("flagella_layout", "peritrichous"))  # peritrichous/polar/none
	var flagella_len: float = maxf(0.0, float(p.get("flagella_len", 60.0))) * sc
	var granule_count: int = clampi(int(p.get("granule_count", 0)), 0, 14)
	var halo_count: int = clampi(int(p.get("halo_count", 0)), 0, 16)
	var biofilm: float = clampf(float(p.get("biofilm", 0.0)), 0.0, 1.0)
	var endospore: float = clampf(float(p.get("endospore", 0.0)), 0.0, 1.0)
	var host_ring: float = clampf(float(p.get("host_ring", 0.0)), 0.0, 1.0)
	var host_tether: float = clampf(float(p.get("host_tether", 0.0)), 0.0, 1.0)
	var seed_val: int = int(p.get("seed", 1))
	_body_color = p.get("body_color", Color(0.55, 0.78, 0.85))
	_outline_color = p.get("outline_color", Color(0.85, 0.95, 0.98, 0.9))
	var granule_color: Color = p.get("granule_color", Color(0.96, 0.86, 0.45, 0.85))
	var halo_color: Color = p.get("halo_color", Color(0.92, 0.62, 0.32, 0.6))

	_body_polys = []
	_septum = []
	_flagella = []
	_granules = []
	_halo = []
	_o2_dots = []
	_stripes = []
	_biofilm_poly = PackedVector2Array()
	_endospore = {}
	_host_tether = []

	var half := length * 0.5
	var r := width * 0.5
	# Wall-less → no crisp membrane ring; a faint fuzzy edge. Others → a crisp ring scaled to the body.
	_outline_dashed = (shape == "wall_less")
	_outline_width = 0.0 if _outline_dashed else maxf(1.5, r * 0.16)

	match shape:
		"coccus":
			_build_cocci(r, seed_val)
		"wall_less":
			_body_polys.append(_pleomorph_blob(maxf(half, r), seed_val))
		"vibrioid":
			_body_polys.append(_capsule(half, r, curvature))
		_:  # "rod" (and any unknown) — a straight or gently bent capsule
			_body_polys.append(_capsule(half, r, curvature))

	# Use the FIRST body poly as the reference cell for septum / cytoplasm / granule placement (cocci handle
	# their own scatter visually; the granules/stripes land in the primary cell, which reads fine in a cluster).
	var straight := maxf(0.0, half - r)

	# ── Septum (binary-fission constriction): a GRADED vertical pinch across the centre. Only for rod-like shapes
	# (a coccus / wall-less blob has no rod waist). Read as a dividing cell.
	if septum_pinch > 0.02 and (shape == "rod" or shape == "vibrioid"):
		var inset := r * (1.0 - 0.85 * septum_pinch)
		_septum.append({
			"a": Vector2(0.0, -inset),
			"b": Vector2(0.0, inset),
			"w": maxf(1.5, r * (0.04 + 0.08 * septum_pinch)),
		})

	# ── Respiration cytoplasm texture (its OWN channel), placed inside the primary cell.
	_cyto_color = Color(0.55, 0.85, 0.95, 0.35).lerp(Color(0.80, 0.55, 0.25, 0.4), respiration)
	if respiration < 0.6 and shape != "wall_less":
		var o2 := int(round((1.0 - respiration) * 7.0))
		for oi in o2:
			var ox := (_hash01(seed_val, oi, 91) - 0.5) * 2.0 * straight * 0.9
			var oy := (_hash01(seed_val, oi, 97) - 0.5) * 2.0 * r * 0.6
			_o2_dots.append({"pos": Vector2(ox, oy), "r": maxf(1.0, r * 0.07)})
	if respiration > 0.4 and shape != "wall_less":
		var n := 1 + int(round(respiration * 4.0))
		for si in n:
			var sx := (float(si + 1) / float(n + 1) - 0.5) * 2.0 * straight * 0.85
			var sh := r * (0.55 + 0.3 * _hash01(seed_val, si, 103))
			_stripes.append({"a": Vector2(sx, -sh), "b": Vector2(sx, sh)})

	# ── Flagella: layout-aware. peritrichous = fan off the left pole; polar = 1-2 long thick off the left pole;
	# none = no flagella (non-motile cocci / cutibacterium / wall-less).
	_build_flagella(flagella_layout, flagella_count, flagella_len, half, r, seed_val)

	# ── Internal granules (fermentation_capacity): scattered inside the primary cell.
	if shape != "wall_less":
		for gi in granule_count:
			var gx := (_hash01(seed_val, gi, 17) - 0.5) * 2.0 * straight * 0.92
			var gy := (_hash01(seed_val, gi, 31) - 0.5) * 2.0 * r * 0.62
			_granules.append({"pos": Vector2(gx, gy), "r": maxf(1.5, r * 0.14), "color": granule_color})

	# ── Acetate-overflow halo (acetate_overflow): excreted dots in a ring just outside the membrane.
	for hi in halo_count:
		var ang := TAU * float(hi) / maxf(1.0, float(halo_count)) + _hash01(seed_val, hi, 53)
		var rad := r + 8.0 * sc + _hash01(seed_val, hi, 71) * 14.0 * sc
		_halo.append({
			"pos": Vector2(cos(ang) * (straight + rad), sin(ang) * rad),
			"r": maxf(1.5, r * 0.10), "color": halo_color,
		})

	# ── Endospore (SporulationCapacity > 0): a bright refractile oval bulging one pole (the spo0A→sigF endospore).
	# Read as spore-CAPABLE — "you cannot sterilize a spore-former" made legible.
	if endospore > 0.02 and (shape == "rod" or shape == "vibrioid"):
		var er := r * (0.42 + 0.34 * endospore)
		_endospore = {
			"center": Vector2(straight * 0.55, 0.0),  # bulge the right pole
			"rx": er * 1.15, "ry": er,
			"color": Color(0.97, 0.98, 0.90, 0.92),  # bright/refractile
		}

	_recompute_bounds()

	# ── Biofilm-matrix halo (Pseudomonas): a faint translucent ground polygon around the cell (like the plant
	# shadow). Built AFTER bounds so it can size to the body span; excluded from bounds() so spacing stays stable.
	if biofilm > 0.02:
		_build_biofilm(biofilm, r)

	# ── Symbiont host-containment ring + coupling tether (lives inside a bacteriocyte; SymbiosisCapacity tether).
	if host_ring > 0.02:
		_host_ring_r = maxf(r * 1.8, _bounds.size.x * 0.9)
		_host_ring_color = Color(0.80, 0.74, 0.95, 0.30 + 0.30 * host_ring)
	else:
		_host_ring_r = 0.0
		_host_ring_color = Color(0, 0, 0, 0)
	if host_tether > 0.02:
		# A short wavy tether from the cell to the host ring edge (the host-coupling exchange).
		var n := 6
		for k in n + 1:
			var u := float(k) / float(n)
			var x := r + u * (_host_ring_r - r)
			var y := sin(u * PI * 2.0) * r * 0.4 * host_tether
			_host_tether.append(Vector2(x, y))

	queue_redraw()


## Local-space bounding box of the cell BODY only (the body polys), so the row spacing + auto-fit stay stable
## regardless of flagella/halo/biofilm extent (parallels lsystem.gd::bounds returning the branch-only box).
func bounds() -> Rect2:
	return _bounds


# ──────────────────────────── geometry builders (called from build) ────────────────────────────

## A stadium (capsule) outline, optionally bent into a comma by `curvature` (vibrioid). Top edge L→R, right cap,
## bottom edge R→L, left cap. The bend rotates each point about the centre proportionally to its x (a sheared arc).
func _capsule(half: float, r: float, curvature: float) -> PackedVector2Array:
	var straight := maxf(0.0, half - r)
	var seg := 10
	var raw := PackedVector2Array()
	raw.append(Vector2(-straight, -r))
	raw.append(Vector2(straight, -r))
	for k in range(1, seg):
		var a := -PI / 2.0 + PI * float(k) / float(seg)
		raw.append(Vector2(straight, 0.0) + Vector2(cos(a), sin(a)) * r)
	raw.append(Vector2(straight, r))
	raw.append(Vector2(-straight, r))
	for k in range(1, seg):
		var a := PI / 2.0 + PI * float(k) / float(seg)
		raw.append(Vector2(-straight, 0.0) + Vector2(cos(a), sin(a)) * r)
	if curvature <= 0.01:
		return raw
	# Bend: map x∈[-half,half] to an arc of total angle `curvature*~110°`, so the rod becomes a comma/banana.
	var bend := curvature * deg_to_rad(110.0)
	var out := PackedVector2Array()
	var rad := (half * 2.0) / maxf(0.001, bend)  # arc radius so the spine arc-length ≈ the rod length
	for v in raw:
		var t := v.x / maxf(0.001, half)  # -1..1 along the spine
		var ang := t * bend * 0.5
		# Position along the arc + offset by the cross-section y, rotated to the local tangent.
		var spine := Vector2(sin(ang) * rad, (1.0 - cos(ang)) * rad)
		var normal := Vector2(cos(ang), sin(ang))  # outward (towards +y of the spine)
		out.append(spine + normal * v.y)
	return out


## A grape-cluster of 2-4 daughter spheres (cocci), via deterministic jitter (no flagella drawn by the caller).
func _build_cocci(r: float, seed_val: int) -> void:
	var n := 2 + int(_hash01(seed_val, 0, 7) * 3.0)  # 2..4
	var poly_r := r
	var spread := poly_r * 0.95
	for ci in n:
		var ang := TAU * float(ci) / float(n) + _hash01(seed_val, ci, 11) * 0.6
		var off := Vector2(cos(ang), sin(ang)) * spread * (0.0 if ci == 0 else 1.0)
		var poly := PackedVector2Array()
		var seg := 14
		for k in seg:
			var a := TAU * float(k) / float(seg)
			poly.append(off + Vector2(cos(a), sin(a)) * poly_r)
		_body_polys.append(poly)


## An irregular soft jittered closed polygon for the wall-less pleomorph (no crisp ring, faint fuzzy edge).
func _pleomorph_blob(rad: float, seed_val: int) -> PackedVector2Array:
	var poly := PackedVector2Array()
	var seg := 16
	for k in seg:
		var a := TAU * float(k) / float(seg)
		var jitter := 0.6 + 0.7 * _hash01(seed_val, k, 41)  # irregular radius per vertex
		poly.append(Vector2(cos(a), sin(a)) * rad * jitter)
	return poly


func _build_flagella(layout: String, count: int, flen: float, half: float, r: float, seed_val: int) -> void:
	if layout == "none" or count <= 0 or flen <= 0.0:
		return
	var n := count
	var thick := maxf(1.0, r * 0.08)
	if layout == "polar":
		n = clampi(count, 1, 2)  # polar = 1-2 flagella
		thick = maxf(1.5, r * 0.16)  # thicker/sheathed (vibrioid attack flagellum)
		flen *= 1.35
	for fi in n:
		var pts := PackedVector2Array()
		var spread := 0.0
		if layout == "peritrichous":
			spread = (float(fi) / maxf(1.0, float(n - 1)) - 0.5) * 0.9
		else:  # polar
			spread = (float(fi) - float(n - 1) * 0.5) * 0.18
		var root := Vector2(-half, 0.0) + Vector2(-r * 0.1, sin(spread) * r * 0.8)
		var steps := 9
		for k in steps + 1:
			var u := float(k) / float(steps)
			var x := root.x - u * flen
			var phase := _hash01(seed_val, fi, k) * TAU
			var amp := 3.5 + u * (r * 0.55)
			var y := root.y + sin(u * PI * 2.4 + phase) * amp + spread * u * flen * 0.25
			pts.append(Vector2(x, y))
		_flagella.append({
			"points": pts,
			"color": Color(_body_color.r, _body_color.g, _body_color.b, 0.7),
			"width": thick,
		})


## A faint translucent biofilm-matrix polygon ringing the cell (a soft 18-gon halo), behind the body.
func _build_biofilm(biofilm: float, r: float) -> void:
	_biofilm_poly = PackedVector2Array()
	var rx := maxf(r * 1.6, _bounds.size.x * 0.62) * (0.9 + 0.5 * biofilm)
	var ry := maxf(r * 1.4, _bounds.size.y * 0.62) * (0.9 + 0.5 * biofilm)
	for k in 18:
		var a := TAU * float(k) / 18.0
		_biofilm_poly.append(_bounds.get_center() + Vector2(cos(a) * rx, sin(a) * ry))
	_biofilm_color = Color(0.40, 0.70, 0.55, 0.10 + 0.18 * biofilm)


func _recompute_bounds() -> void:
	if _body_polys.is_empty():
		_bounds = Rect2()
		return
	var mn := Vector2.INF
	var mx := -Vector2.INF
	for poly in _body_polys:
		for v in poly:
			mn = mn.min(v)
			mx = mx.max(v)
	_bounds = Rect2(mn, mx - mn)


# ──────────────────────────── drawing (no geometry decisions here) ────────────────────────────

func _draw() -> void:
	# Biofilm matrix halo first (behind everything, like the plant shadow).
	if not _biofilm_poly.is_empty():
		draw_colored_polygon(_biofilm_poly, _biofilm_color)
	# Symbiont host-containment ring (behind the cell — it lives inside the host).
	if _host_ring_r > 0.0:
		draw_arc(_bounds.get_center(), _host_ring_r, 0.0, TAU, 26, _host_ring_color, maxf(1.0, _host_ring_r * 0.06), true)
		if _host_tether.size() >= 2:
			draw_polyline(_host_tether, Color(_host_ring_color.r, _host_ring_color.g, _host_ring_color.b, 0.55), maxf(1.0, _host_ring_r * 0.04), true)
	# Flagella behind the body.
	for fl in _flagella:
		var pts: PackedVector2Array = fl["points"]
		if pts.size() >= 2:
			draw_polyline(pts, fl["color"], fl["width"], true)
	# Acetate halo dots (behind the membrane outline so they read as excreted/around).
	for h in _halo:
		draw_circle(h["pos"], h["r"], h["color"])
	# Body fill + membrane outline, per cell (cocci cluster = several).
	for poly in _body_polys:
		if poly.size() < 3:
			continue
		draw_colored_polygon(poly, _body_color)
		if _outline_width > 0.0:
			var ring := PackedVector2Array(poly)
			ring.append(poly[0])
			draw_polyline(ring, _outline_color, _outline_width, true)
		elif _outline_dashed:
			# Wall-less: a faint fuzzy edge — draw a soft thin outline at low alpha (no crisp ring).
			var ring := PackedVector2Array(poly)
			ring.append(poly[0])
			draw_polyline(ring, Color(_outline_color.r, _outline_color.g, _outline_color.b, 0.22), 1.5, true)
	# Respiration cytoplasm texture: fermentative stripes (under the granules), then aerobic O2 dots.
	for st in _stripes:
		draw_line(st["a"], st["b"], _cyto_color, maxf(1.0, _bounds.size.y * 0.03), true)
	for o in _o2_dots:
		draw_circle(o["pos"], o["r"], _cyto_color)
	# Internal granules.
	for g in _granules:
		draw_circle(g["pos"], g["r"], g["color"])
	# Endospore (refractile oval bulging one pole) — drawn on top so it reads as the bright spore body.
	if not _endospore.is_empty():
		var c: Vector2 = _endospore["center"]
		var rx: float = _endospore["rx"]
		var ry: float = _endospore["ry"]
		var ell := PackedVector2Array()
		for k in 16:
			var a := TAU * float(k) / 16.0
			ell.append(c + Vector2(cos(a) * rx, sin(a) * ry))
		draw_colored_polygon(ell, _endospore["color"])
	# Septum (graded dividing-cell pinch).
	for s in _septum:
		draw_line(s["a"], s["b"], _outline_color, float(s.get("w", maxf(1.5, _bounds.size.y * 0.05))), true)


## Deterministic [0,1) hash for per-glyph jitter (no global RNG — inv #3 hygiene). Mirrors lsystem.gd::_hash01.
func _hash01(a: int, b: int, c: int) -> float:
	var h := (a * 73856093) ^ ((b + 1) * 19349663) ^ ((c + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
