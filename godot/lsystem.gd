extends Node2D
## Parametric bracketed L-system "plant", drawn with turtle graphics.
##
## INVARIANT #2 (STOP THE LINE if violated): this is PURE PRESENTATION. It expands a fixed production grammar
## into a turtle path and draws lines, leaf polygons + flowers from NUMERIC parameters. It computes NO
## genotype→phenotype biology. The genome→trait map lives in the Rust core; mapping those trait values to
## these visual params (done by the caller, see main.gd::_plant_params_from_traits) is presentation, not
## biology — exactly the "L-system rule params" role the SPEC assigns to the renderer.
##
## All geometry (leaf/flower polygons, shadow) is precomputed in build(); _draw() only iterates and emits
## draw_* calls. This matters because the headless gate (--check) runs build() but never _draw() — so a
## malformed polygon must surface at build time, not only under a GPU.
##
## Grammar (a classic bushy plant; ABOP-style):
##   axiom: X      X → F+[[X]-X]-F[-FX]+X      F → FF
## Turtle: F draw+advance · +/- turn by `angle` · [ push · ] pop (and drop a leaf at the tip) · X no-op.

const AXIOM := "X"
const RULES := {"X": "F+[[X]-X]-F[-FX]+X", "F": "FF"}

var _segments: Array = []  # [{a:Vector2, b:Vector2, width:float, color:Color}]
var _leaf_polys: Array = []  # [{poly:PackedVector2Array, color:Color}]
var _flowers: Array = []  # [{petals:Array[Vector2], r:float, color:Color, center:Vector2, center_color:Color}]
var _shadow_poly := PackedVector2Array()
var _shadow_color := Color(0, 0, 0, 0.22)
var _ground_a := Vector2.ZERO
var _ground_b := Vector2.ZERO
var _ground_color := Color(0.10, 0.16, 0.09, 0.9)
var _bounds := Rect2()


## Build the plant geometry from visual params, then request a redraw. Keys (all optional, with defaults):
##   iterations:int  angle_deg:float  segment_len:float  len_falloff:float  thickness:float
##   leaf_size:float  leaf_aspect:float  jitter_deg:float  seed:int
##   flower_count:int  petal_count:int
##   branch_base:Color  branch_tip:Color  leaf_color:Color  flower_color:Color
func build(p: Dictionary) -> void:
	var iterations: int = clampi(int(p.get("iterations", 5)), 1, 6)
	var angle: float = deg_to_rad(float(p.get("angle_deg", 25.0)))
	var seg: float = float(p.get("segment_len", 9.0))
	var falloff: float = float(p.get("len_falloff", 0.85))
	var thickness: float = float(p.get("thickness", 4.0))
	var leaf_size: float = float(p.get("leaf_size", 4.0))
	var leaf_aspect: float = float(p.get("leaf_aspect", 0.55))
	var jitter: float = deg_to_rad(float(p.get("jitter_deg", 6.0)))
	var seed_val: int = int(p.get("seed", 1))
	var flower_count: int = int(p.get("flower_count", 0))
	var petal_count: int = maxi(4, int(p.get("petal_count", 5)))
	var branch_base: Color = p.get("branch_base", Color(0.36, 0.24, 0.12))
	var branch_tip: Color = p.get("branch_tip", Color(0.30, 0.55, 0.20))
	var leaf_color: Color = p.get("leaf_color", Color(0.45, 0.80, 0.30))
	var flower_color: Color = p.get("flower_color", Color(0.95, 0.55, 0.55))

	_segments.clear()
	_leaf_polys.clear()
	_flowers.clear()

	var s := _expand(iterations)
	# Turtle state.
	var pos := Vector2.ZERO
	var heading := -PI / 2.0  # point up (screen -y)
	var depth := 0
	var stack: Array = []
	var rng := 0  # cheap deterministic counter folded into the hash for per-branch jitter
	# Collect leaf tips first (pos + LIVE tip heading), so flowers can be chosen deterministically after.
	var leaf_tips: Array = []  # [{pos:Vector2, heading:float}]

	for ch in s:
		match ch:
			"F":
				var jit := (_hash01(seed_val, rng, depth) - 0.5) * 2.0 * jitter
				rng += 1
				var h := heading + jit
				var step := seg * pow(falloff, float(depth))
				var np := pos + Vector2.from_angle(h) * step
				var t := clampf(float(depth) / 5.0, 0.0, 1.0)
				_segments.append({
					"a": pos, "b": np,
					"width": maxf(1.0, thickness * pow(0.72, float(depth))),
					"color": branch_base.lerp(branch_tip, t),
				})
				pos = np
			"+":
				heading -= angle
			"-":
				heading += angle
			"[":
				stack.push_back({"pos": pos, "heading": heading, "depth": depth})
				depth += 1
			"]":
				# A leaf sits at the tip we are about to leave; orient it along the LIVE tip heading
				# (NOT the popped parent heading — that would point leaves back down the branch).
				leaf_tips.append({"pos": pos, "heading": heading})
				var st: Dictionary = stack.pop_back()
				pos = st["pos"]
				heading = st["heading"]
				depth = st["depth"]
			_:
				pass  # X and any other symbol: no turtle action

	# Pick flower sites deterministically: the `flower_count` leaf tips with the smallest hash.
	var flower_idx := {}
	if flower_count > 0 and not leaf_tips.is_empty():
		var order: Array = []
		for i in leaf_tips.size():
			order.append([_hash01(seed_val, i, 99), i])
		order.sort_custom(func(a, b): return a[0] < b[0])
		for k in mini(flower_count, order.size()):
			flower_idx[order[k][1]] = true

	# Precompute leaf polygons and flowers (geometry built here so --check catches malformed shapes).
	for i in leaf_tips.size():
		var tip: Dictionary = leaf_tips[i]
		if flower_idx.has(i):
			_flowers.append(_make_flower(tip["pos"], leaf_size * 1.1, petal_count, flower_color))
		else:
			_leaf_polys.append({
				"poly": _make_leaf(tip["pos"], tip["heading"], leaf_size * 2.2, leaf_aspect),
				"color": leaf_color,
			})

	_recompute_bounds()
	_build_ground_and_shadow(thickness)
	queue_redraw()


## Local-space bounding box of the plant's BRANCHES only (origin = base). Leaves/flowers/ground are excluded
## so per-specimen row spacing + auto-fit stay stable regardless of foliage. Callers fit/scale with this.
func bounds() -> Rect2:
	return _bounds


# ──────────────────────────── geometry builders (called from build) ────────────────────────────

## A 6-point almond/teardrop leaf pointing along `heading`, rooted at `pos`. Length `len`, width `len*aspect`.
func _make_leaf(pos: Vector2, heading: float, len: float, aspect: float) -> PackedVector2Array:
	var w := len * clampf(aspect, 0.2, 1.0)
	# Local outline along +x (root at origin, tip at +x); rotate to heading, translate to pos.
	var local := [
		Vector2(0.0, 0.0),
		Vector2(0.30 * len, -0.45 * w),
		Vector2(0.70 * len, -0.32 * w),
		Vector2(len, 0.0),
		Vector2(0.70 * len, 0.32 * w),
		Vector2(0.30 * len, 0.45 * w),
	]
	var out := PackedVector2Array()
	for v in local:
		out.append(pos + (v as Vector2).rotated(heading))
	return out


## A flower as a ring of `petals` small circles + a centre dot (precomputed positions; drawn as circles).
func _make_flower(pos: Vector2, r: float, petals: int, color: Color) -> Dictionary:
	var sites: Array = []
	for k in petals:
		var ang := TAU * float(k) / float(petals)
		sites.append(pos + Vector2.from_angle(ang) * r * 0.9)
	return {
		"petals": sites,
		"r": maxf(1.5, r * 0.62),
		"color": color,
		"center": pos,
		"center_color": Color(0.98, 0.86, 0.40),
	}


## A flattened 16-gon shadow ellipse + a ground line under the base (origin). Godot 4 has no draw_ellipse,
## so the shadow is a precomputed polygon. Sized from the branch bounds; excluded from bounds().
func _build_ground_and_shadow(thickness: float) -> void:
	_shadow_poly = PackedVector2Array()
	if _segments.is_empty():
		_ground_a = Vector2.ZERO
		_ground_b = Vector2.ZERO
		return
	var half := maxf(14.0, _bounds.size.x * 0.36)
	var rx := half
	var ry := maxf(3.0, thickness * 0.9)
	for k in 16:
		var a := TAU * float(k) / 16.0
		_shadow_poly.append(Vector2(cos(a) * rx, sin(a) * ry))  # centred at origin (the base, y≈0)
	_ground_a = Vector2(-half, 0.0)
	_ground_b = Vector2(half, 0.0)


# ──────────────────────────── drawing (no geometry decisions here) ────────────────────────────

func _draw() -> void:
	# Ground + shadow first (behind the plant).
	if not _shadow_poly.is_empty():
		draw_colored_polygon(_shadow_poly, _shadow_color)
		draw_line(_ground_a, _ground_b, _ground_color, 2.0, true)
	for seg in _segments:
		draw_line(seg["a"], seg["b"], seg["color"], seg["width"], true)
	for leaf in _leaf_polys:
		draw_colored_polygon(leaf["poly"], leaf["color"])
	for fl in _flowers:
		for petal in fl["petals"]:
			draw_circle(petal, fl["r"], fl["color"])
		draw_circle(fl["center"], fl["r"] * 0.7, fl["center_color"])


func _expand(iterations: int) -> String:
	var s := AXIOM
	for _i in iterations:
		var out := ""
		for ch in s:
			out += RULES.get(ch, ch)
		s = out
	return s


func _recompute_bounds() -> void:
	if _segments.is_empty():
		_bounds = Rect2()
		return
	var mn := Vector2.INF
	var mx := -Vector2.INF
	for seg in _segments:
		mn = mn.min(seg["a"]).min(seg["b"])
		mx = mx.max(seg["a"]).max(seg["b"])
	_bounds = Rect2(mn, mx - mn)


## Deterministic [0,1) hash for per-branch jitter / flower-site selection (no global RNG — inv #3 hygiene).
func _hash01(a: int, b: int, c: int) -> float:
	var h := (a * 73856093) ^ ((b + 1) * 19349663) ^ ((c + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
