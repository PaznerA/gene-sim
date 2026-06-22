extends PanelContainer
## Reusable DRAGGABLE + MINIMIZABLE panel chrome for the gene-sim HUD (inv #2: presentation only — no biology).
##
## This wraps an existing content Control in a titled, draggable, minimizable frame. The wrapper IS the field
## reference main.gd keeps (e.g. `_vitals_panel`), so every existing `.visible = ...` toggle still hides chrome
## + content together and `--check` build correctness is unchanged. Drag + minimize are driven by _gui_input /
## tween, which `godot --check` does NOT run — so they are interaction-only, not gate-checked (by design).
##
## Loaded by PATH, not class_name (the class_name registry is empty under a fresh --headless run; ADR-010 rule):
##   const Panel := preload("res://panel.gd")
##
## Usage (main.gd builder):
##   var body := _dark_panel(0.74)            # the OLD PanelContainer becomes the CONTENT
##   ... fill body with the VBox/labels as before ...
##   _vitals_panel = Panel.new()
##   _vitals_panel.setup("VITALS", body, ui, Vector2(12, 46), _pill_rail)
##   # _vitals_panel.visible toggles still work verbatim.

const TITLE_H := 22.0
const PILL_W := 132.0
const PILL_H := 26.0
const ANIM := 0.18  # tween seconds, both directions

signal minimized(panel)   # emitted when this panel collapses (pill rail listens to spawn a pill)
signal restored(panel)    # emitted when it pops back out (pill rail removes its pill)

var title_text: String = "Panel"
var content: Control = null            # the wrapped body (the old _dark_panel / PanelContainer)
var default_dock: Vector2 = Vector2.ZERO
var pill_rail: Control = null          # the PillRail this panel reports to (set in setup)

var _minimized: bool = false
var _active: bool = true  # the caller's "should be shown when not minimized" intent (set via set_active)
var _restore_pos: Vector2 = Vector2.ZERO  # where to fly back to on restore
var _dragging: bool = false
var _drag_offset: Vector2 = Vector2.ZERO
var _titlebar: HBoxContainer = null
var _title_label: Label = null         # the title bar's text label (so the caller can swap the title live)
var _body_holder: MarginContainer = null
var _tween: Tween = null


## Build the chrome and reparent `body` inside it, then dock at `dock`. Adds itself to `ui` (the layer-2
## CanvasLayer). `rail` is the PillRail above the timeline (may be null; minimize just hides if so).
## Explicit-typed throughout (':=' can't infer a Variant from the untyped callers in main.gd).
func setup(p_title: String, body: Control, ui: CanvasLayer, dock: Vector2, rail: Control = null) -> void:
	title_text = p_title
	content = body
	default_dock = dock
	pill_rail = rail

	# This wrapper's own frame: a subtle rounded card so panels read as one consistent deck.
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.05, 0.07, 0.06, 0.0)  # transparent — the body keeps its own bg; chrome is the border
	sb.set_corner_radius_all(7)
	sb.set_content_margin_all(0)
	add_theme_stylebox_override("panel", sb)
	mouse_filter = Control.MOUSE_FILTER_PASS

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 0)
	add_child(col)

	col.add_child(_make_titlebar())

	# Reparent the existing body unchanged. If it already has a parent (built standalone), detach first.
	if body.get_parent() != null:
		body.get_parent().remove_child(body)
	_body_holder = MarginContainer.new()
	_body_holder.add_child(body)
	col.add_child(_body_holder)

	ui.add_child(self)
	position = dock


## Swap the title bar's text live (e.g. "🌱 SPECIMEN" ↔ "🦠 SPECIMEN" per the focused specimen's species).
func set_title(p_title: String) -> void:
	title_text = p_title
	if _title_label != null:
		_title_label.text = p_title


## The title bar: a drag handle glyph + the title label + a minimize button. Draggable via _gui_input on the
## HBox itself (the bar, not the whole panel — so buttons/sliders inside the body stay interactive).
func _make_titlebar() -> PanelContainer:
	_titlebar = HBoxContainer.new()
	_titlebar.custom_minimum_size = Vector2(0, TITLE_H)
	_titlebar.add_theme_constant_override("separation", 6)
	# PASS (not STOP) so a click on the header content falls THROUGH to the backing bar's gui_input — otherwise
	# the HBox swallows the event and drag never fires. The whole header is the grab area (the user's ask).
	_titlebar.mouse_filter = Control.MOUSE_FILTER_PASS
	var bsb := StyleBoxFlat.new()
	bsb.bg_color = Color(0.12, 0.17, 0.14, 0.95)
	bsb.corner_radius_top_left = 7
	bsb.corner_radius_top_right = 7
	bsb.content_margin_left = 8
	bsb.content_margin_right = 5
	bsb.content_margin_top = 2
	bsb.content_margin_bottom = 2
	bsb.border_width_bottom = 1
	bsb.border_color = Color(0.2, 0.45, 0.3, 0.6)
	# HBoxContainer doesn't draw a stylebox, so the bg + drag-input live on a backing PanelContainer that spans
	# the WHOLE header — grab anywhere on the title bar (the per-panel icon + name) to drag.
	var bar := PanelContainer.new()
	bar.add_theme_stylebox_override("panel", bsb)
	bar.mouse_filter = Control.MOUSE_FILTER_STOP
	bar.mouse_default_cursor_shape = Control.CURSOR_MOVE  # the header reads as draggable
	bar.gui_input.connect(_on_titlebar_input)

	# Title = the per-panel unique icon + name (passed in by the caller, e.g. "📊 VITALS"). No generic handle.
	var title := Label.new()
	title.text = title_text
	title.add_theme_font_size_override("font_size", 13)
	title.add_theme_color_override("font_color", Color(0.88, 0.95, 0.88))
	title.mouse_filter = Control.MOUSE_FILTER_PASS
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_title_label = title
	_titlebar.add_child(title)

	var mini := Button.new()
	mini.text = "—"  # minimize glyph
	mini.flat = true
	mini.focus_mode = Control.FOCUS_NONE
	mini.custom_minimum_size = Vector2(22, 18)
	mini.tooltip_text = "Minimize to a pill above the timeline"
	mini.add_theme_font_size_override("font_size", 14)
	mini.pressed.connect(minimize)
	_titlebar.add_child(mini)

	bar.add_child(_titlebar)
	return bar


## Drag the panel by its title bar. _gui_input is NOT run under `godot --check` (no _gui_input/_process/_draw),
## so this is exercised only in a live window — build correctness is still fully gate-checkable.
func _on_titlebar_input(ev: InputEvent) -> void:
	if ev is InputEventMouseButton and ev.button_index == MOUSE_BUTTON_LEFT:
		var mb := ev as InputEventMouseButton
		if mb.pressed:
			_dragging = true
			_drag_offset = get_global_mouse_position() - position
			move_to_front()  # raise above sibling panels in the layer while dragging
		else:
			_dragging = false
	elif ev is InputEventMouseMotion and _dragging:
		var target := get_global_mouse_position() - _drag_offset
		# Clamp into the viewport so a panel can't be lost off-screen.
		var vp := get_viewport_rect().size
		target.x = clampf(target.x, 0.0, maxf(0.0, vp.x - size.x))
		target.y = clampf(target.y, 0.0, maxf(0.0, vp.y - size.y))
		position = target


## Collapse to a pill: remember the current position, tween the panel into the rail slot, then hide + spawn
## the labelled pill. main.gd's visibility toggles see a still-present node; while minimized we keep `visible`
## owned by the rail (restore() shows it again).
## main.gd's visibility rules route through here (NOT a raw `.visible =`) so an external "show" can't resurrect
## a MINIMIZED panel (while minimized the rail owns it). `_active` = the caller's shown-when-not-minimized intent.
func set_active(on: bool) -> void:
	_active = on
	if not _minimized:
		visible = on


func minimize() -> void:
	if _minimized:
		return
	# If a restore tween is still mid-flight, settle to its target first so we sample a STABLE dock position,
	# not a value moving between the pill slot and the dock (fixes rapid restore→minimize corrupting _restore_pos).
	if _tween != null and _tween.is_valid():
		position = _restore_pos
	_minimized = true
	_restore_pos = position
	var slot: Vector2 = pill_rail.reserve_slot() if pill_rail != null else position
	_kill_tween()
	_tween = create_tween()
	_tween.set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_IN)
	_tween.tween_property(self, "position", slot, ANIM)
	_tween.parallel().tween_property(self, "modulate:a", 0.0, ANIM)
	_tween.tween_callback(_after_minimize)
	minimized.emit(self)


func _after_minimize() -> void:
	visible = false
	modulate.a = 1.0
	if pill_rail != null:
		pill_rail.add_pill(self, title_text)


## Pop back out from the pill: show, then tween from the rail slot back to where it was minimized.
func restore() -> void:
	if not _minimized:
		return
	_minimized = false
	if pill_rail != null:
		pill_rail.remove_pill(self)
	if not _active:
		# The caller hid this panel (view switch) while it was parked — snap home hidden, no pop-out tween.
		position = _restore_pos
		visible = false
		restored.emit(self)
		return
	visible = true
	modulate.a = 0.0
	move_to_front()
	_kill_tween()
	_tween = create_tween()
	_tween.set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	_tween.tween_property(self, "position", _restore_pos, ANIM)
	_tween.parallel().tween_property(self, "modulate:a", 1.0, ANIM)
	restored.emit(self)


func is_minimized() -> bool:
	return _minimized


## Re-dock to the default position (used on window resize / view switch resets).
func reset_dock() -> void:
	position = default_dock
	_restore_pos = default_dock


func _kill_tween() -> void:
	if _tween != null and _tween.is_valid():
		_tween.kill()
	_tween = null
