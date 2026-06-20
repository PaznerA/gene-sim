extends Node2D
## Parametric bracketed L-system "plant", drawn with turtle graphics.
##
## INVARIANT #2 (STOP THE LINE if violated): this is PURE PRESENTATION. It expands a fixed production grammar
## into a turtle path and draws lines + leaves from NUMERIC parameters. It computes NO genotype→phenotype
## biology. The genome→trait map lives in the Rust core; mapping those trait values to these visual params
## (done by the caller, see main.gd::_plant_params_from_traits) is presentation, not biology — exactly the
## "L-system rule params" role the SPEC assigns to the renderer.
##
## Grammar (a classic bushy plant; ABOP-style):
##   axiom: X      X → F+[[X]-X]-F[-FX]+X      F → FF
## Turtle: F draw+advance · +/- turn by `angle` · [ push · ] pop (and drop a leaf at the tip) · X no-op.

const AXIOM := "X"
const RULES := {"X": "F+[[X]-X]-F[-FX]+X", "F": "FF"}

var _segments: Array = []  # [{a:Vector2, b:Vector2, width:float, color:Color}]
var _leaves: Array = []  # [{pos:Vector2, size:float, color:Color}]
var _bounds := Rect2()


## Build the plant geometry from visual params, then request a redraw. Keys (all optional, with defaults):
##   iterations:int  angle_deg:float  segment_len:float  len_falloff:float  thickness:float
##   leaf_size:float  jitter_deg:float  seed:int  branch_base:Color  branch_tip:Color  leaf_color:Color
func build(p: Dictionary) -> void:
	var iterations: int = clampi(int(p.get("iterations", 5)), 1, 6)
	var angle: float = deg_to_rad(float(p.get("angle_deg", 25.0)))
	var seg: float = float(p.get("segment_len", 9.0))
	var falloff: float = float(p.get("len_falloff", 0.85))
	var thickness: float = float(p.get("thickness", 4.0))
	var leaf_size: float = float(p.get("leaf_size", 4.0))
	var jitter: float = deg_to_rad(float(p.get("jitter_deg", 6.0)))
	var seed_val: int = int(p.get("seed", 1))
	var branch_base: Color = p.get("branch_base", Color(0.36, 0.24, 0.12))
	var branch_tip: Color = p.get("branch_tip", Color(0.30, 0.55, 0.20))
	var leaf_color: Color = p.get("leaf_color", Color(0.45, 0.80, 0.30))

	_segments.clear()
	_leaves.clear()

	var s := _expand(iterations)
	# Turtle state.
	var pos := Vector2.ZERO
	var heading := -PI / 2.0  # point up (screen -y)
	var depth := 0
	var stack: Array = []
	var rng := 0  # cheap deterministic counter folded into the hash for per-branch jitter
	var max_depth := 1

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
				max_depth = maxi(max_depth, depth)
			"]":
				# A leaf sits at the tip we are about to leave.
				_leaves.append({"pos": pos, "size": leaf_size, "color": leaf_color})
				var st: Dictionary = stack.pop_back()
				pos = st["pos"]
				heading = st["heading"]
				depth = st["depth"]
			_:
				pass  # X and any other symbol: no turtle action

	_recompute_bounds()
	queue_redraw()


## Local-space bounding box of the drawn plant (origin = base). Lets the caller fit/scale it.
func bounds() -> Rect2:
	return _bounds


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


func _draw() -> void:
	for seg in _segments:
		draw_line(seg["a"], seg["b"], seg["color"], seg["width"], true)
	for leaf in _leaves:
		draw_circle(leaf["pos"], leaf["size"], leaf["color"])
		draw_circle(leaf["pos"] - Vector2(leaf["size"], leaf["size"]) * 0.3, leaf["size"] * 0.35, Color(1, 1, 1, 0.45))


## Deterministic [0,1) hash for per-branch jitter (organic look, reproducible for a given seed).
func _hash01(a: int, b: int, c: int) -> float:
	var h := (a * 73856093) ^ ((b + 1) * 19349663) ^ ((c + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
