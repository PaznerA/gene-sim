extends SceneTree
## ADR-029 S6 code-level proof (no display needed). Exercises the colony POLISH/PERF render surface deterministically:
##   1. PERF LEVER (the headline): at Field scope the colony layer's draw-primitive count is O(#colonies) — bounded by
##      the connected-region count and INDEPENDENT of cells × MAX_DOTS_PER_CELL. A mostly-single-species map of N and
##      of 4N cells builds the SAME small district count (tens), proving the de-spam scales to bigger maps (the
##      perf-bigger-maps lever). Compared to the old O(cells × MAX_DOTS_PER_CELL) per-organism dot count, it is tiny.
##   2. SELECTED-POP CAP HARDENING: a map-spanning SELECTED colony cannot re-spam — selected_pop_report caps the
##      forced-pop sprites at SELECTED_POP_BUDGET in the deterministic draw order, and a viewport rect CULLS
##      off-screen cells WITHOUT charging the budget (visible cells only).
##   3. DISTRICT INSPECT: selected_colony_summary() returns the registry fields {species, variant, label,
##      gen_created, parent} joined with the live cell-count for the inspect card.
##   4. LABEL DECLUTTER: _label_plan() labels only the selected district + districts >= LABEL_MIN_CELLS, de-overlapped
##      by centroid distance (highest-priority first) — a tiny non-selected district is suppressed; selecting it (or a
##      district overlapping a bigger one) forces its label in.
## Run: godot --headless --path godot --script colony_s6_test.gd   (prints COLONY_S6_TEST_OK on success)

const Snapshot := preload("res://snapshot.gd")
const Colonies := preload("res://colonies.gd")
const Organisms := preload("res://organisms.gd")
const SpeciesVisualMap := preload("res://species_visual_map.gd")

var _fail := false

func _ck(cond: bool, msg: String) -> void:
	if not cond:
		printerr("COLONY_S6_TEST_FAIL: ", msg)
		_fail = true


# A mostly-single-species PLANT field (sid 0, variant 0) filling the WHOLE w×h grid, with 3 fixed nested brushed
# discs (variants 11/12/13) at fractional centres → exactly 4 connected components (1 parent + 3 children) regardless
# of grid size. Density 1.0 everywhere (every cell populated → the per-organism path would draw w*h*MAX_DOTS dots).
func _perf_snap(w: int, h: int):
	var snap = Snapshot.new()
	snap.width = w
	snap.height = h
	var n := w * h
	var dens := PackedFloat32Array(); dens.resize(n)
	var fit := PackedFloat32Array(); fit.resize(n)
	var spc := PackedFloat32Array(); spc.resize(n)
	var var_p := PackedFloat32Array(); var_p.resize(n)
	var r: int = maxi(2, w / 12)
	var discs := [[w * 0.25, h * 0.25, 11], [w * 0.5, h * 0.6, 12], [w * 0.75, h * 0.4, 13]]
	for y in h:
		for x in w:
			var i := y * w + x
			dens[i] = 1.0
			fit[i] = 0.6
			spc[i] = 0.0
			var vid := 0
			for d in discs:
				var dx := x - int(d[0])
				var dy := y - int(d[1])
				if dx * dx + dy * dy <= r * r:
					vid = int(d[2])
					break
			var_p[i] = float(vid)
	snap.density = dens
	snap.fitness = fit
	snap.dominant_species_id = spc
	snap.dominant_variant_id = var_p
	return snap


# A fully-populated single-(species,variant) field — every cell is sid 0, variant 0 → one map-spanning colony. Used
# to prove the selected-pop budget/cull caps a colony that covers the whole map.
func _solid_snap(w: int, h: int):
	var snap = Snapshot.new()
	snap.width = w
	snap.height = h
	var n := w * h
	var dens := PackedFloat32Array(); dens.resize(n)
	var spc := PackedFloat32Array(); spc.resize(n)
	var var_p := PackedFloat32Array(); var_p.resize(n)
	for i in n:
		dens[i] = 1.0
		spc[i] = 0.0
		var_p[i] = 0.0
	snap.density = dens
	snap.dominant_species_id = spc
	snap.dominant_variant_id = var_p
	return snap


# The declutter / inspect map (40×30): a BIG plant block A (sid 0, var 0) with a small nested brushed disc C
# (sid 0, var 7), plus a tiny far MICROBE island B (sid 1, var 0, 2×2 = 4 cells < LABEL_MIN_CELLS). Keys: A=0, C=7,
# B=65536 — three distinct districts.
func _declutter_snap():
	var w := 40
	var h := 30
	var snap = Snapshot.new()
	snap.width = w
	snap.height = h
	var n := w * h
	var dens := PackedFloat32Array(); dens.resize(n)
	var fit := PackedFloat32Array(); fit.resize(n)
	var spc := PackedFloat32Array(); spc.resize(n)
	var var_p := PackedFloat32Array(); var_p.resize(n)
	# Block A (plant sid 0) at x[2,16) y[2,16).
	for y in range(2, 16):
		for x in range(2, 16):
			var i := y * w + x
			dens[i] = 1.0; fit[i] = 0.6; spc[i] = 0.0; var_p[i] = 0.0
	# Disc C (variant 7) centred (8,8) radius 2 → 13 cells, strictly inside A.
	for y in range(6, 11):
		for x in range(6, 11):
			var dx := x - 8
			var dy := y - 8
			if dx * dx + dy * dy <= 4:
				var_p[y * w + x] = 7.0
	# Island B (microbe sid 1) at x[30,32) y[24,26) → 2×2 = 4 cells, far from A.
	for y in range(24, 26):
		for x in range(30, 32):
			var i := y * w + x
			dens[i] = 1.0; fit[i] = 0.5; spc[i] = 1.0; var_p[i] = 0.0
	snap.density = dens
	snap.fitness = fit
	snap.dominant_species_id = spc
	snap.dominant_variant_id = var_p
	return snap


func _plan_has_key(plan: Array, key: int) -> bool:
	for c in plan:
		if int(c.get("key", -1)) == key:
			return true
	return false


func _init() -> void:
	var plant := {"size": SpeciesVisualMap.SIZE_PLANT, "color": SpeciesVisualMap.COLOR_PLANT, "is_plant": true, "morph": "plant"}
	var rod := {"size": SpeciesVisualMap.SIZE_ROD, "color": SpeciesVisualMap.COLOR_ROD, "is_plant": false, "morph": "rod"}
	var table := {0: plant, 1: rod}

	# ─────────────────── 1. PERF LEVER — O(#colonies), independent of cell count ───────────────────
	var col_n = Colonies.new()
	col_n.set_snapshot(_perf_snap(48, 48), 12.0, table)
	var col_4n = Colonies.new()
	col_4n.set_snapshot(_perf_snap(96, 96), 12.0, table)  # 4× the cells of 48×48
	var dn := col_n.district_count()
	var d4n := col_4n.district_count()
	# The colony-polygon count is bounded by the #connected-regions (1 parent + 3 brushed discs), NOT by the cells.
	_ck(dn == d4n, "Field-scope district count CHANGED with grid size (%d at 48² vs %d at 96²) — NOT O(#colonies)" % [dn, d4n])
	_ck(d4n <= 24, "Field-scope district count %d is not small (tens) — the de-spam regressed" % d4n)
	_ck(d4n == 4, "expected 4 districts (1 parent + 3 brushed discs), got %d" % d4n)
	# O(#colonies) ≪ the old O(cells × MAX_DOTS_PER_CELL) per-organism dot count it replaces.
	var cells_4n := 96 * 96
	var old_dot_count := cells_4n * Organisms.MAX_DOTS_PER_CELL
	_ck(d4n * 200 < old_dot_count, "district count %d not ≪ old O(cells×%d)=%d dots" % [d4n, Organisms.MAX_DOTS_PER_CELL, old_dot_count])
	print("PERF_LEVER_OK districts: 48²(%d cells)=%d  96²(%d cells)=%d  |  old per-organism dots @96²=%d → %.0f× fewer draws" % [
		48 * 48, dn, cells_4n, d4n, old_dot_count, float(old_dot_count) / float(maxi(1, col_4n.draw_entry_count()))])
	col_n.free()
	col_4n.free()

	# ─────────────────── 2. SELECTED-POP CAP HARDENING (map-spanning colony can't re-spam) ───────────────────
	var GW := 96
	var GH := 96
	var org = Organisms.new()
	org.set_snapshot(_solid_snap(GW, GH), 12.0)  # one colony covering ALL 9216 cells
	org.set_selected_colony(0)  # packed key sid 0 * 65536 + var 0 = 0
	# (a) NO viewport clamp (empty rect, the headless case): every cell is a candidate but only SELECTED_POP_BUDGET pop.
	var rep_all: Dictionary = org.selected_pop_report(Rect2())
	_ck(int(rep_all["candidates"]) == GW * GH, "expected %d selected candidates, got %d" % [GW * GH, rep_all["candidates"]])
	_ck(int(rep_all["popped"]) == Organisms.SELECTED_POP_BUDGET, "map-spanning selection popped %d, expected the BUDGET %d (re-spam!)" % [rep_all["popped"], Organisms.SELECTED_POP_BUDGET])
	_ck(int(rep_all["budget_capped"]) == GW * GH - Organisms.SELECTED_POP_BUDGET, "budget_capped %d != cells-budget %d" % [rep_all["budget_capped"], GW * GH - Organisms.SELECTED_POP_BUDGET])
	_ck(int(rep_all["culled_offscreen"]) == 0, "no viewport rect → nothing should be culled, got %d" % rep_all["culled_offscreen"])
	# (b) VIEWPORT CLAMP: a small rect covering only cells (0..4)×(0..4) (centres 6..54 px < 60) → 25 visible cells pop;
	# every OTHER selected cell is CULLED and does NOT consume the budget (budget never spent: 25 < 700).
	var rect := Rect2(0.0, 0.0, 60.0, 60.0)
	var rep_clip: Dictionary = org.selected_pop_report(rect)
	_ck(int(rep_clip["popped"]) == 25, "viewport-clamped pop should be the 25 visible cells, got %d" % rep_clip["popped"])
	_ck(int(rep_clip["budget_capped"]) == 0, "a 25-cell visible window should NOT spend the 700 budget (off-screen cells consumed it?), capped=%d" % rep_clip["budget_capped"])
	_ck(int(rep_clip["culled_offscreen"]) == GW * GH - 25, "off-screen cells not culled: culled=%d expected %d" % [rep_clip["culled_offscreen"], GW * GH - 25])
	_ck(int(rep_clip["popped"]) + int(rep_clip["culled_offscreen"]) == int(rep_clip["candidates"]), "popped+culled != candidates (budget leak)")
	print("CAP_OK map-spanning sel: budget-capped pop=%d/%d (cap %d) | viewport-clamped pop=%d culled=%d (off-screen don't charge budget)" % [
		rep_all["popped"], rep_all["candidates"], Organisms.SELECTED_POP_BUDGET, rep_clip["popped"], rep_clip["culled_offscreen"]])
	org.free()

	# ─────────────────── 3. DISTRICT INSPECT + 4. LABEL DECLUTTER ───────────────────
	var col = Colonies.new()
	col.set_colony_registry({7: {"species": 0, "label": "Wheat", "color": Color(0.78, 0.34, 0.30), "gen_created": 20, "parent": 0}})
	col.set_snapshot(_declutter_snap(), 12.0, table)

	# 4a. Nothing selected: the BIG block A (key 0, count≥6) is labeled; the tiny far microbe island B (key 65536,
	# count 4 < LABEL_MIN_CELLS) is SUPPRESSED; the disc C (key 7) overlaps A's centroid → DE-OVERLAP drops the
	# lower-priority C.
	var plan0 := col._label_plan()
	_ck(_plan_has_key(plan0, 0), "big district A (key 0) not labeled — threshold/declutter dropped a large district")
	_ck(not _plan_has_key(plan0, 65536), "tiny district B (key 65536, count 4) WAS labeled — cell-count threshold not applied")
	_ck(not _plan_has_key(plan0, 7), "overlapping disc C (key 7) WAS labeled beside the bigger A — de-overlap not applied")
	print("DECLUTTER_OK plan0 labels=%d (A in, B suppressed by threshold, C dropped by de-overlap)" % plan0.size())

	# 4b. Select the tiny far island B → it is now labeled (selected districts ALWAYS earn a label, even below threshold).
	col.set_selected_colony(65536)
	var plan_b := col._label_plan()
	_ck(_plan_has_key(plan_b, 65536), "selected tiny district B (key 65536) NOT labeled — selection should force a label")
	print("DECLUTTER_SELECT_OK selected tiny B labeled (labels=%d)" % plan_b.size())

	# 3. Select the brushed disc C (key 7) → its inspect summary carries the registry fields + the live cell-count.
	col.set_selected_colony(7)
	var summary := col.selected_colony_summary()
	_ck(not summary.is_empty(), "selected_colony_summary returned {} for the selected disc (key 7)")
	_ck(int(summary.get("species", -1)) == 0, "summary species != 0 (got %s)" % summary.get("species", -1))
	_ck(int(summary.get("variant", -1)) == 7, "summary variant != 7 (got %s)" % summary.get("variant", -1))
	_ck(int(summary.get("count", 0)) > 0, "summary live cell-count not positive (got %s)" % summary.get("count", 0))
	_ck(str(summary.get("label", "")) == "Wheat", "summary registry label != 'Wheat' (got '%s')" % summary.get("label", ""))
	_ck(int(summary.get("gen_created", -1)) == 20, "summary gen_created != 20 (got %s)" % summary.get("gen_created", -1))
	_ck(int(summary.get("parent", -1)) == 0, "summary parent != 0 (got %s)" % summary.get("parent", -1))
	print("INSPECT_OK summary={species:%d variant:%d label:'%s' count:%d gen:%d parent:%d}" % [
		summary["species"], summary["variant"], summary["label"], summary["count"], summary["gen_created"], summary["parent"]])

	# 4c. With C selected it is forced into the label plan even though it overlaps (and is smaller than) A.
	var plan_c := col._label_plan()
	_ck(_plan_has_key(plan_c, 7), "selected overlapping disc C (key 7) NOT labeled — selection should override de-overlap")
	print("DECLUTTER_SELECT_OVERLAP_OK selected C forced its label past the de-overlap (labels=%d)" % plan_c.size())

	col.free()

	if _fail:
		quit(1)
		return
	print("COLONY_S6_TEST_OK")
	quit(0)
