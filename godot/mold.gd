extends Node2D
## Parametric filamentous-fungus "mold" specimen glyph (SP-4): branching hyphae (a mycelium of recursive-turtle
## filaments) topped by a CONIDIOPHORE — the asexual spore-bearing head. Aspergillus = a globose vesicle with
## radiating biseriate conidia chains (the black A. niger head); Penicillium = the brush-like penicillus. The
## sibling of lsystem.gd / microbe.gd: same Node2D + build(Dictionary) + bounds()->Rect2 contract, so main.gd's
## specimen row/label/focus/framing machinery reuses it verbatim.
##
## INVARIANT #2 (STOP THE LINE if violated): PURE PRESENTATION. It maps already-expressed trait scalars
## (growth_rate → hyphal extent, sporulation_capacity → conidia chain density — the brlA→abaA→wetA cascade made
## visual) to pixels. It computes NO genotype→phenotype biology; the genome→trait map lives in the Rust core.
##
## All geometry is precomputed in build(); _draw() only iterates and emits draw_* calls — so the headless gate
## (--check) catches a malformed polygon at build time, not only under a GPU. Jitter is a deterministic hash of
## the specimen index (no global RNG — inv #3), so the view is stable + hash-irrelevant.

var _hyphae: Array = []  # [{a:Vector2, b:Vector2, width:float, color:Color}] mycelium filaments
var _stalk: Array = []  # [{a:Vector2, b:Vector2, width:float}] the conidiophore stalk (the brlA bristle)
var _vesicle: Dictionary = {}  # {center:Vector2, r:float, color:Color} the globose vesicle (Aspergillus); {} for brush
var _conidia: Array = []  # [{pos:Vector2, r:float, color:Color}] the conidia (spore) chain dots
var _metulae: Array = []  # [{a:Vector2, b:Vector2, width:float}] the brush/biseriate stalklets carrying chains
var _stalk_color := Color(0.62, 0.58, 0.40)
var _bounds := Rect2()


## Build the mold geometry from visual params, then request a redraw. Keys (all optional, with defaults):
##   head:String("aspergillus"/"penicillium")  hyphae_count:int  hyphae_len:float  growth:float
##   conidia_density:float (0..1, the brlA→abaA→wetA cascade)  conidia_color:Color  stalk_color:Color
##   vesicle_color:Color  seed:int
func build(p: Dictionary) -> void:
	var head: String = str(p.get("head", "aspergillus"))
	var hyphae_count: int = clampi(int(p.get("hyphae_count", 4)), 1, 8)
	var hyphae_len: float = maxf(8.0, float(p.get("hyphae_len", 60.0)))
	var conidia_density: float = clampf(float(p.get("conidia_density", 0.6)), 0.0, 1.0)
	var conidia_color: Color = p.get("conidia_color", Color(0.18, 0.16, 0.18, 0.95))  # black A. niger spores
	var vesicle_color: Color = p.get("vesicle_color", Color(0.40, 0.36, 0.34))
	_stalk_color = p.get("stalk_color", Color(0.62, 0.58, 0.40))
	var seed_val: int = int(p.get("seed", 1))

	_hyphae = []
	_stalk = []
	_vesicle = {}
	_conidia = []
	_metulae = []

	# ── Mycelium: a few recursive-turtle filaments fanning from the base (origin), pointing up + branching.
	# Drawn DOWN/around the base so the conidiophore (up) reads as the fruiting head above the mat.
	for hi in hyphae_count:
		var base_ang := PI * 0.5 + (float(hi) / maxf(1.0, float(hyphae_count - 1)) - 0.5) * deg_to_rad(150.0)
		_grow_hypha(Vector2.ZERO, base_ang, hyphae_len, maxf(1.0, hyphae_len * 0.05), 0, seed_val * 31 + hi)

	# ── Conidiophore stalk: a single thick filament rising UP from the base (screen -y) — the brlA bristle.
	var stalk_top := Vector2(0.0, -hyphae_len * 1.4)
	_stalk.append({"a": Vector2.ZERO, "b": stalk_top, "width": maxf(2.0, hyphae_len * 0.08)})

	# ── Head: density of conidia chains driven by conidia_density (the brlA→abaA→wetA cascade). Count of chains
	# and dots-per-chain both scale, so a knockdown (density→0) leaves a bare stalk (the brlA-null bristle).
	var n_chains := 4 + int(round(conidia_density * 10.0))
	var dots_per_chain := 2 + int(round(conidia_density * 5.0))
	var conidium_r := maxf(2.0, hyphae_len * 0.045)

	if head == "penicillium":
		# Brush (penicillus): the stalk branches into metulae → phialides → chains, fanning in a brush.
		var spread := deg_to_rad(70.0)
		for ci in n_chains:
			var t := float(ci) / maxf(1.0, float(n_chains - 1)) - 0.5
			var ang := -PI * 0.5 + t * spread  # upward fan
			var metula_len := hyphae_len * 0.30
			var mtip := stalk_top + Vector2.from_angle(ang) * metula_len
			_metulae.append({"a": stalk_top, "b": mtip, "width": maxf(1.0, hyphae_len * 0.035)})
			_emit_chain(mtip, ang, dots_per_chain, conidium_r, conidia_color, seed_val * 7 + ci)
	else:
		# Aspergillus: a globose vesicle topping the stalk, with radiating biseriate conidia chains all around.
		var vr := hyphae_len * 0.22
		_vesicle = {"center": stalk_top, "r": vr, "color": vesicle_color}
		for ci in n_chains:
			var ang := -PI + PI * (float(ci) / maxf(1.0, float(n_chains - 1)))  # radiate over the upper hemisphere
			var root := stalk_top + Vector2.from_angle(ang) * vr
			_emit_chain(root, ang, dots_per_chain, conidium_r, conidia_color, seed_val * 13 + ci)

	_recompute_bounds()
	queue_redraw()


## Recursive hypha: draw a segment, then (probabilistically by depth, deterministically by hash) branch.
func _grow_hypha(pos: Vector2, ang: float, length: float, width: float, depth: int, seed_val: int) -> void:
	if depth > 3 or length < 6.0:
		return
	var jit := (_hash01(seed_val, depth, 5) - 0.5) * deg_to_rad(40.0)
	var a := ang + jit
	var np := pos + Vector2.from_angle(a) * length
	_hyphae.append({
		"a": pos, "b": np,
		"width": maxf(0.8, width),
		"color": Color(0.55, 0.62, 0.45, 0.85).lerp(Color(0.70, 0.74, 0.55, 0.85), float(depth) / 3.0),
	})
	# Branch into 1-2 children, shorter + thinner.
	var branches := 1 + (1 if _hash01(seed_val, depth, 9) > 0.45 else 0)
	for bi in branches:
		var spread := (float(bi) - float(branches - 1) * 0.5) * deg_to_rad(35.0)
		_grow_hypha(np, a + spread, length * 0.74, width * 0.7, depth + 1, seed_val * 3 + bi)


## A chain of conidia (spore dots) marching out from `root` along `ang`, spaced ~2r apart (the wetA spore chain).
func _emit_chain(root: Vector2, ang: float, count: int, r: float, color: Color, seed_val: int) -> void:
	var dir := Vector2.from_angle(ang)
	for k in count:
		var jit := (_hash01(seed_val, k, 3) - 0.5) * r * 0.6
		var perp := Vector2(-dir.y, dir.x) * jit
		var pos := root + dir * (r * 2.0 * float(k + 1)) + perp
		_conidia.append({"pos": pos, "r": r, "color": color})


## Local-space bounding box of the whole mold (hyphae mat + stalk + head), so the row spacing + auto-fit stay
## stable (a tall conidiophore is much taller than a microbe — main.gd's adaptive spacing reads this).
func bounds() -> Rect2:
	return _bounds


func _recompute_bounds() -> void:
	var has := false
	var mn := Vector2.INF
	var mx := -Vector2.INF
	for h in _hyphae:
		mn = mn.min(h["a"]).min(h["b"])
		mx = mx.max(h["a"]).max(h["b"])
		has = true
	for s in _stalk:
		mn = mn.min(s["a"]).min(s["b"])
		mx = mx.max(s["a"]).max(s["b"])
		has = true
	for c in _conidia:
		mn = mn.min(c["pos"] - Vector2(c["r"], c["r"]))
		mx = mx.max(c["pos"] + Vector2(c["r"], c["r"]))
		has = true
	if not has:
		_bounds = Rect2()
		return
	_bounds = Rect2(mn, mx - mn)


# ──────────────────────────── drawing (no geometry decisions here) ────────────────────────────

func _draw() -> void:
	# Mycelium mat first (behind the conidiophore).
	for h in _hyphae:
		draw_line(h["a"], h["b"], h["color"], h["width"], true)
	# Conidiophore stalk.
	for s in _stalk:
		draw_line(s["a"], s["b"], _stalk_color, s["width"], true)
	# Brush metulae (Penicillium).
	for m in _metulae:
		draw_line(m["a"], m["b"], _stalk_color, m["width"], true)
	# Globose vesicle (Aspergillus).
	if not _vesicle.is_empty():
		draw_circle(_vesicle["center"], _vesicle["r"], _vesicle["color"])
	# Conidia (spore) chains on top.
	for c in _conidia:
		draw_circle(c["pos"], c["r"], c["color"])


## Deterministic [0,1) hash (no global RNG — inv #3 hygiene). Mirrors lsystem.gd / microbe.gd::_hash01.
func _hash01(a: int, b: int, c: int) -> float:
	var h := (a * 73856093) ^ ((b + 1) * 19349663) ^ ((c + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
