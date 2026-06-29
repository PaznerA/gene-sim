extends SceneTree
## ADR-029 S5 code-level proof (no display needed). Exercises the PLANT-REALISM render surface deterministically:
##   1. ALWAYS-VISIBLE FLOOR: a sub-MIN_COLONY_CELLS PLANT colony renders as a DISTRICT (haze:false), while a
##      sub-MIN_COLONY_CELLS MICROBE colony stays a haze speck (microbe behaviour unchanged).
##   2. CANOPY HULL vs HARD DISTRICT: a plant district carries is_plant=true + a softer (extra-Chaikin) hull with
##      strictly MORE contour points than an identically-shaped microbe hard district (the morph-aware branch).
##   3. >=1-COLONY GUARANTEE: every non-empty PLANT cell lands in a colony (_labels[i] != background).
##   4. GHOST FLOOR: the plant ghost-fill floor PLANT_GHOST_FILL_FACTOR > the microbe GHOST_FILL_FACTOR.
## Run: godot --headless --path godot --script colony_s5_test.gd   (prints COLONY_S5_TEST_OK on success)

const W := 32
const H := 24
const Snapshot := preload("res://snapshot.gd")
const Colonies := preload("res://colonies.gd")
const SpeciesVisualMap := preload("res://species_visual_map.gd")

var _fail := false

func _ck(cond: bool, msg: String) -> void:
	if not cond:
		printerr("COLONY_S5_TEST_FAIL: ", msg)
		_fail = true

# Build a GSS6 snapshot: a 6x6 PLANT block (sid 0) + an identically-sized 6x6 MICROBE block (sid 1), plus a
# single-cell PLANT island and a single-cell MICROBE island (both 1 cell, < MIN_COLONY_CELLS). All variant 0,
# so each (species, contiguous region) falls out as its own connected-component colony.
func _make_snap():
	var snap = Snapshot.new()
	snap.width = W
	snap.height = H
	var n := W * H
	var dens := PackedFloat32Array(); dens.resize(n)
	var fit := PackedFloat32Array(); fit.resize(n)
	var spc := PackedFloat32Array(); spc.resize(n)
	var var_p := PackedFloat32Array(); var_p.resize(n)  # all variant 0
	# 6x6 plant block (sid 0) at minx 2
	for y in range(2, 8):
		for x in range(2, 8):
			var i := y * W + x
			dens[i] = 1.0; fit[i] = 0.6; spc[i] = 0.0
	# 6x6 microbe block (sid 1) at minx 12 (same shape as the plant block → equal simplified contour)
	for y in range(2, 8):
		for x in range(12, 18):
			var i := y * W + x
			dens[i] = 1.0; fit[i] = 0.6; spc[i] = 1.0
	# tiny PLANT island (1 cell, sid 0) at (25,20) — isolated → its own sub-MIN colony
	var pi := 20 * W + 25
	dens[pi] = 1.0; fit[pi] = 0.5; spc[pi] = 0.0
	# tiny MICROBE island (1 cell, sid 1) at (29,20) — isolated → its own sub-MIN colony
	var mi := 20 * W + 29
	dens[mi] = 1.0; fit[mi] = 0.5; spc[mi] = 1.0
	snap.density = dens
	snap.fitness = fit
	snap.dominant_species_id = spc
	snap.dominant_variant_id = var_p
	return snap

# Find the non-haze district for species `sid` whose bbox left edge == `minx` (distinguishes the big block from
# the tiny island, both sid 0). key = sid*VARIANT_KEY_BASE + variant; here every variant is 0.
func _district(col, sid: int, minx: int) -> Dictionary:
	for c in col._colony_draw:
		if bool(c.get("haze", false)):
			continue
		if int(c.get("key", -1)) / 65536 == sid and int(c.get("minx", -999)) == minx:
			return c
	return {}

func _init() -> void:
	var table := {
		0: {"size": SpeciesVisualMap.SIZE_PLANT, "color": SpeciesVisualMap.COLOR_PLANT, "is_plant": true, "morph": "plant"},
		1: {"size": SpeciesVisualMap.SIZE_ROD, "color": SpeciesVisualMap.COLOR_ROD, "is_plant": false, "morph": "rod"},
	}
	var col = Colonies.new()
	var snap = _make_snap()
	col.set_snapshot(snap, 12.0, table)

	# --- 1. ALWAYS-VISIBLE FLOOR (plant: never haze; microbe: still haze) ---
	var tiny_plant := _district(col, 0, 25)
	_ck(not tiny_plant.is_empty(), "tiny (1-cell) plant colony missing — it did NOT render as a district (always-visible floor broken)")
	_ck(not bool(tiny_plant.get("haze", false)), "tiny plant colony rendered as a HAZE speck, not a district")
	_ck(bool(tiny_plant.get("is_plant", false)), "tiny plant district not flagged is_plant")
	# The tiny MICROBE colony must STILL be a haze speck — microbe behaviour unchanged. With the tiny plant now a
	# district, the ONLY haze entry left is the 1-cell microbe island.
	var hazes := 0
	var tiny_microbe_haze := false
	for c in col._colony_draw:
		if bool(c.get("haze", false)):
			hazes += 1
			if int(c.get("count", -1)) == 1:
				tiny_microbe_haze = true
	_ck(tiny_microbe_haze, "tiny microbe colony did NOT stay a haze speck (microbe behaviour changed)")
	_ck(hazes == 1, "expected exactly 1 haze speck (the tiny microbe), got %d" % hazes)
	print("FLOOR_OK tiny_plant=district tiny_microbe=haze hazes=%d" % hazes)

	# --- 2. CANOPY HULL vs HARD DISTRICT (morph-aware branch) ---
	var big_plant := _district(col, 0, 2)
	var big_microbe := _district(col, 1, 12)
	_ck(not big_plant.is_empty(), "big plant district (sid 0) missing")
	_ck(not big_microbe.is_empty(), "big microbe district (sid 1) missing")
	_ck(bool(big_plant.get("is_plant", false)), "big plant district not flagged is_plant")
	_ck(not bool(big_microbe.get("is_plant", true)), "microbe district wrongly flagged is_plant")
	var pp: int = (big_plant.get("points", PackedVector2Array()) as PackedVector2Array).size()
	var mp: int = (big_microbe.get("points", PackedVector2Array()) as PackedVector2Array).size()
	# Same 6x6 shape → same simplified loop; the plant gets PLANT_CHAIKIN_PASSES (>1) vs the microbe's single pass,
	# so the canopy hull is strictly rounder (more contour points). Proves the soft-hull-vs-hard-district branch.
	_ck(pp > mp, "plant canopy hull (%d pts) not softer/rounder than the microbe hard district (%d pts) — extra Chaikin missing" % [pp, mp])
	print("CANOPY_OK plant_pts=%d microbe_pts=%d" % [pp, mp])

	# --- 3. >=1-COLONY GUARANTEE (every non-empty plant cell lands in a colony) ---
	var labels: PackedInt32Array = col._labels
	_ck(labels.size() == W * H, "label image not built (size %d, expected %d)" % [labels.size(), W * H])
	var dens: PackedFloat32Array = snap.density
	var spc: PackedFloat32Array = snap.dominant_species_id
	var plant_cells := 0
	var unlabeled := 0
	if labels.size() == W * H:
		for i in W * H:
			if dens[i] > 0.0 and int(round(spc[i])) == 0:  # a non-empty PLANT cell
				plant_cells += 1
				if labels[i] < 0:
					unlabeled += 1
	_ck(plant_cells > 0, "test built no plant cells")
	_ck(unlabeled == 0, "%d non-empty plant cells have NO colony label (>=1-colony guarantee broken)" % unlabeled)
	print("COLONY_GUARANTEE_OK plant_cells=%d unlabeled=%d" % [plant_cells, unlabeled])

	# --- 4. GHOST FLOOR (plant ghost fill floors ABOVE the microbe ghost) ---
	_ck(Colonies.PLANT_GHOST_FILL_FACTOR > Colonies.GHOST_FILL_FACTOR,
		"plant ghost-fill floor %.3f is NOT > microbe GHOST_FILL_FACTOR %.3f" % [Colonies.PLANT_GHOST_FILL_FACTOR, Colonies.GHOST_FILL_FACTOR])
	print("GHOST_FLOOR_OK plant=%.2f microbe=%.2f" % [Colonies.PLANT_GHOST_FILL_FACTOR, Colonies.GHOST_FILL_FACTOR])

	col.free()
	if _fail:
		quit(1)
		return
	print("COLONY_S5_TEST_OK")
	quit(0)
