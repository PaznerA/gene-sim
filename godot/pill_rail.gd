extends Control
## A horizontal rail of labelled PILLS for minimized panels, docked ABOVE the timeline (inv #2: presentation).
##
## Each minimized panel.gd registers a pill here (a small rounded Button with the panel's title). Clicking the
## pill calls panel.restore() (tween back out). Loaded by path (no class_name; ADR-010 rule).
##   const PillRail := preload("res://pill_rail.gd")
##
## Lives on the same layer-2 CanvasLayer as the panels. Anchored BOTTOM_WIDE just above the timeline strip
## (timeline sits at offset_top -54 / offset_bottom -6; the rail sits at offset_top -84 / offset_bottom -58).

const PILL_W := 132.0
const PILL_H := 26.0
const GAP := 8.0

var _row: HBoxContainer = null
var _pills: Dictionary = {}  # panel(Object) -> Button. Keyed by the panel node, iterated by insertion order
var _order: Array = []       # ordered list of panels (NEVER iterate _pills directly — inv #3 hygiene in UI too)


func setup(ui: CanvasLayer) -> void:
	set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	offset_left = 8
	offset_right = -8
	offset_top = -84   # 30px band sitting just above the timeline (-54)
	offset_bottom = -58
	mouse_filter = Control.MOUSE_FILTER_IGNORE  # empty rail never eats clicks; pills (children) STOP on their own
	_row = HBoxContainer.new()
	_row.add_theme_constant_override("separation", int(GAP))
	_row.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_row)
	ui.add_child(self)


## Where the NEXT minimized panel should fly to (global position of its incoming pill slot). Called by
## panel.minimize() BEFORE the pill exists, so compute from the current pill count.
func reserve_slot() -> Vector2:
	var idx := _order.size()
	var local := Vector2(float(idx) * (PILL_W + GAP), 0.0)
	return global_position + local


## Spawn the labelled pill for a now-minimized panel. Clicking it restores the panel.
func add_pill(panel: Object, label: String) -> void:
	if _pills.has(panel):
		return
	var pill := Button.new()
	pill.text = "▢ " + label  # restore glyph + the panel's title
	pill.custom_minimum_size = Vector2(PILL_W, PILL_H)
	pill.clip_text = true
	pill.focus_mode = Control.FOCUS_NONE
	pill.tooltip_text = "Restore " + label
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.10, 0.16, 0.13, 0.92)
	sb.set_corner_radius_all(13)  # full pill rounding
	sb.set_content_margin_all(4)
	sb.border_width_bottom = 2
	sb.border_color = Color(0.2, 0.5, 0.32, 0.7)
	var hover := sb.duplicate()
	hover.bg_color = Color(0.16, 0.24, 0.18, 0.96)
	pill.add_theme_stylebox_override("normal", sb)
	pill.add_theme_stylebox_override("hover", hover)
	pill.add_theme_stylebox_override("pressed", hover)
	pill.add_theme_color_override("font_color", Color(0.85, 0.93, 0.85))
	pill.add_theme_font_size_override("font_size", 12)
	pill.pressed.connect(_on_pill_pressed.bind(panel))
	_row.add_child(pill)
	_pills[panel] = pill
	_order.append(panel)


## Remove the pill for a restored panel and reflow the row (HBox reflows automatically on child removal).
func remove_pill(panel: Object) -> void:
	if not _pills.has(panel):
		return
	var pill := _pills[panel] as Button
	_row.remove_child(pill)
	pill.queue_free()
	_pills.erase(panel)
	_order.erase(panel)


func _on_pill_pressed(panel: Object) -> void:
	if panel != null and panel.has_method("restore"):
		panel.restore()
