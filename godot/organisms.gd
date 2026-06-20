extends Node2D
## Read-only organism marker layer for the S4.3 ecosystem view.
##
## Draws a small cluster of dots per non-empty snapshot cell: count scales with `density`, hue with
## `allele_freq`, brightness with `fitness`. INVARIANT #2 (STOP THE LINE if violated): this is PURE
## PRESENTATION. It reads the derived per-cell channels the Rust core already computed and never computes
## any genotype→phenotype biology. The intra-cell dot scatter is deterministic hash jitter for visual
## flavour only — it is NOT a spatial model (the core owns all placement; see sim-core::snapshot).

const MAX_DOTS_PER_CELL := 5

var _w: int = 0
var _h: int = 0
var _cell: float = 12.0
var _density: PackedFloat32Array
var _allele: PackedFloat32Array
var _fitness: PackedFloat32Array


## Point the layer at a parsed snapshot (snapshot.gd instance) and a cell size in pixels, then redraw.
func set_snapshot(snap, cell: float) -> void:
	_w = snap.width
	_h = snap.height
	_cell = cell
	_density = snap.density
	_allele = snap.allele_freq
	_fitness = snap.fitness
	queue_redraw()


func _draw() -> void:
	if _w == 0 or _h == 0:
		return
	var base_r := maxf(1.5, _cell * 0.16)
	var rim := Color(0.03, 0.05, 0.04, 0.92)
	for y in _h:
		for x in _w:
			var i := y * _w + x
			var d := _density[i]
			if d <= 0.0:
				continue
			var dots := int(ceil(d * float(MAX_DOTS_PER_CELL)))
			var fit := clampf(_fitness[i], 0.0, 1.0)
			var col := _organism_color(_allele[i], fit)
			# fitter cells render slightly larger markers (extra read on the data).
			var radius := base_r * (0.82 + 0.5 * fit)
			var base := Vector2(float(x) * _cell, float(y) * _cell)
			for k in dots:
				var off := Vector2(_hash01(x, y, k * 2), _hash01(x, y, k * 2 + 1))
				# inset a touch so dots stay inside the cell
				var p := base + (Vector2.ONE * 0.15 + off * 0.7) * _cell
				draw_circle(p, radius + 1.2, rim)  # dark rim so dots read on grass
				draw_circle(p, radius, col)  # body, coloured by genetics
				draw_circle(p - Vector2(radius, radius) * 0.32, radius * 0.34, Color(1, 1, 1, 0.7))  # specular core


## allele_freq → hue (cyan→blue→magenta→red, off the grass green), fitness → brightness/saturation.
## Presentation mapping only (no biology).
func _organism_color(allele: float, fitness: float) -> Color:
	var hue := 0.52 - 0.52 * clampf(allele, 0.0, 1.0)  # 0.52 cyan … 0.0 red
	var sat := 0.6 + 0.3 * fitness
	var val := 0.6 + 0.4 * fitness
	return Color.from_hsv(hue, sat, val, 0.97)


## Deterministic [0,1) hash for intra-cell dot jitter (visual scatter only).
func _hash01(x: int, y: int, k: int) -> float:
	var h := (x * 73856093) ^ (y * 19349663) ^ ((k + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
