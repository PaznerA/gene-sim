extends Node2D
## Isometric ground + data overlay (P3). Draws the field as depth-sorted CPU diamonds via iso.gd:
## a hash-based grass backdrop, optionally tinted by a snapshot channel (the data overlay, iso-native).
##
## INVARIANT #2: read-only presentation. The grass shade is a deterministic hash backdrop (no data); the
## tint only VISUALISES a per-cell channel the Rust core already exported. No biology here.

const SHADES := [
	Color(0.20, 0.36, 0.18), Color(0.24, 0.42, 0.21),
	Color(0.18, 0.32, 0.17), Color(0.27, 0.46, 0.24),
]
const TILE_H := 0.22  # min block depth as a fraction of cell
const HEIGHT_MAX := 0.7  # max terrain lift as a fraction of cell (the rolling-hills relief)

var _w: int = 0
var _h: int = 0
var _cell: float = 16.0
var _iso = null  # iso.gd instance
var _snap = null
var _overlay_mode: int = 0  # 0 off · 1 density · 2 allele · 3 fitness · 4 moisture · 5 nutrients · 6 ph
# · 7 light · 8 free_nutrient · 9 detritus (the GSS3 live-pool joule-economy planes, full fields like soil)
# · 10 toxin · 11 kin · 12 alarm (the GSS4 chem planes, ADR-013 F5: allelopathy/kin/chemotaxis)


func setup(w: int, h: int, cell: float, iso) -> void:
	_w = w
	_h = h
	_cell = cell
	_iso = iso
	queue_redraw()


func set_snapshot(snap, overlay_mode: int) -> void:
	_snap = snap
	_overlay_mode = overlay_mode
	queue_redraw()


func _draw() -> void:
	if _iso == null or _w == 0 or _h == 0:
		return
	# Walk cells back-to-front (ascending depth_key = cx+cy) so nearer diamonds overdraw farther ones.
	var cells: Array = []
	for y in _h:
		for x in _w:
			cells.append([_iso.depth_key(x, y), x, y])
	cells.sort_custom(func(a, b): return a[0] < b[0])

	var has_data := _snap != null and _overlay_mode != 0
	for c in cells:
		var x: int = c[1]
		var y: int = c[2]
		var col := _grass(x, y)
		if has_data:
			var v := _channel(x, y)
			# Soil (4-6) + pool (7-9) channels are full fields → always tint; population channels only where populated.
			if _overlay_mode >= 4 or v > 0.0:
				col = col.lerp(_inferno(v), 0.62)
		# 3D heightfield tile: the top diamond is RAISED by the cell's terrain height; the two darker side
		# faces (left/right) drop from the raised top to a common base, so neighbouring heights show relief.
		# Back-to-front (cx+cy) order makes nearer/taller tiles overdraw farther ones.
		var lift: float = _cell * HEIGHT_MAX * _iso.terrain_height(x, y)
		var up := Vector2(0.0, -lift)
		var skirt := Vector2(0.0, lift + _cell * TILE_H)  # raised top → common base (+ a min block depth)
		var d: PackedVector2Array = _iso.diamond_points(x, y, _cell)  # [top, right, bottom, left]
		var tb := d[2] + up  # raised bottom vertex
		var tl := d[3] + up  # raised left
		var tr := d[1] + up  # raised right
		draw_colored_polygon(PackedVector2Array([tl, tb, tb + skirt, tl + skirt]), col.darkened(0.42))
		draw_colored_polygon(PackedVector2Array([tb, tr, tr + skirt, tb + skirt]), col.darkened(0.60))
		draw_colored_polygon(
			PackedVector2Array([d[0] + up, tr, tb, tl]), col)  # raised top diamond


func _channel(x: int, y: int) -> float:
	var i: int = y * _snap.width + x
	match _overlay_mode:
		1: return clampf(_snap.density[i], 0.0, 1.0)
		2: return clampf(_snap.allele_freq[i], 0.0, 1.0)
		3: return clampf(_snap.fitness[i], 0.0, 1.0)
		4: return clampf(_snap.soil_moisture[i], 0.0, 1.0)
		5: return clampf(_snap.soil_nutrients[i], 0.0, 1.0)
		6: return clampf(_snap.soil_ph[i], 0.0, 1.0)
		7: return clampf(_snap.light[i], 0.0, 1.0)
		8: return clampf(_snap.free_nutrient[i], 0.0, 1.0)
		9: return clampf(_snap.detritus[i], 0.0, 1.0)
		10: return clampf(_snap.toxin[i], 0.0, 1.0)  # GSS4 chem (ADR-013 F5)
		11: return clampf(_snap.kin[i], 0.0, 1.0)
		_: return clampf(_snap.alarm[i], 0.0, 1.0)  # mode 12 alarm


func _grass(x: int, y: int) -> Color:
	var n := SHADES.size()
	var ti := int(_hash01(int(x / 3.0), int(y / 3.0), 7) * float(n)) % n
	if _hash01(x, y, 11) > 0.88:
		ti = int(_hash01(x, y, 13) * float(n)) % n
	return SHADES[ti]


## inferno ramp mirrored from main.gd / data_layer.gdshader (legend stays consistent).
func _inferno(t: float) -> Color:
	var c0 := Color(0.05, 0.03, 0.15)
	var c1 := Color(0.35, 0.07, 0.43)
	var c2 := Color(0.75, 0.18, 0.33)
	var c3 := Color(0.96, 0.49, 0.14)
	var c4 := Color(0.99, 0.92, 0.55)
	if t < 0.25:
		return c0.lerp(c1, t / 0.25)
	elif t < 0.5:
		return c1.lerp(c2, (t - 0.25) / 0.25)
	elif t < 0.75:
		return c2.lerp(c3, (t - 0.5) / 0.25)
	return c3.lerp(c4, (t - 0.75) / 0.25)


func _hash01(a: int, b: int, c: int) -> float:
	var h := (a * 73856093) ^ ((b + 1) * 19349663) ^ ((c + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
