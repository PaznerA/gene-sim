extends Control
## Tiny deterministic trend plot for the live Vitals panel (S3/P8). Plots one or two rolling [0,1] series
## (e.g. mean fitness + allele frequency) the Rust core exported, as a filled line + a faint second line.
##
## INVARIANT #2: read-only presentation — it plots recorded numbers, computes NO biology. INVARIANT #3: a
## deterministic plot of recorded data, no RNG. NO class_name (preload convention; resolves under --headless).

var _series: Array = []  # primary series, values in [0,1], newest last
var _series2: Array = []  # optional secondary series, values in [0,1]


## Point the plot at rolling [0,1] series and redraw. series2 is optional (drawn faint).
func set_series(series: Array, series2: Array = []) -> void:
	_series = series
	_series2 = series2
	queue_redraw()


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.0, 0.0, 0.0, 0.30))
	_plot(_series2, Color(0.96, 0.86, 0.3, 0.6), false)  # secondary behind
	_plot(_series, Color(0.42, 0.92, 0.5, 0.95), true)


func _plot(series: Array, col: Color, fill: bool) -> void:
	if series.size() < 2:
		return
	var pts := PackedVector2Array()
	var n := series.size()
	for i in n:
		var x := size.x * float(i) / float(n - 1)
		var y := size.y - size.y * clampf(float(series[i]), 0.0, 1.0)
		pts.append(Vector2(x, clampf(y, 1.0, size.y - 1.0)))
	if fill:
		var area := pts.duplicate()
		area.append(Vector2(size.x, size.y))
		area.append(Vector2(0.0, size.y))
		draw_colored_polygon(area, Color(col.r, col.g, col.b, 0.18))
	draw_polyline(pts, col, 1.5 if fill else 1.0, true)
