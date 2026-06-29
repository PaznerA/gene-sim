extends SceneTree
## ADR-029 S4 code-level proof (no display needed). Exercises the brush->colony render surface deterministically:
##   1. HOLE-CUT: a brushed child (variant != 0) nested in a parent territory makes the parent a FRAME — the parent
##      district reports has_holes, and the hole-cut triangulated fill area == outer area MINUS the child hole area.
##   2. HERITABLE PERSISTENCE: moving the disc (the heritable S1 Variant tag follows the organisms) keeps the child
##      district alive and moves its centroid — proving the district tracks its members, not a fixed region.
##   3. REGISTRY: a renderer-side registry entry names + colours the child district (reads as a named family).
##   4. SELECTION: organisms.gd exposes the selected-pop override + its anti-re-spam budget; the packed key matches.
## Run: godot --headless --path godot --script colony_s4_test.gd   (prints COLONY_S4_TEST_OK on success)

const W := 32
const H := 24
const Snapshot := preload("res://snapshot.gd")
const Colonies := preload("res://colonies.gd")
const Organisms := preload("res://organisms.gd")
const SpeciesVisualMap := preload("res://species_visual_map.gd")

var _fail := false

func _ck(cond: bool, msg: String) -> void:
	if not cond:
		printerr("COLONY_S4_TEST_FAIL: ", msg)
		_fail = true

# Build a synthetic GSS6 snapshot: a solid plant territory (sid 0, variant 0) with a brushed disc (variant `cvid`)
# strictly INTERIOR at centre `cc`, radius `cr`. Density/fitness fill the territory; everything else is background.
func _make_snap(cc: Vector2i, cr: int, cvid: int):
	var snap = Snapshot.new()
	snap.width = W
	snap.height = H
	var n := W * H
	var dens := PackedFloat32Array(); dens.resize(n)
	var fit := PackedFloat32Array(); fit.resize(n)
	var spc := PackedFloat32Array(); spc.resize(n)
	var var_p := PackedFloat32Array(); var_p.resize(n)
	for y in H:
		for x in W:
			var i := y * W + x
			var in_terr := x >= 4 and x < 28 and y >= 3 and y < 21
			if in_terr:
				dens[i] = 1.0
				fit[i] = 0.5
				spc[i] = 0.0
				var dx := x - cc.x
				var dy := y - cc.y
				var_p[i] = float(cvid) if (dx * dx + dy * dy <= cr * cr) else 0.0
			else:
				dens[i] = 0.0
	snap.density = dens
	snap.fitness = fit
	snap.dominant_species_id = spc
	snap.dominant_variant_id = var_p
	return snap

func _poly_area(p: PackedVector2Array) -> float:
	var a := 0.0
	var nn := p.size()
	for i in nn:
		var q := p[(i + 1) % nn]
		a += p[i].x * q.y - q.x * p[i].y
	return abs(a) * 0.5

func _district_by_variant(col, vid: int) -> Dictionary:
	for c in col._colony_draw:
		if bool(c.get("haze", false)):
			continue
		if int(c.get("key", -1)) % 65536 == vid:
			return c
	return {}

func _init() -> void:
	var table := {0: {"size": SpeciesVisualMap.SIZE_PLANT, "color": SpeciesVisualMap.COLOR_PLANT, "is_plant": true, "morph": "plant"}}

	# --- 1. HOLE-CUT ---
	var col = Colonies.new()
	col.set_snapshot(_make_snap(Vector2i(16, 12), 4, 7), 12.0, table)
	var parent := _district_by_variant(col, 0)
	var child := _district_by_variant(col, 7)
	_ck(not parent.is_empty(), "parent district (variant 0) missing")
	_ck(not child.is_empty(), "child district (variant 7) missing — brushed disc did not fall out as its own colony")
	_ck(bool(parent.get("has_holes", false)), "parent district has NO holes — the child region was not cut out (hole-cut failed)")
	_ck(not bool(child.get("has_holes", false)), "child district unexpectedly has holes")

	# The hole-cut fill: bridge+triangulate the parent's outer loop minus its hole loop; the filled area must equal
	# outer-minus-hole (i.e. the child region is genuinely cut out of the parent frame).
	var ploops: Array = parent.get("loops", [])
	_ck(ploops.size() >= 2, "parent has < 2 loops (no interior hole loop traced)")
	if ploops.size() >= 2:
		var outer: PackedVector2Array = col._project_loop(ploops[0])
		var holes := [col._project_loop(ploops[1])]
		var ring: PackedVector2Array = col._eliminate_holes(outer, holes)
		var tri := Geometry2D.triangulate_polygon(ring)
		var tri_area := 0.0
		for t in range(0, tri.size(), 3):
			var a = ring[tri[t]]; var b = ring[tri[t + 1]]; var c = ring[tri[t + 2]]
			tri_area += abs((b - a).cross(c - a)) * 0.5
		var outer_area := _poly_area(outer)
		var hole_area := _poly_area(holes[0])
		var expect := outer_area - hole_area
		_ck(tri.size() >= 3, "hole-cut triangulation produced no triangles")
		_ck(absf(tri_area - expect) < expect * 0.02 + 1.0,
			"hole-cut area %.1f != outer-minus-hole %.1f (the child was NOT cut from the parent fill)" % [tri_area, expect])
		_ck(tri_area < outer_area - hole_area * 0.5,
			"hole-cut area not meaningfully smaller than the solid outer (%.1f vs %.1f)" % [tri_area, outer_area])
		print("HOLE_CUT_OK frame=%.1f outer=%.1f hole=%.1f" % [tri_area, outer_area, hole_area])

	# --- 2. HERITABLE PERSISTENCE (the district moves with its organisms / the S1 Variant tag) ---
	var c0 = _district_by_variant(col, 7)
	var cen0: Vector2 = c0.get("centroid", Vector2.ZERO)
	col.set_snapshot(_make_snap(Vector2i(20, 14), 4, 7), 12.0, table)
	var c1 = _district_by_variant(col, 7)
	_ck(not c1.is_empty(), "child district (variant 7) vanished after the disc moved — the heritable tag did not persist")
	var cen1: Vector2 = c1.get("centroid", Vector2.ZERO)
	_ck(cen1.distance_to(cen0) > 2.0, "child district did not MOVE with its members (centroid %s -> %s)" % [cen0, cen1])
	print("PERSIST_OK centroid %s -> %s" % [cen0, cen1])

	# --- 3. REGISTRY (a brushed district reads as a named family) ---
	col.set_colony_registry({7: {"species": 0, "label": "Wheat", "color": Color(0.78, 0.34, 0.30), "gen_created": 20, "parent": 0}})
	col.set_snapshot(_make_snap(Vector2i(16, 12), 4, 7), 12.0, table)
	var creg = _district_by_variant(col, 7)
	_ck(str(creg.get("label", "")).begins_with("Wheat v7"), "registry label not applied to the child district: '%s'" % creg.get("label", ""))
	print("REGISTRY_OK label='%s'" % creg.get("label", ""))

	# --- 4. SELECTION OVERRIDE PATH (organisms.gd) ---
	var org = Organisms.new()
	_ck(org.has_method("set_selected_colony"), "organisms.gd missing set_selected_colony override")
	_ck(org.has_method("set_variant_plane"), "organisms.gd missing set_variant_plane")
	_ck(int(Organisms.SELECTED_POP_BUDGET) > 0, "organisms.gd selected-pop budget not positive (no re-spam cap)")
	org.set_variant_plane(PackedFloat32Array([7.0]))
	org.set_selected_colony(0 * 65536 + 7)  # a cell with sid 0, variant 7 → packed key 7
	_ck(org._selected_key == 7, "selected colony key not stored (%d)" % org._selected_key)
	col.set_selected_colony(7)
	_ck(col._selected_key == 7, "colonies selected key not stored")
	print("SELECTION_OK budget=%d key=%d" % [Organisms.SELECTED_POP_BUDGET, org._selected_key])

	col.free()
	org.free()
	if _fail:
		quit(1)
		return
	print("COLONY_S4_TEST_OK")
	quit(0)
