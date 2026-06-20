extends Node2D
## Selective-edit BRUSH overlay (ADR-011 S-F). Highlights the disc of world cells a region-scoped CRISPR edit
## would cover, in both orthographic and isometric views, following the cursor. Read-only presentation
## (invariant #2): it only VISUALISES the region the player is about to send to LiveSim.apply_edit_region — it
## computes no biology. No class_name (preload convention; resolves under --headless).

var _iso = null  # iso.gd instance (null = orthographic)
var _cell: float = 16.0
var _wdims: Vector2i = Vector2i(32, 32)  # world grid (== the live snapshot grid, 1:1)
var _center: Vector2i = Vector2i(-1, -1)  # hovered world cell; (-1,-1) = inactive (nothing drawn)
var _radius: int = 4


func setup(iso, cell: float, wdims: Vector2i) -> void:
	_iso = iso
	_cell = cell
	_wdims = wdims
	queue_redraw()


func set_brush(center: Vector2i, radius: int) -> void:
	_center = center
	_radius = radius
	queue_redraw()


func clear() -> void:
	_center = Vector2i(-1, -1)
	queue_redraw()


func _draw() -> void:
	if _center.x < 0:
		return
	var fill := Color(0.97, 0.86, 0.32, 0.30)
	var centre_fill := Color(0.99, 0.62, 0.25, 0.45)
	var r2 := _radius * _radius
	for dy in range(-_radius, _radius + 1):
		for dx in range(-_radius, _radius + 1):
			if dx * dx + dy * dy > r2:
				continue  # Euclidean disc, matching sim_core::Region
			var cx := _center.x + dx
			var cy := _center.y + dy
			if cx < 0 or cy < 0 or cx >= _wdims.x or cy >= _wdims.y:
				continue
			var col := centre_fill if (dx == 0 and dy == 0) else fill
			if _iso != null:
				draw_colored_polygon(_iso.diamond_points(float(cx), float(cy), _cell), col)
			else:
				draw_rect(Rect2(Vector2(float(cx) * _cell, float(cy) * _cell), Vector2(_cell, _cell)), col)
