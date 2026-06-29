extends Node2D
## gene-sim thin UI entry point — 2D ecosystem view of one scope (S4.3).
##
## INVARIANT #2 (STOP THE LINE if violated): this renderer READS sim snapshots only. It must NEVER compute
## genotype→phenotype or any biology — all of that lives in the Rust core (crates/genome, crates/sim-core).
## GDScript here only loads/plays snapshot data and draws it: a tiled field backdrop, organism dot markers,
## and toggleable per-cell data-layer SHADERS (density/allele_freq/fitness) over a zoomable viewport (S4.4).
##
## CLI (args after `--`):
##   --snap <file.bin>     Headless: parse one snapshot and report its header (S4.2 gate path).
##   --run  <dir>          Play snap_*.bin in <dir> as a live run (windowed; auto-advances, loops).
##   --shot <out.png>      Render one frame to a PNG then quit (needs a display; for verification).
##   --gen  <n>            With --run/--shot: pick the snapshot whose generation == n (else the last).
##   --layer <0..9>        With --shot: preselect the data layer (0 off / 1 density / 2 allele / 3 fitness /
##                         4 soil_moisture / 5 soil_nutrients / 6 soil_ph / 7 light / 8 free_nutrient / 9 detritus).
##   --zoom  <f>           With --shot: preset the zoom scope (1 field … 6 cells).
##   --ortho               Render the ecosystem orthographically (flat); ISOMETRIC (CPU diamonds) is the default.
##   --live [--seed N]     Drive an OPEN-ENDED SANDBOX run live via the LiveSim gdext node (build the cdylib
##                         cargo build --manifest-path crates/godot-sim/Cargo.toml). Space pauses, ▶ steps.
##   --view specimen       Open the L-system specimen view (instead of the ecosystem view) for --shot.
##   --view relations      Open the Relations FlowMatrix heatmap view for --shot.
##   --view codex          Open the browsable CODEX encyclopedia view for --shot.
##   --focus <i>           With --view specimen: focus specimen i (0 baseline, 1.. edits) for --shot.
## With no args and a display, auto-discovers the newest data/runs/<id>/ that holds snap_*.bin.
##
## Keys (windowed): Space pause · V cycle ecosystem→specimen→relations→codex · Tab cycle specimen · D cycle layer ·
##   S toggle plant sprites/dots · B toggle selective edit brush (live) · [ / ] brush radius ·
##   ,/. step · 1/2/3 zoom scope · wheel zoom (brush: wheel = radius) · arrows pan.
## Brush (live, ADR-011 / SP-3.6): with B on, hover paints a disc on the map; click/drag applies the ACTIVE
##   intervention tool to ONLY that region — CRISPR (apply_edit_region) / PCR (pcr_amplify) / Antibiotic (cull) /
##   Nutrient (nutrient) / Toxin (toxin) / Inoculate (inoculate — seed a contaminant, ADR-019). The disc COLOUR
##   signals the active tool; POSITION MATTERS (the brush
##   cell → RegionSpec → Region::contains in the core picks the orgs/cells). Biology stays in the core (inv #2).
## Mouse (windowed): drag = pan · hover = cell/plant tooltip · click = pin detail (cell stats + genome
##   ontology in ecosystem; focus + detail a plant in specimen).

## Load the reader by path, not via a `class_name` global: that registry is only populated by an editor
## import pass, so a bare identifier is unresolved under a fresh `--headless` run. `preload` needs no cache.
const SnapshotReader := preload("res://snapshot.gd")
const Organisms := preload("res://organisms.gd")
const Colonies := preload("res://colonies.gd")  # ADR-029 S2: Field-scope district-polygon de-spam layer
const Lsystem := preload("res://lsystem.gd")
const Microbe := preload("res://microbe.gd")
const Mold := preload("res://mold.gd")
const GlyphFactory := preload("res://glyph_factory.gd")
const Codex := preload("res://codex.gd")
const Timeline := preload("res://timeline.gd")
const Iso := preload("res://iso.gd")
const IsoGround := preload("res://iso_ground.gd")
const Sparkline := preload("res://sparkline.gd")
const RelationsHeatmap := preload("res://relations_heatmap.gd")
const RelationsGraph := preload("res://relations_graph.gd")  # Item 4: node-link trophic-web graph (companion view)
const Brush := preload("res://brush.gd")
const PanelChrome := preload("res://panel.gd")
const PillRail := preload("res://pill_rail.gd")
const MainMenu := preload("res://main_menu.gd")
const Gallery := preload("res://gallery.gd")  # STARTERS scenario picker (gen-1 fresh run / gen-N checkpoint load)
const DataLayerShader := preload("res://data_layer.gdshader")
const SpeciesVisualMap := preload("res://species_visual_map.gd")

const OVERLAY_NAMES := ["off", "density", "allele_freq", "fitness", "soil_moisture", "soil_nutrients", "soil_ph",
	"light", "free_nutrient", "detritus",  # GSS3 live-pool joule-economy planes appended after soil_ph
	"toxin", "kin", "alarm"]  # GSS4 chem planes (ADR-013 F5: allelopathy/kin/chemotaxis) appended after detritus
# Optional per-channel legend captions (the joule economy made readable at a glance). Falls back to
# "<name>   low → high" for any channel not listed here. Renderer-only labelling (inv #2).
const OVERLAY_LEGENDS := {
	"light": "light   dark → bright",
	"free_nutrient": "nutrient   drained → rich",
	"detritus": "detritus   clean → litter",
}
# View modes (Rel-UI.0): the top toggle cycles Ecosystem → Specimen → Relations. The Relations view renders the
# emergent S×S FlowMatrix (core-measured inter-species joule flows) as a heatmap; it is renderer-only VIEW state
# (a third _view_mode value) and degrades gracefully until the F4 core wires the matrix (see _flow_matrix_or_empty).
const VIEW_NAMES := ["Ecosystem", "Specimen", "Relations", "Codex"]
const VIEW_COUNT := 4
const VIEW_ECOSYSTEM := 0
const VIEW_SPECIMEN := 1
const VIEW_RELATIONS := 2
const VIEW_CODEX := 3  # SP-4 browsable encyclopedia (renderer-only static content via codex.gd; inv #2)
# The species-genome traits, in canonical order (matches the core's Trait::ALL). Iterate THIS, never the
# specimens.json Dictionary's keys, so the readout order is stable (inv #3 hygiene, even in UI).
const TRAIT_KEYS := [
	"growth_rate", "stature", "branchiness", "leaf_size", "leaf_hue",
	"reflectance", "fecundity", "drought_tolerance", "kill_switch_linkage"
]
# The MICROBE (E. coli) phenotype, in canonical order — the 5 traits the core binds via ecoli_trait_map and
# exports through observe().phenotype (Debug-cased GrowthRate/GlucoseUptake/… → these snake_case keys). The
# specimen view selects this set vs TRAIT_KEYS by species so the readout shows the 5 real microbe phenotypes,
# not 9 plant bars where 8 read 0. growth_rate is shared with the plant set, so it lights up for both.
const MICROBE_TRAIT_KEYS := [
	"growth_rate", "glucose_uptake", "respiration_mode", "acetate_overflow", "fermentation_capacity"
]
# The PREDATOR (Bdellovibrio) phenotype: growth + the attack-rate lever. The spore-former (Bacillus / molds) and
# obligate-symbiont (Carsonella / Syn3) sets surface the previously-DROPPED diagnostic traits — they were silently
# omitted from the readout because TRAIT_KEY_MAP didn't carry them. _active_trait_keys() picks the set by morphotype.
const PREDATOR_TRAIT_KEYS := ["growth_rate", "predation_capacity"]
const SPORE_TRAIT_KEYS := ["growth_rate", "sporulation_capacity"]
const SYMBIONT_TRAIT_KEYS := ["growth_rate", "symbiosis_capacity"]
const FRAME_SECONDS := 0.45  # seconds per snapshot when playing a FILE run (the Timer cadence)
# Decoupled-single-thread live loop (keeps the brush/clicks responsive while the sim runs fast — see _process):
const STEPS_PER_SECOND_BASE := 1.0 / FRAME_SECONDS  # live generations/sec at speed 1.0 (the speed slider scales it)
const MAX_STEPS_PER_FRAME := 64  # hard cap so a slow/backlogged frame can't spiral — input keeps priority
const RENDER_HZ := 30.0  # snapshot+redraw cadence, DECOUPLED from the sim step rate (the heavy work, throttled)
const TARGET_FIELD_PX := 880.0  # the field is scaled to about this many pixels on its long side
# Zoom "scopes": magnification presets the viewport switches between (keys 1/2/3; SPEC §W10).
const SCOPES := [{"name": "field", "zoom": 1.0}, {"name": "patch", "zoom": 2.6}, {"name": "cells", "zoom": 6.0}]
const ZOOM_MIN := 0.6
const ZOOM_MAX := 12.0
# --live (P5): drive the sim live via the LiveSim gdext node instead of replaying snapshot files.
const LIVE_GRID := Vector2i(32, 32)  # snapshot grid pulled from LiveSim each tick (== the core world grid, so
# a render cell maps 1:1 to a world cell — the selective brush picks world cells directly, ADR-011 S-F)
const LIVE_STEP := 1  # generations advanced per tick (a FIXED integer — deterministic cadence, inv #3)
const LIVE_HISTORY := 150  # rolling snapshot buffer kept for the timeline / scrubbing
# LOAD-GEM-REPLAY v2: the entity_count a loaded gem's run resets under — mirrors the harness DISCOVER_ENTITY_COUNT
# (the per-species roster counts drive the actual spawn; this only feeds the metadata fold, so it must match for
# the closest reproduction of the captured run).
const GEM_ENTITY_COUNT := 1000
const SAVE_DIR := "user://saves/slot1"  # default save slot (the journal: seed.json + actions.ndjson)

var _snaps: Array = []  # parsed snapshot.gd instances, ordered by generation
var _idx: int = 0
var _cell: float = 12.0
var _overlay_mode: int = 1  # index into OVERLAY_NAMES; 1 = density
var _paused: bool = false
var _view_mode: int = 0  # 0 = ecosystem · 1 = specimen (L-system plants / microbe) · 2 = relations (FlowMatrix heatmap)
var _specimens: Dictionary = {}  # parsed specimens.json: {baseline:{...}, edits:[...]}
var _live_specimen_log: Array = []  # --live: incremental log of distinct genome states (baseline + per edit)
# --live, multi-species: per-species incremental specimen log, keyed by species_id (int) ->
# {key:String, name:String, entries:Array of {label,traits}}. Fed from LiveSim.observe_species() so EVERY
# species (not just the active observe() one) shows its OWN baseline + edits. _live_specimen_log mirrors the
# PRIMARY species' entries for back-compat with the existing single-species paths.
var _live_species_logs: Dictionary = {}
var _live_species_order: Array = []  # species_id ints in registry (SpeciesId) order — stable iteration (inv #3)
var _run_dir: String = ""
var _field_px := Vector2.ZERO

var _world: Node2D  # holds the ecosystem layers (terrain/overlay/organisms)
var _specimen_root: Node2D  # holds the L-system plant specimens
var _iso = null  # iso.gd transform instance (isometric is the DEFAULT); null = orthographic (--ortho opt-out)
var _iso_ground: Node2D  # CPU-diamond ground + data overlay (iso mode only)
var _vignette: CanvasLayer  # screen-space edge darkening (ecosystem view only)
var _pill_rail: Control  # rail of minimized-panel pills above the timeline (Phase U panel framework)
var _controls_panel: Control  # the wrapped control deck (Phase U)
var _terrain: TileMapLayer
var _overlay: Sprite2D
var _organisms: Node2D
var _colonies: Node2D  # ADR-029 S2: colony polygon layer (shown at Field scope; organisms at closer scopes)
# ADR-029 S4 — the SELECTED colony (packed species*65536+variant key, or -1), set on a world-click; pops the
# clicked district open to its members regardless of zoom. Re-applied to both layers each _show so a moving colony
# (the heritable Variant tag) stays selected. Presentation-only state (inv #2): a click → a render scope.
var _selected_colony_id: int = -1
# ADR-029 S4 — renderer-side COLONY REGISTRY (variant_id -> {species, label, color, gen_created, parent}) + the
# journal of fired brush strokes still awaiting their minted child id. Assembled in ORDERED passes (no hash-order
# iteration; inv #3) from observe_species() + the player's own ApplyEditRegion strokes. Names + colours districts.
var _colony_registry: Dictionary = {}
var _pending_brush: Array = []  # [{gen, species, cx, cy, r, parent, tries}] resolved against the next snapshot
var _cam: Camera2D
var _hud: Label
var _legend_label: Label
var _legend: Control
var _timer: Timer
var _view_buttons: Array = []  # the 3 segmented VIEW toggles (Ecosystem/Specimen/Relations), top-right panel
var _viewscope_panel: Control  # the always-on top-right VIEW + SCOPE switcher (separated from the CONTROLS deck)
var _play_button: Button
var _layer_picker: OptionButton
var _specimen_bounds := Rect2()
var _focus: int = 0  # which specimen (index into _specimen_list()) is focused in the specimen view
var _specimen_panel: Control
var _specimen_picker: OptionButton
# Relations view: a FULL-WINDOW graph/heatmap (like the specimen view) + a compact docked info/toggle card.
var _relations_root: Node2D  # holds the relations heatmap (parallels _specimen_root)
var _relations_panel: Control  # PanelChrome wrapper (🔗 RELATIONS) — now the compact info/toggle card only
var _relations_full: Control  # full-window container holding the graph + heatmap (fills the field, like _specimen_root)
var _relations_heatmap: Control  # the RelationsHeatmap _draw() node
var _relations_graph: Control  # Item 4: the RelationsGraph node-link _draw() node (companion to the heatmap)
var _relations_show_graph: bool = true  # Item 4: which relations representation is visible (Graph default vs Matrix)
var _relations_graph_btn: Button  # the Graph/Matrix segmented toggle (kept for set_pressed_no_signal sync)
var _relations_matrix_btn: Button
var _relations_banner: Label  # degrade-state banner (State 1/2/4 text; hidden in State 3)
var _relations_flowsum: Label  # plain-language "who's eating whom" top-N flow summary (parsed from the FlowMatrix)
var _relations_nearest: Label  # ADR-014 nearest-species strip (view-only/advisory; hidden when no overlay)
# Codex view (SP-4): a FULL-WINDOW browsable encyclopedia (modelled on _relations_full) — a left entry list +
# filter, a right scrollable detail pane. Renderer-only (inv #2): every field is a static codex.gd lookup; no
# biology is computed. Degrades to a "codex unavailable" note when codex.gd did not load (older build / unstaged).
var _codex_full: Control  # full-window container (visible only in the Codex view, like _relations_full)
var _codex_list: ItemList  # left list of entries (species / roles / genes / flows, declared order, inv #3)
var _codex_filter: LineEdit  # case-insensitive substring filter over the list labels/ids
var _codex_detail_box: VBoxContainer  # right scrollable detail pane for the selected entry
var _codex_entries: Array = []  # parallel to _codex_list rows: [{kind, id}] for the selected-row lookup
# Per-species panel vitals (Rel-UI.1): 3-up Population / Allele / Fitness, value + ▲▼ trend per row.
var _species_vital_rows: Array = []  # [{key:String, fmt:String, value:Label}] one per vitals row
var _species_vital_note: Label  # "pending core export" note, shown only when a stat reads "—"
var _prev_species_stats: Dictionary = {}  # last tick's per-species stat values (keyed "<species_id>:<key>") for ▲▼
var _trait_rows: Array = []  # [{name:Label, bar:ProgressBar, value:Label, delta:Label}] one per max(plant,microbe) trait row
var _prev_button: Button
var _next_button: Button
var _speed_slider: HSlider
var _scope_buttons: Array = []  # 3 Buttons, one per SCOPES preset (field/patch/cells)
var _frame_seconds: float = FRAME_SECONDS  # runtime FILE-replay interval (the speed slider mutates this)
# Live decoupled loop state (the speed slider sets _steps_per_second; carries accumulate fractional work).
var _steps_per_second: float = STEPS_PER_SECOND_BASE  # live step-rate target (scaled by the speed slider)
var _step_carry: float = 0.0  # fractional generations owed this frame (accumulator)
var _render_carry: float = 0.0  # seconds since the last snapshot/redraw (throttles to RENDER_HZ)
# LOAD-GEM-REPLAY v2: a loaded gem's RESOLVED mid-run CRISPR edit schedule (from the CORE resolver
# LiveSim.gem_edit_schedule — inv #2, NO target/gen math in GDScript). Each row is {gen_abs, cas, target, guide,
# species}, kept gen_abs >= 1 and stable-sorted by gen_abs (ties keep list order) so a single forward pointer
# (_gem_schedule_idx) fires them in the SAME order the harness journal does. Empty = no gem loaded (hash-neutral).
var _gem_schedule: Array = []
var _gem_schedule_idx: int = 0
var _syncing: bool = false  # re-entrancy guard so programmatic widget updates don't recurse via signals
var _timeline: Control  # full-width bottom generation timeline (timeline.gd)
var _tooltip: PanelContainer
var _tooltip_label: Label
var _detail_panel: PanelContainer
var _detail_box: VBoxContainer
# SP-4 codex: the renderer-only encyclopedia (static res://data/codex/codex.json). Built once; joined on the
# core-exported ids (key/go/role) by the inspect card + tooltips. Graceful {} when an entry is missing.
var _codex = Codex.new()
var _dragging: bool = false  # left-button drag-pan in progress
var _drag_moved: float = 0.0  # accumulated drag distance (to tell a click from a drag)
var _live = null  # LiveSim gdext node when --live is active (drives an open-ended run); null = file replay
var _menu = null  # the pre-run main-menu overlay while it is open (ADR-012 E4); null once dismissed
var _intervention_panel: Control  # live-mode CRISPR injection UI
var _cas_picker: OptionButton
var _crispr_species: OptionButton  # Variant-Lab A: which species the whole-species CRISPR edit targets (default primary)
var _locus_picker: OptionButton
var _guide_edit: LineEdit
var _inject_button: Button  # Item 1: explicit whole-species inject (CRISPR sub-panel); mirrors the Guide-Enter hook
var _inject_status: Label
var _cas_ids: Array = []  # cas variant id per _cas_picker item
var _locus_ids: Array = []  # locus id per _locus_picker item
var _injections: Array = []  # [{generation, tool, applied, label}] for the timeline markers (SP-3.7)
var _brush: Node2D  # selective-edit brush overlay (ADR-011 S-F), retinted per active tool (SP-3.6)
var _brush_on: bool = false  # brush mode active (paint region edits) vs normal pan/inspect
var _brush_radius: int = 4  # brush disc radius in world cells
var _brush_cell: Vector2i = Vector2i(-1, -1)  # hovered world cell
var _brush_painting: bool = false  # a left-button drag-paint stroke is in progress (SP-3.6 drag-to-paint)
var _brush_button: Button
# SP-3.6 intervention TOOL PALETTE: one active tool of the 6 (CRISPR / PCR / Antibiotic / Nutrient / Toxin / Inoculate).
# Selecting a tool swaps the per-tool param sub-panel + retints the brush; the brush then paints THAT tool's
# Action at the hovered cell. Biology stays in the core (inv #2) — these only issue the journaled Action + read
# the verdict. The ButtonGroup keeps exactly one selected.
const TOOL_CRISPR := 0
const TOOL_PCR := 1
const TOOL_ANTIBIOTIC := 2
const TOOL_NUTRIENT := 3
const TOOL_TOXIN := 4
const TOOL_INOCULATE := 5  # ADR-019 S3: the seed/inoculate brush — drops a baked contaminant at the painted disc
# Per-tool brush tint (the painted disc COLOUR signals which tool will land WHERE) — mirrors timeline.gd::TOOL_STYLE.
const TOOL_TINTS := [
	Color(0.42, 0.9, 0.46, 0.30),   # CRISPR green
	Color(0.36, 0.82, 0.92, 0.30),  # PCR cyan
	Color(0.95, 0.42, 0.42, 0.30),  # Antibiotic red
	Color(0.95, 0.78, 0.32, 0.30),  # Nutrient amber
	Color(0.74, 0.46, 0.95, 0.30),  # Toxin violet
	Color(0.62, 0.85, 0.38, 0.30),  # Inoculate biohazard-lime (ADR-019 contamination)
]
# The `tool` string each marker carries (timeline.gd keys its colour/glyph off these).
const TOOL_KEYS := ["crispr", "pcr", "cull", "nutrient", "toxin", "inoculate"]
var _active_tool: int = TOOL_CRISPR
var _tool_buttons: Array = []  # the 6 palette toggle Buttons (radio via a shared ButtonGroup)
var _tool_panels: Array = []   # the 6 per-tool param sub-VBoxes (only the active one is visible)
# PCR params
var _pcr_species: OptionButton
var _pcr_count: SpinBox
var _pcr_endow: SpinBox
# Antibiotic (cull) params
var _cull_species: OptionButton
var _cull_strength: HSlider
var _cull_strength_label: Label
# Nutrient params
var _nutrient_channel: OptionButton
var _nutrient_amount: SpinBox
# Toxin params
var _toxin_channel: OptionButton
var _toxin_amount: SpinBox
# Inoculate (ADR-019 S3 contamination seed) params: which baked contaminant to drop + the per-disc dose. The
# species KEYS the picker/menu offer (kebab file stems under res://data/species/ that bake a contaminant
# SpeciesSpec). DISCOVERED at UI build (see _discover_contaminant_keys) so a new airborne bake lights up
# automatically (no biology in GDScript — these are just file stems; the core builds the genome from the JSON
# bytes, inv #2). The discovery scans res://data/species/ and drops the NON-airborne stems below: the player
# species (default plant / ecoli / bdellovibrio) and the obligate symbionts (carsonella / syn3), which can
# never airborne-arrive (Mode B, host-targeted — the core's expand_schedule HARD-FILTERS them too). What
# remains is exactly the 7 baked airborne Mode-A contaminants (bacillus/pseudomonas/staph/cutibacterium/
# aspergillus-niger/penicillium/mycoplasma). Mirrors the core's ConsortiumConfig::default_mode_a subset.
const NON_AIRBORNE_STEMS := ["default", "ecoli", "bdellovibrio", "carsonella", "syn3"]
# Fallback when a res:// directory scan is unavailable (e.g. an odd export): the 7 known baked airborne keys.
const CONTAMINANT_KEYS_FALLBACK := [
	"bacillus", "pseudomonas", "staph", "cutibacterium", "aspergillus-niger", "penicillium", "mycoplasma"]
# Static per-contaminant blurbs for the consortium checkbox tooltips when the codex is not loaded (Item 2b). Short
# role/trait one-liners only — descriptive UI text, NOT biology (the core owns every genome; inv #2). Keyed by the
# airborne file stem; an unknown key falls back to a generic line in _consortium_blurb.
const CONSORTIUM_FALLBACK_BLURBS := {
	"bacillus": "Bacillus subtilis — hardy soil generalist; forms cull-surviving endospores.",
	"pseudomonas": "Pseudomonas aeruginosa — biofilm generalist; metabolically versatile opportunist.",
	"staph": "Staphylococcus epidermidis — skin flora; tolerant surface coloniser.",
	"cutibacterium": "Cutibacterium acnes — slow-growing anaerobe; skin/sebaceous niche.",
	"aspergillus-niger": "Aspergillus niger — mold; airborne conidia, hyphal decomposer.",
	"penicillium": "Penicillium — mold; airborne spores, fast saprophytic colony.",
	"mycoplasma": "Mycoplasma genitalium — wall-less minimal-genome microbe; classic cell-culture contaminant.",
}
# The core's ConsortiumConfig::default_mode_a kebab keys — the non-empty consortium the MENU seeds when the
# player picks a containment level > Sealed, so "Open" actually contaminates (mirrors immigration.rs).
const DEFAULT_MODE_A_KEYS := ["bacillus", "pseudomonas", "aspergillus-niger"]
var _contaminant_keys: Array = []  # discovered airborne contaminant stems (filled by _discover_contaminant_keys)
var _inoc_species: OptionButton  # the contaminant the seed brush drops (and a fired schedule references)
var _inoc_count: SpinBox         # organisms per inoculation disc
var _inoc_endow: SpinBox         # joules per inoculated organism (minted from the core's `immigration` tap)
var _registered_contaminants: Dictionary = {}  # key:String → true once register_contaminant_json succeeded this run
# Variant Lab (Slices B+C, renderer-only — inv #2): named snapshots of a species' CURRENT (post-edit) genome,
# exported as an OPAQUE SpeciesSpec JSON STRING by LiveSim.export_species_json (the single biology→spec mapping lives
# in the core). GDScript moves only the inert JSON text + a display name + ints — it never parses the genome. Each
# entry: {name:String, json:String, key:String, species_id:int, role:String, traits:Dictionary}. `key` is the
# species' core key (== the JSON's key, captured at save time) so a reseed resolves the SAME genome through
# register_contaminant_json — the very res:// boundary the airborne consortium uses; `role`/`key`/`traits` only drive
# the list glyph + caption.
var _saved_variants: Array = []
var _saved_variants_box: VBoxContainer  # the dynamic 🌱 Reseed list inside the CONTAMINATION panel (Slice C)
# Slice C arming: a "reseeded" saved variant is registered with the core + becomes the ACTIVE Inoculate payload, so
# the next Inoculate brush stroke (TOOL_INOCULATE) seeds THAT variant — reusing 100% of the inoculate machinery.
# -1 = none armed (the Inoculate brush falls back to the contaminant picker). Indexes into _saved_variants.
var _armed_variant_idx: int = -1
var _save_variant_name: LineEdit  # the specimen-view name field for the next saved variant (Slice B)
var _save_variant_button: Button
# ContainmentLevel knob (ADR-019 S3, the ISO-14644 ladder) + consortium menu. The level + the checked consortium
# keys + the pressure params are pushed to the core via set_containment BEFORE reset, which deterministically
# expands them into a journaled immigration schedule off the off-stream IMMG family (zero SimRng draws, inv #3).
# Default Sealed (level 0) → empty schedule → hash-neutral. GDScript only moves the level + keys + bytes (inv #2).
const CONTAINMENT_LABELS := ["🔒 Sealed (ISO 5 / OFF)", "Clean (ISO 7)", "Lab (ISO 8)", "☣ Open (room air)"]
var _containment_level: int = 0  # 0 Sealed · 1 Clean · 2 Lab · 3 Open (mirrors sim_core::ContainmentLevel)
var _containment_selector: OptionButton
var _consortium_checks: Dictionary = {}  # key:String → CheckBox (the consortium menu; checked keys ride the schedule)
var _containment_radius: int = 5         # schedule inoculation disc radius (cells)
var _containment_endow: SpinBox          # per-immigrant J for the scheduled events
var _containment_horizon: int = 400      # generations the schedule spans
var _containment_panel: Control
# OVERSIGHT earned-credit loop (ADR-017 S4–S6): the in-game earn→preview→spend surface over the harness ledger.
# Renderer-only (inv #2): GDScript moves only ints (species id, growth_ratio_q permille) + reads the VarDictionary
# the LiveSim oversight #[func]s marshal; the credit economy / FBA→factor map / spend gate all stay in the core.
# Every handler is has_method-guarded so an older cdylib / file replay degrades gracefully (no oversight #[func]s).
var _oversight_panel: Control
var _oversight_species: OptionButton     # which species the deep edit targets (E. coli + any editable species)
var _oversight_ratio: SpinBox            # growth_ratio_q permille (1000 = wild-type / no-op, 0 = growth-lethal KO)
var _oversight_credit_label: Label       # credit / cost / accrual ledger readout (refreshed live from oversight_state)
var _oversight_preview_label: Label      # predicted vs current demand factor + affordability (Preview output)
var _oversight_status: Label             # commit verdict line
var _oversight_commit_button: Button     # enabled only when credit suffices (affordable)
# Gamification (ADR-011 S-G2): a mission to SUPPRESS allele frequency in a target zone under a budget +
# deadline (the brush lowers allele, selection raises it → a tug-of-war). Renderer-side game rules over the
# core-exported snapshot (inv #2 — no biology computed here); not part of the determinism hash.
var _mission_on: bool = false
# The active species file stem (ADR-017): "" = abstract plant; non-empty (e.g. "ecoli") = a loaded species whose
# specimen view has no L-system plant body (the in-game specimen view is plant-shaped — a documented follow-up).
var _species_stem: String = ""
var _mission_zone: Vector2i = Vector2i(8, 8)  # target world cell (disc centre)
var _mission_radius: int = 6
var _mission_target: float = 0.40  # win when the zone's mean allele_freq drops to/below this
var _mission_deadline: int = 140  # lose if the generation passes this with the goal unmet
var _edit_budget: int = 6  # total edits (inject + brush) the mission allows
var _edits_used: int = 0
var _mission_status: int = 0  # 0 active · 1 won · 2 lost
var _mission_marker: Node2D  # cyan zone marker (a Brush instance reused as a static goal ring)
var _mission_panel: Control
var _mission_label: Label
var _mission_banner: Label
var _seed: int = 42  # active master seed (from --seed; New-run/Restart rebind it)
var _restart_button: Button
var _newrun_button: Button
var _seed_edit: LineEdit
var _titlebar: Control
var _title_badge: Label  # ● LIVE / REPLAY
var _title_status: RichTextLabel  # seed · gen · pop · fit · allele chip strip (BBCode → color-coded ▲▼ trend glyphs)
var _vitals_panel: Control
var _vitals_pop: Label
var _vitals_fit: Label
var _vitals_allele: Label
var _sparkline: Control
var _prev_obs: Dictionary = {}  # previous vitals, for the ▲▼ trend glyphs (deterministic last-vs-now)
var _fit_history: Array = []  # rolling mean-fitness [0,1] for the sparkline (live: per tick; replay: per snap)
var _allele_history: Array = []  # rolling allele-freq [0,1] for the sparkline
# Item (c): per-species extinction/boom alerts (--live). _prev_pop caches last render-tick's per-species
# population (keyed by species_id) so we can detect a crash-to-0 (extinction) or a >~10× jump (boom) vs the
# prior frame. _alert_label is a transient HUD flash; _alert_until is the engine time it auto-hides at.
var _prev_pop: Dictionary = {}  # species_id:int → population:int from the previous render frame
# Item 2(b): persistent EXTINCTION tracking for the specimen view. _ever_alive marks any species seen with pop>0
# (so we never false-flag a not-yet-spawned species); _extinct[sid]=true once a previously-alive species hits pop 0
# (or drops out of observe_species). It UN-marks on regermination (a spore bank coming back, pop 0→N) so the
# struck-through glyph reflects the live state. _render_specimens greys + strikes the extinct species (kept for
# investigation, never removed). Pure renderer state derived from the core's population export (inv #2).
var _ever_alive: Dictionary = {}  # species_id:int → true once observed with population > 0
var _extinct: Dictionary = {}     # species_id:int → true while currently extinct (struck-through in the specimen view)
var _alert_label: Label  # transient HUD notification ("✗ plant extinct" / "📈 bdellovibrio boom")
var _alert_until: float = 0.0  # Time.get_ticks_msec()/1000 deadline after which the flash hides
const ALERT_BOOM_FACTOR := 10  # a population jump of ≥ this multiple vs the prior frame counts as a "boom"
const ALERT_BOOM_FLOOR := 5    # ignore booms from a tiny prior pop (noise); require prev ≥ this to flag
const ALERT_HOLD_SEC := 3.5    # how long an alert flash stays visible


func _ready() -> void:
	var v := Engine.get_version_info()
	print("gene-sim UI booted — Godot %s (%s)" % [v.string, DisplayServer.get_name()])
	_seed = int(_arg_value("--seed", "42"))

	# ---- S4.2 headless gate path: parse one snapshot, report header, quit. Keep exact output ("snapshot OK").
	var snap_path := _arg_value("--snap")
	var shot_path := _arg_value("--shot")
	if snap_path != "" and shot_path == "":
		var snap := SnapshotReader.load_from(snap_path)
		if snap == null:
			printerr("snapshot load FAILED: %s" % snap_path)
			get_tree().quit(1)
			return
		print("snapshot OK — %dx%d, gen=%d, population=%d, cells=%d, channels=%d" % [
			snap.width, snap.height, snap.generation, snap.population, snap.cell_count(), snap.channel_count])
		get_tree().quit()
		return

	# ---- Resolve the snapshots to show: --live drives the sim via LiveSim; else a --run dir / --snap / auto.
	if _has_flag("--live") and _setup_live():
		pass  # _setup_live populated _snaps from a live LiveSim reset
	else:
		var run_dir := _arg_value("--run")
		if run_dir != "":
			_run_dir = run_dir
			_snaps = _load_run(run_dir)
		elif snap_path != "":
			var one := SnapshotReader.load_from(snap_path)
			if one != null:
				_snaps = [one]
		else:
			var newest := _newest_run_dir()
			if newest != "":
				print("auto-discovered run: %s" % newest)
				_run_dir = newest
				_snaps = _load_run(newest)
		if _run_dir != "":
			_specimens = _load_specimens(_run_dir)

	if _snaps.is_empty():
		# Headless smoke (S4.1) with nothing to show, or no run found: boot cleanly and exit.
		if DisplayServer.get_name() == "headless":
			print("headless smoke OK")
		else:
			print("no snapshots to render (pass --run <dir> or --snap <file>)")
		get_tree().quit()
		return

	_build_scene()
	var layer_arg := _arg_value("--layer")  # optional: preselect a data layer (0 off … 3 fitness) for --shot
	if layer_arg != "":
		_overlay_mode = clampi(int(layer_arg), 0, OVERLAY_NAMES.size() - 1)
	_idx = _pick_index(int(_arg_value("--gen", "-1")))
	_show(_idx)
	var zoom_arg := _arg_value("--zoom")  # optional: preset zoom scope for --shot
	if zoom_arg != "":
		_set_zoom(float(zoom_arg))
	var reseed := _arg_value("--reset-seed")  # optional: exercise the lifecycle reset for --shot verification
	if _live != null and reseed != "":
		_do_reset(int(reseed))
	# --steps N (live shot): advance the sim N generations before the view/capture so the ecosystem DEVELOPS —
	# populations establish (varied node sizes on the map + relations graph) and the FlowMatrix accumulates the
	# measured trophic flows the relations graph draws as edges. Pure stepping of the deterministic core (inv #3).
	var steps_arg := _arg_value("--steps")
	# Skip the generic pre-gem advance when a gem is being loaded: the gem reconfigure (below) RESETS the sim, so
	# stepping here would advance a soon-discarded default sim. The --gem capture path re-advances WITH edit firing.
	if _live != null and steps_arg != "" and steps_arg.is_valid_int() and _arg_value("--gem") == "":
		_live.step(maxi(0, int(steps_arg)))
		_publish_frame()
	if _live != null and _has_flag("--inject"):  # optional: fire one demo injection for --shot verification
		_live.step(20)
		_publish_frame()
		# Issue 4: seed every species' BASELINE specimen from the PRE-edit genome so the demo --shot shows the
		# baseline alongside the appended "edit 1" variant (the inject below force-appends the edited variant).
		_log_live_genome("baseline — gen %d" % int(_live.observe().get("generation", 0)))
		_on_inject_pressed()
	if _live != null and _has_flag("--brush"):  # optional: show + fire one demo brush stroke for --shot
		_live.step(20)
		_publish_frame()
		# --tool <crispr|pcr|cull|nutrient|toxin|inoculate>: pick the palette tool to demo (SP-3.6/ADR-019 per-tool
		# smoke); default CRISPR.
		var tool_arg := _arg_value("--tool")
		if tool_arg != "" and _tool_buttons.size() == TOOL_KEYS.size():
			var ti: int = maxi(0, TOOL_KEYS.find(tool_arg))
			_tool_buttons[ti].set_pressed_no_signal(true)
			_select_tool(ti)
		_set_brush_mode(true)
		_brush_cell = Vector2i(LIVE_GRID.x / 2, LIVE_GRID.y / 2)
		_brush_radius = 6
		_brush.set_brush(_brush_cell, _brush_radius)
		_apply_active_tool()
		# ADR-029 S4: publish + show the POST-edit frame so the following --shot captures what the brush did — the
		# disc now carries its freshly-minted child Variant id, so colonies.gd renders it as a NESTED hole-cut
		# district (the S2 deferred acceptance) and _resolve_pending_colonies names it. Renderer-only (inv #2).
		_publish_frame()
		_show(_snaps.size() - 1)
		# Select the brushed colony so a --shot also demonstrates the selected-pop (the disc explodes to members).
		if "dominant_variant_id" in _snaps[_snaps.size() - 1]:
			var bsid := _dominant_species_at(_brush_cell)
			var bvid := _dominant_variant_at(_brush_cell)
			if bsid >= 0 and bvid >= 0:
				_set_selected_colony(bsid * 65536 + bvid)
	if _arg_value("--view") == "relations":  # optional: open the Relations FlowMatrix heatmap for --shot
		_set_view_mode(VIEW_RELATIONS)
	if _arg_value("--view") == "codex":  # optional: open the browsable CODEX encyclopedia for --shot
		_set_view_mode(VIEW_CODEX)
	if _arg_value("--view") == "specimen":  # optional: open the L-system specimen view for --shot
		_set_view_mode(VIEW_SPECIMEN)
		var focus_arg := _arg_value("--focus")  # optional: focus a specific specimen (0=baseline, 1..=edits)
		if focus_arg != "" and not _specimen_list().is_empty():
			_focus = clampi(int(focus_arg), 0, _specimen_list().size() - 1)
			if _specimen_picker != null:
				_specimen_picker.select(_focus)
			_on_specimen_selected(_focus)  # re-run readout/emphasis/frame for the chosen specimen
	var inspect_arg := _arg_value("--inspect")  # "x,y": pin the cell detail panel for --shot
	if inspect_arg != "" and _view_mode == 0 and not _snaps.is_empty():
		var parts := inspect_arg.split(",")
		if parts.size() == 2:
			var cx := int(parts[0])
			var cy := int(parts[1])
			var snap = _snaps[_idx]
			if cx >= 0 and cy >= 0 and cx < snap.width and cy < snap.height:
				var i: int = cy * snap.width + cx
				_fill_detail("Cell (%d, %d)" % [cx, cy], _cell_lines(snap, i))

	# Headless render smoke (gate): build the scene + specimen plants + the detail panel, prove it all
	# constructs without a GPU, quit.
	if _has_flag("--check"):
		_render_specimens()  # exercise the L-system build path headlessly (catches GDScript errors)
		_update_trait_readout()  # exercise the per-species vitals + trait readout build path (Rel-UI.1)
		_refresh_relations()  # exercise the relations heatmap refresh + degrade path (Rel-UI.0, State 1 in replay)
		_refresh_codex()  # SP-4: exercise the browsable CODEX list build + detail render headlessly (inv #4)
		for _e in _codex_entries:  # render EVERY entry once so a typo in any kind branch (species/role/gene/flow) goes RED
			_render_codex_detail(_e)
		_fill_detail("(check)", ["density 0.0"])  # exercise the detail/ontology rendering path
		# SP-4 headless guards (inv #4 — every path built before any GPU): (a) BUILD every baked species' glyph via
		# the key-led factory so a parse error / malformed polygon in ANY morphotype body goes RED; (b) load the
		# codex + exercise the codex-enriched inspect join with a real species so a garbled codex.json or a broken
		# join goes RED. The deferred SP-4 died because its GDScript path was never built headlessly — this fixes it.
		var built := _check_build_all_glyphs()
		var codex_ok := _check_codex_inspect()
		print("render scene OK — %d snapshots, %d specimens, cell=%d, grid %dx%d, glyphs=%d, codex=%s" % [
			_snaps.size(), _specimen_list().size(), int(_cell), _snaps[0].width, _snaps[0].height,
			built, "OK" if codex_ok else "MISSING"])
		get_tree().quit()
		return

	# LOAD-GEM-REPLAY v2: --gem <path> loads a saved emergent-run gem (an ABSOLUTE filesystem path — data/runs is
	# gitignored, NOT under res://) and replays it INCLUDING its mid-run CRISPR edit schedule, which the CORE
	# (LiveSim.gem_edit_schedule) resolves — GDScript computes NO target/gen (inv #2). Headless → drive the
	# deterministic edit-schedule SMOKE (report APPLIED edits) + quit; windowed → configure the run, arm the
	# schedule, and fall through to the live play loop (which fires each edit at the top of its gen's step).
	var gem_arg := _arg_value("--gem")
	if _live != null and gem_arg != "":
		if DisplayServer.get_name() == "headless":
			_run_gem_smoke(gem_arg)
			return
		_configure_run_from_gem(gem_arg)  # windowed: compose + arm; the _process live loop replays it
		# SCENARIO GIF PREVIEW capture (tools/make_starter_gif.sh): with --shot --steps N, advance the freshly-loaded
		# gem N generations HERE — firing each due CORE-resolved edit + scheduled immigration at the top of every
		# gen's step (the SAME interleave the live _process loop uses) — so a per-keyframe --shot lands on a DEVELOPED
		# frame incl. the gem's edits. Renderer-only (inv #2): it only drives existing core surfaces; no biology here.
		if shot_path != "" and steps_arg != "" and steps_arg.is_valid_int():
			_advance_gem_for_shot(maxi(0, int(steps_arg)))

	# STARTERS GALLERY: --gallery-check drives the scenario picker headlessly (the gate — list every starter, load
	# each kind through the proven path, assert a gen-N load carries its recorded markers) + quits; --gallery opens
	# the picker windowed. Renderer-only (inv #2): the gallery moves inert JSON + drives existing #[func]s.
	if _live != null and _has_flag("--gallery-check"):
		_run_gallery_check()
		return
	if _live != null and _has_flag("--gallery"):
		_show_gallery()

	if shot_path != "":
		if _live != null and _has_flag("--menu"):
			_show_main_menu()  # capture the main-menu overlay for visual verification
		await _take_shot(shot_path)
		return

	# Playback driver (windowed). LIVE mode uses the decoupled per-frame loop in _process (input-first,
	# step-budget, throttled render) so a fast sim never starves the brush/clicks; the Timer drives only FILE
	# replay. (Determinism is unaffected — _process advances by whole LIVE_STEP generations, never wall-clock.)
	_timer = Timer.new()
	_timer.wait_time = _frame_seconds
	add_child(_timer)
	if _live != null:
		set_process(true)
	else:
		_timer.timeout.connect(_advance)
		if _snaps.size() > 1:
			_timer.start()

	# Main menu (ADR-012 E4): a plain windowed --live launch lets the player set the world before it runs.
	if _live != null and _should_show_menu():
		_show_main_menu()


# ──────────────────────────── environment + main menu (ADR-012 Phase E) ───────────────────────────────────

## Apply the climate / population / species CLI flags to the LiveSim BEFORE its first reset, so a headless or
## scripted run (`--lat/--lon/--temp/--season/--entities/--species`) is byte-identical to driving the same values
## through the menu (inv #2: the renderer only forwards numbers + the inert species string; the core builds the
## world). Absent flags = the neutral world + the default plant.
func _apply_cli_environment() -> void:
	if _live == null:
		return
	var ent := _arg_value("--entities")
	if ent != "":
		_live.set_entity_count(int(ent))
	_live.set_environment(
		float(_arg_value("--lat", "0")),
		float(_arg_value("--lon", "0")),
		float(_arg_value("--temp", "0.5")),
		int(_arg_value("--season", "0")),
	)
	# --species <stem> (ADR-017): the scripted/headless equivalent of the menu's Species picker. Routes through
	# the SAME res:// byte-mover the menu uses, so the specimen view + readout pick up E. coli identically.
	_species_stem = _apply_species(_arg_value("--species"))
	# --roster "stem:count,stem:count,…" (shot/headless parity for a MULTI-species run — the scripted equivalent of
	# the menu's roster composer / the Load Starter preset). Lets --shot exercise a real multi-species map + the
	# relations GRAPH with measured predator/prey flows. When present + non-trivial it OVERRIDES --species. Renderer
	# only moves stems+counts (inv #2); the core builds every species; armed here, BEFORE _do_reset (inv #3).
	var roster_arg := _arg_value("--roster")
	if roster_arg != "":
		var roster := _parse_roster_arg(roster_arg)
		if _apply_roster(roster):
			_species_stem = _roster_active_stem(roster)


## Show the menu only for a plain interactive launch — never for the headless/gate paths (--shot/--check) or an
## explicit --no-menu (the CLI-flag parity path), so scripted runs stay deterministic + menu-free.
func _should_show_menu() -> bool:
	return (
		DisplayServer.get_name() != "headless"
		and _arg_value("--shot") == ""
		and not _has_flag("--no-menu")
		and not _has_flag("--check")
		and not _has_flag("--inject")
		and not _has_flag("--brush")
		and _arg_value("--gem") == ""  # --gem composed the run from a saved gem → skip the new-run menu
	)


## Instantiate the main-menu overlay, pause the sim behind it, and wire Start → reconfigure + reseed in place.
func _show_main_menu() -> void:
	var menu := MainMenu.new()
	menu.setup(_live, _seed, _mission_on)  # seed the mission checkbox from the --mission CLI flag (default off)
	menu.start_run.connect(_on_menu_start)
	menu.load_gem.connect(_on_menu_load_gem)  # LOAD-GEM-REPLAY v2: the menu's "Load Gem" FileDialog hands a gem path
	menu.open_gallery.connect(_on_menu_open_gallery)  # STARTERS: the menu's "Starters Gallery" → the scenario picker
	add_child(menu)
	_menu = menu  # mark the modal open so _unhandled_input swallows sim hotkeys until Start
	_paused = true
	if _timer != null:
		_timer.stop()


## The menu's "Starters Gallery": dismiss the (self-freeing) menu and open the scenario picker. The gallery is the
## SAME modal band as the menu (it sets _menu so sim hotkeys stay swallowed), and routes its Play back to the
## proven menu-Start / load_session paths.
func _on_menu_open_gallery() -> void:
	_show_gallery()


## Instantiate the STARTERS scenario picker, keep the sim paused behind it, and wire its Play/Back. gen-1 → the
## proven _on_menu_start path (fresh run); gen-N checkpoint → the EXISTING load_session #[func]; Back → the new-run
## menu. Renderer-only (inv #2): the gallery moves inert JSON + drives existing #[func]s; main.gd owns the core.
func _show_gallery() -> void:
	if _live == null:
		return
	var g := Gallery.new()
	g.setup(_live)
	g.play_gen1.connect(_on_gallery_play_gen1)
	g.play_checkpoint.connect(_on_gallery_play_checkpoint)
	g.back.connect(_on_gallery_back)
	add_child(g)
	_menu = g  # the gallery is modal too — swallow sim hotkeys until Play/Back
	_paused = true
	if _timer != null:
		_timer.stop()


## A gallery gen-1 Play: route the starter cfg through the proven menu Start path (set_roster/set_environment/
## set_containment/reset) — byte-identical to composing the same roster + env in the new-run menu.
func _on_gallery_play_gen1(cfg: Dictionary) -> void:
	_menu = null  # the gallery freed itself after emitting → hotkeys live again
	_on_menu_start(cfg)


## A gallery checkpoint Play: restore the recorded session at gen N via the EXISTING load_session #[func], then set
## the gallery-parsed recorded markers on the live timeline (the cdylib exposes no journal_actions export, so the
## renderer's own journal→marker projection is authoritative — the SAME projection the gallery preview scrubbed).
func _on_gallery_play_checkpoint(slug: String, markers: Array) -> void:
	_menu = null  # the gallery freed itself after emitting → hotkeys live again
	if not _load_checkpoint_session(slug):
		push_warning("Starters: failed to load checkpoint session %s" % slug)
		return
	_injections = markers.duplicate()  # the recorded-edit projection (renderer-side; inv #2)
	if _timeline != null:
		_timeline.set_markers(_injections)


## Back from the gallery → reopen the new-run main menu.
func _on_gallery_back() -> void:
	_menu = null
	_show_main_menu()


## Load a gen-N checkpoint STARTER session via the EXISTING LiveSim.load_session #[func]: the staged
## res://data/presets/starters/<slug>/ dir (seed.json + actions.ndjson). The core rebuilds the FULL run
## composition + replays the journal deterministically (inv #3), restoring the run to gen N. Renderer-only (inv
## #2): GDScript only globalizes the staged path + drives load_session + resyncs the view. has_method-guarded so
## an older cdylib / file replay degrades with a clear message. Returns false on a missing export or a load error.
func _load_checkpoint_session(slug: String) -> bool:
	if _live == null or not _live.has_method("load_session"):
		push_warning("Starters: LiveSim.load_session unavailable in this build — cannot restore the checkpoint.")
		return false
	var dir := ProjectSettings.globalize_path("res://data/presets/starters/%s" % slug)
	if not DirAccess.dir_exists_absolute(dir):
		push_warning("Starters: checkpoint session dir not found at %s (was data/presets staged into res://?)" % dir)
		return false
	var r: Dictionary = _live.load_session(dir)
	if not bool(r.get("ok", false)):
		push_warning("Starters: load_session failed for %s: %s" % [slug, str(r.get("detail", "?"))])
		return false
	_resync_to_live()  # clears _injections; the caller sets the recorded markers next
	print("STARTER CHECKPOINT loaded — %s · gen %d · %d actions" % [slug, int(r.get("generation", 0)), int(r.get("actions", 0))])
	return true


## HEADLESS GALLERY GATE (--gallery-check, inv #4): instantiate the scenario picker, list every starter, exercise
## BOTH Play paths through the real handlers (gen-1 → _on_menu_start fresh run; gen-N → load_session), and assert a
## checkpoint load carries its recorded edit markers. Prints a greppable `GALLERY OK …` line + quits (non-zero on a
## broken gallery — empty index, a checkpoint with no markers, or a failed load). Renderer-only (inv #2).
func _run_gallery_check() -> void:
	var probe := Gallery.new()
	probe.setup(_live)
	add_child(probe)
	var s: Dictionary = probe.check_headless()
	probe.queue_free()
	var count := int(s.get("count", 0))
	if count <= 0:
		print("GALLERY FAIL — no starters listed (index missing/empty at res://data/presets/starters/index.json)")
		get_tree().quit(1)
		return

	# gen-1 Play: route the first gen-1 starter through the proven menu Start path (resets the live core).
	var gen1_loaded := false
	var first_gen1 := int(s.get("first_gen1", -1))
	if first_gen1 >= 0:
		var g1 := Gallery.new()
		g1.setup(_live)
		g1.play_gen1.connect(_on_gallery_play_gen1)
		add_child(g1)
		g1.select_entry(first_gen1)
		g1.play_selected()  # emits play_gen1 → _on_menu_start → a fresh deterministic run
		gen1_loaded = _live.observe() != null

	# gen-N Play: load the first checkpoint via the EXISTING load_session #[func]; the recorded markers must land.
	var checkpoint_markers := 0
	var checkpoint_loaded := false
	var first_ckpt := int(s.get("first_checkpoint", -1))
	if first_ckpt >= 0:
		var g2 := Gallery.new()
		g2.setup(_live)
		g2.play_checkpoint.connect(_on_gallery_play_checkpoint)
		add_child(g2)
		g2.select_entry(first_ckpt)
		g2.play_selected()  # emits play_checkpoint → load_session + sets _injections to the recorded markers
		checkpoint_markers = _injections.size()
		checkpoint_loaded = checkpoint_markers > 0

	print("GALLERY OK — listed=%d gen1=%d checkpoint=%d gen1_loaded=%s checkpoint_loaded=%s checkpoint_markers=%d" % [
		count, int(s.get("gen1", 0)), int(s.get("checkpoint", 0)),
		gen1_loaded, checkpoint_loaded, checkpoint_markers])

	# A listed checkpoint that loads with ZERO recorded markers is a broken gen-N path (the journal projection
	# failed) → RED. A library with only gen-1 starters passes on the listing + gen-1 load alone.
	if first_ckpt >= 0 and not checkpoint_loaded:
		print("GALLERY FAIL — checkpoint %d loaded but showed no recorded markers" % first_ckpt)
		get_tree().quit(1)
		return
	get_tree().quit()


## The menu's Start: apply seed/entity/climate to the LiveSim, reseed the world in place (no relaunch — the same
## proven _do_reset path), and resume. The menu frees itself after emitting.
func _on_menu_start(cfg: Dictionary) -> void:
	_menu = null  # the modal is dismissed (the menu frees itself after emitting) → hotkeys live again
	if _live == null:
		return
	_seed = int(cfg.get("seed", _seed))
	_live.set_entity_count(int(cfg.get("entities", 1000)))
	_live.set_environment(
		float(cfg.get("lat", 0.0)),
		float(cfg.get("lon", 0.0)),
		float(cfg.get("temp", 0.5)),
		int(cfg.get("season", 0)),
	)
	# SP-2: compose the run from the menu ROSTER when non-trivial; else the legacy single-species path. The roster
	# is moved as inert JSON bytes + int counts (FileAccess reads the res:// JSON; set_roster hands it to the core's
	# serde + SpeciesSpec::build) — GDScript moves only strings + ints, all biology stays in Rust (inv #2). Pushing
	# the roster + containment BEFORE _do_reset is load-bearing: the core seeds the single RNG ONCE over the full
	# composed population at reset (inv #3), and the containment schedule expands deterministically at reset.
	var roster: Array = cfg.get("roster", [])
	var species_stem: String = String(cfg.get("species", ""))
	if _apply_roster(roster):
		# A composed roster (≥2 species OR a non-default stem) was armed via set_roster. The first roster stem is
		# tracked as the "active" species for the specimen view / readout (the rest light up via per-species panels).
		_species_stem = _roster_active_stem(roster)
	else:
		# Trivial roster (single default plant) or set_roster unavailable/failed → the proven single-species path.
		# ADR-017: run the selected species (e.g. "ecoli") before reset; "" keeps the abstract plant.
		_species_stem = _apply_species(_roster_active_stem(roster) if not roster.is_empty() else species_stem)
	# Containment (ADR-019 S3): push the up-front level BEFORE reset so its immigration schedule expands
	# deterministically. Sealed (0) → empty schedule → hash-neutral. For level > 0 the menu seeds the core's
	# default Mode-A consortium so "Open"/"Clean"/"Lab" actually contaminate (R1 fix); the in-run CONTAMINATION
	# panel still lets the player recompose it.
	var seeded_consortium: Array = _apply_menu_containment(int(cfg.get("containment", 0)))
	_do_reset(_seed)
	# The fresh reset rebuilt the core env (empty registry). Register the seeded consortium NOW so a fired
	# schedule event can resolve its key against a loaded genome (mirrors _on_apply_containment_pressed).
	for key in seeded_consortium:
		_ensure_contaminant_registered(str(key))
	_populate_locus_picker()  # refresh the edit-target picker for the new species' genome (ADR-017)
	_populate_species_pickers()  # refresh the PCR/Antibiotic target-species pickers for the new roster (SP-3.6)
	# Mission is a MENU choice now (off by default = free-play sandbox). Apply it + (re)activate its UI on Start;
	# the --mission CLI flag is the headless/scripted equivalent (set in _setup_live, overridden here).
	_mission_on = bool(cfg.get("mission", false))
	_mission_status = 0
	_edits_used = 0
	if _mission_panel != null:
		_mission_panel.set_active(_mission_on)
	if _mission_marker != null and _mission_on:
		_mission_marker.set_brush(_mission_zone, _mission_radius)
	if _mission_banner != null:
		_mission_banner.visible = false
	_paused = false
	if _timer != null:
		_timer.start()


## Read a species JSON from the res:// VFS and load it into the live core (ADR-017). "" = the default plant
## (clears). Returns the EFFECTIVE stem ("" if the file was missing or failed to build). Biology stays in Rust
## (inv #2): this only moves bytes — FileAccess reads the inert JSON text, set_species_json hands it straight to
## the core's serde + SpeciesSpec::build. res:// resolves IDENTICALLY in `--live` dev (project dir on disk) and
## in the exported .deb/.exe (embedded PCK), which is exactly why the old cwd/exe-dir dance disappears. Graceful
## fallback: a missing file or invalid JSON → warning → default plant → byte-identical historical run (inv #3).
func _apply_species(stem: String) -> String:
	if _live == null:
		return stem
	if stem == "":
		_live.set_species_json("")  # clear to default plant
		return ""
	var path := "res://data/species/%s.json" % stem
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		push_warning("species '%s' not found at %s; using the default plant" % [stem, path])
		_live.set_species_json("")
		return ""
	var text := f.get_as_text()  # whole JSON; FileAccess (RefCounted) closes on free
	if not _live.set_species_json(text):
		push_warning("species '%s' failed to validate; using the default plant" % stem)
		_live.set_species_json("")  # ensure cleared state on a failed build
		return ""
	return stem


## SP-2: arm the multi-species ROSTER on the live core from a cfg.roster ([{stem,count}], in load-bearing order).
## Returns true when a COMPOSED roster was armed via LiveSim.set_roster (so the caller skips the single-species
## path); false when the roster is trivial (a single default plant — reproduces today's run byte-for-byte), the
## binding is unavailable (forward-compat probe, mirroring observe_species/fire_due_inoculations), or every build
## failed (graceful fallback to the default plant). The renderer only moves inert JSON bytes + int counts; the core
## builds every RosterEntry / genome→phenotype (inv #2). The single ChaCha8Rng seeds ONCE over the full population
## at reset (inv #3) — so set_roster MUST be applied BEFORE _do_reset (the caller's order).
func _apply_roster(roster: Array) -> bool:
	if _live == null or roster.is_empty():
		return false
	# Trivial roster = exactly one row, the default plant → keep the proven single-species path (hash-neutral).
	if roster.size() == 1 and String((roster[0] as Dictionary).get("stem", "default")) == "default":
		return false
	if not _live.has_method("set_roster"):
		push_warning("LiveSim.set_roster unavailable in this build; falling back to the single-species path")
		return false
	# Collect the JSON texts + counts positionally (PackedStringArray/PackedInt32Array zip by index in the core).
	var texts := PackedStringArray()
	var counts := PackedInt32Array()
	for e in roster:
		var d: Dictionary = e
		var stem: String = String(d.get("stem", "default"))
		var path := "res://data/species/%s.json" % stem
		var f := FileAccess.open(path, FileAccess.READ)
		if f == null:
			push_warning("roster species '%s' not found at %s; falling back to the default plant" % [stem, path])
			_apply_species("")
			return false
		texts.append(f.get_as_text())  # whole JSON; FileAccess (RefCounted) closes on free
		counts.append(maxi(0, int(d.get("count", 0))))
	if not _live.set_roster(texts, counts):
		# A build failed in the core (graceful, mirroring set_species_json) → byte-clean default-plant run.
		push_warning("roster failed to build; falling back to the default plant")
		_apply_species("")
		return false
	return true


## The roster's first/active species stem (the one the specimen view + readout track; the rest light up via the
## per-species panels). "default" maps to the abstract plant (its core key is ""), so return "" for _species_stem.
func _roster_active_stem(roster: Array) -> String:
	if roster.is_empty():
		return ""
	var stem: String = String((roster[0] as Dictionary).get("stem", "default"))
	return "" if stem == "default" else stem


## Parse a "--roster" CLI string ("default:800,ecoli:250,bdellovibrio:50") into a [{stem,count}] array in the given
## (load-bearing) order. A bare "stem" with no ":count" defaults to 100. Pure renderer string parsing (inv #2).
func _parse_roster_arg(arg: String) -> Array:
	var roster: Array = []
	for part in arg.split(",", false):
		var kv := String(part).strip_edges().split(":")
		var stem := String(kv[0]).strip_edges()
		if stem == "":
			continue
		var count := 100
		if kv.size() >= 2 and String(kv[1]).strip_edges().is_valid_int():
			count = int(String(kv[1]).strip_edges())
		roster.append({"stem": stem, "count": count})
	return roster


## SP-2: push the menu's up-front ContainmentLevel to the core BEFORE reset (ADR-019 S3). The menu chooses only
## the LEVEL; when that level is > Sealed we seed the core's DEFAULT consortium (DEFAULT_MODE_A_KEYS, mirroring
## ConsortiumConfig::default_mode_a) so "Open"/"Clean"/"Lab" actually contaminate — R1 fix: previously this
## always pushed an EMPTY consortium, so expand_schedule returned Vec::new() for n_species==0 regardless of level
## and the menu choice was a silent no-op. Sealed (0) → empty consortium → empty schedule → hash-neutral. The
## schedule expands deterministically at reset off the off-stream IMMG family (zero SimRng draws, inv #3). The
## in-run CONTAMINATION panel still lets the player recompose the full consortium. Returns the seeded keys so the
## caller can register them AFTER reset (a fresh core env has an empty registry — a fired schedule event must
## resolve its key against a loaded genome, exactly like _on_apply_containment_pressed).
func _apply_menu_containment(level: int) -> Array:
	if _live == null or not _live.has_method("set_containment"):
		return []
	_containment_level = clampi(level, 0, CONTAINMENT_LABELS.size() - 1)
	if _containment_selector != null:
		_containment_selector.select(_containment_level)
	# Sealed (0) → empty consortium (hash-neutral). level > 0 → the core's default Mode-A consortium, filtered to
	# stems whose res:// JSON actually bakes (the core does the real serde/build at register time; inv #2).
	var keys := PackedStringArray()
	if _containment_level > 0:
		for key in DEFAULT_MODE_A_KEYS:
			if FileAccess.file_exists("res://data/species/%s.json" % key):
				keys.append(key)
		# Reflect the seeded consortium in the in-run panel's checkboxes (kept in sync; built later, so guard).
		for key in keys:
			var cb: CheckBox = _consortium_checks.get(key, null)
			if cb != null:
				cb.button_pressed = true
	var endow := int(_containment_endow.value) if _containment_endow != null else 120000
	_live.set_containment(_containment_level, keys, _containment_radius, endow, _containment_horizon)
	var seeded: Array = []
	for key in keys:
		seeded.append(str(key))
	return seeded


# ──────────────────────────── live mode (P5): drive the sim via the LiveSim gdext node ────────────────────

## Load the LiveSim GDExtension at RUNTIME (so the default project + gate stay extension-free), instantiate
## it, reset, and seed _snaps with the gen-0 snapshot. Returns false (→ fall back to file replay) if the
## cdylib is not built or the extension fails to load. The renderer only CALLS LiveSim (inv #2: biology in Rust).
func _setup_live() -> bool:
	# An EXPORTED build auto-registers LiveSim at startup from the bundled res://gene_sim.gdextension — use it
	# directly. Only the dev/editor run (extension not yet loaded) needs to locate + load the source-tree
	# cdylib at runtime. Probing the source tree FIRST would wrongly fail the exported game (no crates/ in the
	# PCK), so the class check gates the dev-only probe.
	if not ClassDB.class_exists("LiveSim"):
		var dylib := ProjectSettings.globalize_path("res://../crates/godot-sim/target/debug/libgodot_sim.dylib")
		if not FileAccess.file_exists(dylib):
			dylib = ProjectSettings.globalize_path("res://../crates/godot-sim/target/debug/libgodot_sim.so")
		if not FileAccess.file_exists(dylib):
			printerr("--live needs the LiveSim cdylib. Build it:  cargo build --manifest-path crates/godot-sim/Cargo.toml")
			return false
		var ext := "user://gene_sim_live.gdextension"
		var f := FileAccess.open(ext, FileAccess.WRITE)
		if f == null:
			printerr("--live: cannot write the runtime .gdextension")
			return false
		f.store_string(("[configuration]\nentry_symbol = \"gdext_rust_init\"\ncompatibility_minimum = 4.6\n"
			+ "[libraries]\nmacos.debug = \"%s\"\nmacos.release = \"%s\"\nlinux.debug = \"%s\"\nlinux.release = \"%s\"\n")
			% [dylib, dylib, dylib, dylib])
		f.close()
		var st := GDExtensionManager.load_extension(ext)
		if not ClassDB.class_exists("LiveSim"):
			printerr("--live: failed to load LiveSim extension (status %d)" % st)
			return false
	_live = ClassDB.instantiate("LiveSim")
	_apply_cli_environment()  # CLI env/entity flags (headless + scripted parity); the menu overrides on Start
	_live.reset(_seed)
	var snap = SnapshotReader.parse_bytes(_live.snapshot(LIVE_GRID.x, LIVE_GRID.y))
	if snap == null:
		printerr("--live: LiveSim.snapshot() returned unparseable bytes")
		_live = null
		return false
	_snaps = [snap]
	_live_specimen_log = []
	_live_species_logs = {}
	_live_species_order = []
	_log_live_genome("baseline — gen 0")  # seed the specimen history before any edit (incremental log)
	# Default = SANDBOX (free play, unlimited edits). The suppress-the-zone mission is opt-in behind --mission
	# until deeper tasks exist (S-G2 stays available but off by default).
	_mission_on = _has_flag("--mission")
	print("LIVE MODE — %s (open-ended run, %d gen/tick)" % [
		"MISSION" if _mission_on else "sandbox", LIVE_STEP])
	return true


## Live-mode per-frame loop (decoupled-single-thread, inv #2/#3): the engine delivers input BEFORE _process, so
## the brush + clicks stay responsive; we advance the sim by whole LIVE_STEP generations on a per-frame budget
## (fixed-integer steps → deterministic; the journal replays bit-exact) and do the heavy snapshot+redraw only at
## RENDER_HZ. FILE replay uses the Timer, not this. (History granularity is the render rate, not per-generation;
## lower the speed slider for finer detail.)
func _process(delta: float) -> void:
	_tick_alert_flash()  # Item (c): expire a finished extinction/boom flash even while paused (wall-clock UI only)
	# The live sim keeps stepping in the Ecosystem AND Relations views (the FlowMatrix is per-generation, so the
	# heatmap wants live frames); only the Specimen view pauses stepping (it inspects a static genome). Determinism
	# is unaffected — _process advances by whole LIVE_STEP generations, never wall-clock.
	if _paused or _view_mode == VIEW_SPECIMEN or _live == null:
		return
	# Advance: accumulate owed generations, then step in a bounded loop (cap so a slow/backlogged frame can't
	# spiral — input keeps priority over throughput).
	_step_carry += _steps_per_second * delta
	var steps := int(_step_carry)
	_step_carry -= float(steps)
	if steps > MAX_STEPS_PER_FRAME:
		steps = MAX_STEPS_PER_FRAME
		_step_carry = 0.0  # drop the backlog rather than chase it
	for _i in steps:
		_fire_due_gem_edits()  # LOAD-GEM-REPLAY v2: fire a loaded gem's edits at the TOP of this gen's step (BEFORE the advance)
		_live.step(LIVE_STEP)
		_fire_due_immigration()  # ADR-019 S3: drain the deterministic schedule's events DUE at this gen + mark them
	# Render: throttle the heavy snapshot+parse+redraw to RENDER_HZ, decoupled from the step rate.
	_render_carry += delta
	if steps > 0 and _render_carry >= 1.0 / RENDER_HZ:
		_render_carry = 0.0
		_publish_frame()


## Pull the current snapshot from the live env + refresh the rolling history/display — the heavy per-frame work,
## throttled to RENDER_HZ by _process (stepping happens THERE, not here). Main-thread only: a future worker-thread
## migration would reintroduce the &mut aliasing hazard (every LiveSim method is &mut self) the design avoided.
func _publish_frame() -> void:
	if _live == null:
		return
	var snap = SnapshotReader.parse_bytes(_live.snapshot(LIVE_GRID.x, LIVE_GRID.y))
	if snap == null:
		return
	_snaps.append(snap)
	if _snaps.size() > LIVE_HISTORY:
		_snaps.pop_front()
	# Roll the sparkline histories: mean fitness over populated cells + the run-level allele freq from observe().
	var obs: Dictionary = _live.observe()
	_fit_history.append(_mean_pop(snap.fitness, snap.density))
	_allele_history.append(clampf(float(obs.get("allele_freq", 0.0)), 0.0, 1.0))
	if _fit_history.size() > LIVE_HISTORY:
		_fit_history.pop_front()
	if _allele_history.size() > LIVE_HISTORY:
		_allele_history.pop_front()
	if _timeline != null:
		var gens: Array = []
		for s in _snaps:
			gens.append(s.generation)
		_timeline.setup(gens)
		_timeline.set_markers(_injections)
	_show(_snaps.size() - 1)
	_refresh_oversight_panel()  # OVERSIGHT credit accrues as mineralization earns it — keep the ledger readout live
	_poll_population_alerts()  # Item (c): per-species extinction/boom flash vs the prior render frame (--live)
	if _view_mode == VIEW_RELATIONS:
		_refresh_relations()  # the FlowMatrix is per-generation — repaint the heatmap each render tick in Relations


## Drain the deterministic immigration schedule's events that are DUE at the current generation (ADR-019 S3): the
## core's fire_due_inoculations fires + journals each scheduled RegionInoculate (byte-identical to a hand-fired one,
## so save/load replay reproduces it). When ≥1 fired this tick we drop ONE immigration timeline marker at the
## current generation so a scheduled arrival is legible on the timeline just like a manual seed. Read-only w.r.t.
## biology (inv #2): GDScript only asks the core to advance its own journaled schedule + draws a marker; the core
## owns the schedule, the spawn, and the conserved `immigration` tap. No-op when the cdylib lacks the export
## (forward-compat probe, mirroring observe_species/flow_matrix) or the schedule is empty (default Sealed).
func _fire_due_immigration() -> void:
	if _live == null or not _live.has_method("fire_due_inoculations"):
		return
	var fired: int = int(_live.fire_due_inoculations())
	if fired <= 0:
		return
	var gen := int(_live.observe().get("generation", 0)) if _live.has_method("observe") else 0
	_record_tool_outcome(TOOL_INOCULATE, {
		"applied": true,
		"detail": "🦠 schedule fired ×%d (gen %d)" % [fired, gen],
		"generation": gen,
	})


# ──────────────────────────── LOAD-GEM-REPLAY v2: load + replay a saved gem ────────────────────────────────
#
# A saved gem (data/runs/gems/<…>.json — GITIGNORED, so loaded by ABSOLUTE filesystem path, NOT res://) bundles a
# reproducible run description (master_seed + roster + env + containment) AND a mid-run CRISPR edit schedule. The
# loader (a) composes the run through the SAME proven Load-Starter/menu path (set_roster/set_environment/
# set_containment/reset) and (b) asks the CORE (LiveSim.gem_edit_schedule) to RESOLVE every edit to its absolute
# generation + REAL locus + REAL species ordinal. GDScript computes NO target/gen math (inv #2) — it only moves the
# resolved ints/strings into LiveSim.apply_edit and fires each at the TOP of its gen's step, matching the harness
# capture/verify interleave (apply at loop gen == gen_abs, BEFORE that gen's Advance — fixes the prior off-by-one).


## Read a gem JSON file and configure the live run from it (the windowed "Load Gem" + the headless --gem smoke share
## this). Composes the run via the existing menu Start path (_on_menu_start → set_roster/set_environment/
## set_containment/_do_reset) then arms the CORE-resolved edit schedule. Returns false on a missing/malformed gem.
func _configure_run_from_gem(path: String) -> bool:
	if _live == null:
		return false
	var gem_json := _read_text_file(path)
	if gem_json == "":
		push_warning("Load Gem: cannot read %s" % path)
		return false
	var cfg := _gem_cfg_from_text(gem_json, path)
	if cfg.is_empty():
		return false
	# Compose + reseed the run via the SAME proven path the menu Start uses (roster keys → set_roster, temp_q/season
	# → set_environment, containment → set_containment, master_seed → reset). Mission off (a gem is a sandbox replay).
	_on_menu_start(cfg)
	# Resolve the gem's mid-run edit schedule in the CORE (NOT here — inv #2) and arm it for the live loop.
	var n := _load_gem_schedule(gem_json)
	print("LOAD GEM — %s · roster %d spp · %d scheduled edit(s)" % [path.get_file(), int((cfg.get("roster", []) as Array).size()), n])
	return true


## Build the run-config Dictionary _on_menu_start consumes from a gem's JSON TEXT. The renderer only moves inert
## strings + ints (inv #2): roster stems, the q16 temp/season knobs, the containment ordinal, the master seed. The
## roster DROPS zero-count (absent) species so the set_roster order == the harness RESOLVED-roster order — the
## SpeciesId ordinals the CORE schedule resolver returns are POSITIONAL over the non-zero entries, so they line up
## 1:1 with set_roster. NOTE on the seed: Godot's JSON reads a u64 master_seed exactly when it fits i64, else as a
## double (lossy) — fine for a renderer REPLAY (the per-species edit GATE is genome-based, not seed-driven).
func _gem_cfg_from_text(gem_json: String, src: String) -> Dictionary:
	var parsed: Variant = JSON.parse_string(gem_json)
	if typeof(parsed) != TYPE_DICTIONARY:
		push_warning("Load Gem: malformed gem JSON in %s" % src)
		return {}
	var gem: Dictionary = parsed
	var config_v: Variant = gem.get("config", null)
	if typeof(config_v) != TYPE_DICTIONARY or (config_v as Dictionary).is_empty():
		push_warning("Load Gem: gem %s has no 'config'" % src)
		return {}
	var config: Dictionary = config_v
	# config.roster is [[stem, count], …] (serde tuple → JSON 2-array). Keep order; drop zero-count (absent) species.
	var roster: Array = []
	for entry in config.get("roster", []):
		if typeof(entry) == TYPE_ARRAY and (entry as Array).size() >= 2:
			var count := int(entry[1])
			if count > 0:
				roster.append({"stem": String(entry[0]), "count": count})
	var temp_q := int(config.get("temp_q", 500))
	var active_stem := String((roster[0] as Dictionary).get("stem", "default")) if not roster.is_empty() else "default"
	return {
		"seed": int(config.get("master_seed", _seed)),
		"lat": 0.0,
		"lon": 0.0,
		"temp": clampf(float(temp_q) / 1000.0, 0.0, 1.0),
		"season": int(config.get("season", 0)),
		"entities": GEM_ENTITY_COUNT,
		"mission": false,
		"species": active_stem,
		"roster": roster,
		"containment": int(config.get("containment_level", 0)),
	}


## Resolve a gem's mid-run CRISPR edit schedule via the CORE (LiveSim.gem_edit_schedule → harness::gem_edit_schedule,
## the SAME edits_to_actions resolver the capture/verify path uses — the target/gen math is NOT reimplemented here,
## inv #2) and arm it on _gem_schedule for the live loop. Keeps only gen_abs >= 1 (a gen_abs==0 edit never fires —
## the capture loop is 1..=gens — so dropping it is exactly faithful to the harness build_journal) and stable-sorts
## by gen_abs (ties keep list order). Returns the count armed. has_method-guards the resolver: an OLDER cdylib
## without it degrades with a clear message (no edits replayed) rather than erroring. Returns 0 on any degrade.
func _load_gem_schedule(gem_json: String) -> int:
	_gem_schedule = []
	_gem_schedule_idx = 0
	if _live == null:
		return 0
	if not _live.has_method("gem_edit_schedule"):
		push_warning("LiveSim.gem_edit_schedule unavailable in this build (older cdylib) — the gem's mid-run edits will NOT replay. Rebuild crates/godot-sim to enable LOAD-GEM-REPLAY v2.")
		return 0
	var sched: Array = _live.gem_edit_schedule(gem_json)
	var rows: Array = []
	for i in sched.size():
		var e: Variant = sched[i]
		if typeof(e) == TYPE_DICTIONARY and int((e as Dictionary).get("gen_abs", 0)) >= 1:
			var d: Dictionary = (e as Dictionary).duplicate()
			d["_ord"] = i  # original index → a stable tie-break so a single forward pointer fires the harness order
			rows.append(d)
	rows.sort_custom(func(a, b):
		var ga := int((a as Dictionary).get("gen_abs", 0))
		var gb := int((b as Dictionary).get("gen_abs", 0))
		if ga != gb:
			return ga < gb
		return int((a as Dictionary).get("_ord", 0)) < int((b as Dictionary).get("_ord", 0)))
	_gem_schedule = rows
	return _gem_schedule.size()


## Fire every armed gem edit DUE at the top of the upcoming step — the harness interleave (apply at loop gen ==
## gen_abs, BEFORE that gen's Advance). env.generation == G now; this tick advances to G + LIVE_STEP, so fire each
## pending edit whose gen_abs is in (G, G+LIVE_STEP] (with LIVE_STEP==1: gen_abs == G+1). The schedule is gen_abs-
## sorted, so a single forward pointer suffices. No-op (and no observe()) when no gem is armed — hash-neutral.
func _fire_due_gem_edits() -> void:
	if _live == null or _gem_schedule_idx >= _gem_schedule.size():
		return
	var due := int(_live.observe().get("generation", 0)) + LIVE_STEP
	while _gem_schedule_idx < _gem_schedule.size():
		var e: Dictionary = _gem_schedule[_gem_schedule_idx]
		if int(e.get("gen_abs", 0)) > due:
			break
		_fire_one_gem_edit(e)
		_gem_schedule_idx += 1


## Apply ONE resolved gem edit through the unchanged LiveSim.apply_edit surface (inv #2 — the renderer only moves
## the CORE-resolved ids + guide string; the authoritative PAM/score/gate stays in crispr) and drop a timeline
## marker. Returns the core's outcome Dictionary {applied, detail, generation}.
func _fire_one_gem_edit(e: Dictionary) -> Dictionary:
	var outcome: Dictionary = _live.apply_edit(
		int(e.get("cas", 0)), int(e.get("target", 0)), str(e.get("guide", "")), int(e.get("species", 0)))
	_record_edit_outcome(outcome)
	return outcome


## SCENARIO GIF PREVIEW capture helper (tools/make_starter_gif.sh): advance a freshly-loaded gem `n` generations,
## firing each due CORE-resolved edit + scheduled immigration at the top of every gen's step — the SAME interleave
## the live _process loop runs (see _process) — then publish one frame so the following --shot captures a DEVELOPED
## state incl. the gem's edits. Renderer-only (inv #2): it only drives existing core surfaces (_live.step +
## _fire_due_gem_edits + _fire_due_immigration); no genotype→phenotype/biology is computed in GDScript. Each capture
## is a separate godot process replaying the gem to gen N, so the frame is a deterministic pure function of (gem, N).
func _advance_gem_for_shot(n: int) -> void:
	if _live == null:
		return
	for _i in n:
		_fire_due_gem_edits()      # fire a loaded gem's edits BEFORE this gen's advance (the harness interleave)
		_live.step(LIVE_STEP)
		_fire_due_immigration()    # drain the deterministic immigration schedule's events due at this gen
	_publish_frame()


## The menu's "Load Gem" → compose + replay the chosen gem (the windowed entry; _configure_run_from_gem resumes the
## live loop via _on_menu_start). The menu frees itself after emitting load_gem.
func _on_menu_load_gem(path: String) -> void:
	if _live == null:
		return
	if not _configure_run_from_gem(path):
		push_warning("Load Gem: failed to load %s" % path)


## HEADLESS --gem smoke (LOAD-GEM-REPLAY v2 fidelity gate): load a gem, drive the run gen-by-gen firing each CORE-
## resolved edit at the TOP of its gen's step (env at gen-1, BEFORE the advance — the harness build_journal
## interleave), and report APPLIED edits (apply_edit's outcome.applied == true), NOT merely dispatched, so the
## gate can SEE edit fidelity. Prints a greppable `GEM_SMOKE …` line + a human summary, then quits.
func _run_gem_smoke(path: String) -> void:
	if not _configure_run_from_gem(path):
		print("GEM_SMOKE FAIL — could not configure the run from %s" % path)
		get_tree().quit(1)
		return
	var total := _gem_schedule.size()
	var horizon := 0
	for e in _gem_schedule:
		horizon = maxi(horizon, int((e as Dictionary).get("gen_abs", 0)))
	var fired := 0
	var applied := 0
	# Drive gen-by-gen. At loop gen `g` env.generation == g-1; fire the edits scheduled at gen_abs == g BEFORE the
	# step to g — exactly the harness capture/verify interleave (which the unedited round-trip already proves).
	for g in range(1, horizon + 1):
		while _gem_schedule_idx < _gem_schedule.size() and int((_gem_schedule[_gem_schedule_idx] as Dictionary).get("gen_abs", 0)) == g:
			var e: Dictionary = _gem_schedule[_gem_schedule_idx]
			var outcome := _fire_one_gem_edit(e)
			fired += 1
			if bool(outcome.get("applied", false)):
				applied += 1
			else:
				print("GEM_SMOKE — edit NOT applied at gen %d (target %d, species %d): %s" % [
					g, int(e.get("target", 0)), int(e.get("species", 0)), str(outcome.get("detail", ""))])
			_gem_schedule_idx += 1
		_live.step(1)
	print("GEM_SMOKE fired=%d applied=%d total=%d horizon=%d" % [fired, applied, total, horizon])
	if total > 0 and applied == total:
		print("GEM SMOKE OK — every scheduled edit APPLIED at its resolved generation (%d/%d)" % [applied, total])
	elif total == 0:
		print("GEM SMOKE OK — gem carries no mid-run edits (nothing to replay)")
	else:
		print("GEM SMOKE PARTIAL — %d/%d scheduled edits applied (see lines above)" % [applied, total])
	get_tree().quit()


## Read a whole text file from an ABSOLUTE filesystem path (a gem lives in gitignored data/runs, outside res://).
## Returns "" when the file cannot be opened.
func _read_text_file(path: String) -> String:
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return ""
	var text := f.get_as_text()  # FileAccess (RefCounted) closes on free
	f.close()
	return text


## Live-mode CRISPR intervention UI (P6): pick a Cas variant / locus / guide and Inject. The renderer only
## REQUESTS the edit (LiveSim.apply_edit) — the core applies it (authoritative PAM/score/gate stay in crispr,
## inv #2); the species-granular EditAction carries no organism handle (inv #6). Hidden unless --live.
## Repopulate the locus target picker from the ACTIVE species genome (ADR-017). Called at UI build AND after a
## species change/reset, so the offered targets always match the genome `apply_edit` resolves against — E. coli's
## 136 real genes, not the plant baseline (otherwise an edit lands on a mislabeled locus).
func _populate_locus_picker() -> void:
	if _live == null or _locus_picker == null:
		return
	_locus_picker.clear()
	_locus_ids.clear()
	for l in _live.loci():
		_locus_picker.add_item(str((l as Dictionary).get("name", "locus")))
		_locus_ids.append(int((l as Dictionary).get("id", 0)))


## The unified 6-TOOL intervention palette (SP-3.6/ADR-019): a radio row of CRISPR / PCR / Antibiotic / Nutrient /
## Toxin / Inoculate, a swapped per-tool param sub-panel, the shared region brush (drag to paint — POSITION
## MATTERS), and one status
## readout. Reuses 100% of the existing brush + region plumbing; each tool issues ONE journaled Action through a
## LiveSim #[func] and reads the verdict (biology stays in the core, inv #2).
func _build_intervention_ui(ui: CanvasLayer) -> void:
	var body := _dark_panel(0.55)
	body.custom_minimum_size = Vector2(278, 0)
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 5)
	body.add_child(col)

	# Tool palette: 6 radio toggles (one active). Selecting a tool swaps its param sub-panel + retints the brush.
	var palette := HBoxContainer.new()
	palette.add_theme_constant_override("separation", 3)
	col.add_child(palette)
	var grp := ButtonGroup.new()
	# Richer 2-3 line tooltips (Item 2a): say WHAT each tool acts on. The two SUBSTRATE tools (Nutrient/Toxin)
	# paint a POOL/CHEM channel — no organism is targeted; the three ORGANISM tools (CRISPR/PCR/Antibiotic) act on
	# resident orgs in the disc; Inoculate seeds a NEW contaminant. POSITION MATTERS for every tool (the brush disc
	# → RegionSpec → the core picks cells/orgs). Tooltip text only — biology stays in the core (inv #2).
	var tool_specs := [
		{"glyph": "🧬", "name": "CRISPR", "tip":
			"Apply a CRISPR edit to the RESIDENT species in the painted disc.\nThe core resolves PAM / on- & off-target score / gate (inv #2).\nPOSITION MATTERS — only orgs inside the brush cell are edited."},
		{"glyph": "🧫", "name": "PCR", "tip":
			"Amplify a RESIDENT species — adds faithful clones at the disc.\nPick the target species + clone count + per-clone J endowment.\nActs on organisms; POSITION MATTERS (clones spawn in the disc)."},
		{"glyph": "💊", "name": "Antibiotic", "tip":
			"Kill a FRACTION of a resident species in the disc (a cull).\nPick the target species + cull strength; survivors persist.\nActs on organisms; POSITION MATTERS (only the disc is dosed)."},
		{"glyph": "🌱", "name": "Nutrient", "tip":
			"Feed a POOL / CHEM channel (light / free-nutrient / detritus).\nTargets the ENVIRONMENT plane, NOT an organism — orgs benefit\nindirectly via uptake. POSITION MATTERS (only the disc is fed)."},
		{"glyph": "☣", "name": "Toxin", "tip":
			"Spike a CHEM channel (toxin / kin / alarm) in the disc.\nTargets the ENVIRONMENT plane, NOT an organism — it suppresses\nuptake / signals. POSITION MATTERS (only the disc is spiked)."},
		{"glyph": "🦠", "name": "Inoculate", "tip":
			"Seed a NEW contaminant SpeciesSpec at the disc (ADR-019).\nManual seeds work at ANY containment (even Sealed); SCHEDULED\nimmigrants need Containment > Sealed. POSITION MATTERS (the disc)."},
	]
	_tool_buttons.clear()
	for i in tool_specs.size():
		var spec: Dictionary = tool_specs[i]
		var b := Button.new()
		b.text = str(spec["glyph"])
		b.tooltip_text = str(spec["name"]) + "\n" + str(spec["tip"])
		b.toggle_mode = true
		b.button_group = grp
		b.custom_minimum_size = Vector2(34, 0)
		b.pressed.connect(_on_tool_selected.bind(i))
		palette.add_child(b)
		_tool_buttons.append(b)

	# Per-tool param sub-panels (only the active tool's is visible). Built into one stack; visibility swaps.
	var params_stack := VBoxContainer.new()
	col.add_child(params_stack)
	_tool_panels.clear()
	_tool_panels.resize(6)
	_tool_panels[TOOL_CRISPR] = _build_crispr_params()
	_tool_panels[TOOL_PCR] = _build_pcr_params()
	_tool_panels[TOOL_ANTIBIOTIC] = _build_cull_params()
	_tool_panels[TOOL_NUTRIENT] = _build_nutrient_params()
	_tool_panels[TOOL_TOXIN] = _build_toxin_params()
	_tool_panels[TOOL_INOCULATE] = _build_inoculate_params()
	for p in _tool_panels:
		params_stack.add_child(p)

	# Action row: a brush toggle (shared by every tool) + a whole-species CRISPR inject (CRISPR-only convenience).
	var btns := HBoxContainer.new()
	btns.add_theme_constant_override("separation", 6)
	col.add_child(btns)
	_brush_button = Button.new()
	_brush_button.text = "🖌 Brush: off"
	_brush_button.toggle_mode = true
	_brush_button.tooltip_text = "Paint the active tool on the map (key B); drag to paint; wheel = radius"
	_brush_button.toggled.connect(_on_brush_toggled)
	btns.add_child(_brush_button)

	_inject_status = _dim_label("")
	_inject_status.custom_minimum_size = Vector2(266, 0)
	_inject_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	col.add_child(_inject_status)

	if _live != null:
		for v in _live.cas_variants():
			_cas_picker.add_item(str((v as Dictionary).get("name", "cas")))
			_cas_ids.append(int((v as Dictionary).get("id", 0)))
		_populate_locus_picker()
		_populate_species_pickers()

	_tool_buttons[TOOL_CRISPR].set_pressed_no_signal(true)
	_select_tool(TOOL_CRISPR)  # default to CRISPR; shows its params + green brush tint

	var fs := _field_screen_size()
	_intervention_panel = PanelChrome.new()
	_intervention_panel.setup("🧪 INTERVENE", body, ui, Vector2(maxf(240.0, fs.x - 290.0), 176.0), _pill_rail)  # y=176 clears the always-on VIEW+SCOPE panel
	_intervention_panel.set_active(_live != null)


## The CONTAINMENT panel (ADR-019 S3): the ISO-14644 ContainmentLevel ladder selector + the consortium menu
## (which baked contaminants ride the deterministic immigration schedule) + the schedule pressure params. Dirtier
## level → more contamination pressure; the schedule itself is derived IN THE CORE off the off-stream IMMG family
## (zero SimRng draws, inv #3) and journaled, so it replays. GDScript here only collects the level + checked keys +
## ints and pushes them to LiveSim.set_containment before reset — biology stays in the core (inv #2). Default
## Sealed → empty schedule → hash-neutral. The pinned literal 0x47a0_3c8f_6701_f240 is untouched (renderer-only).
func _build_contamination_ui(ui: CanvasLayer) -> void:
	var body := _dark_panel(0.55)
	body.custom_minimum_size = Vector2(260, 0)
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 5)
	body.add_child(col)

	# ContainmentLevel ladder selector (ISO 14644: Sealed/OFF → Clean → Lab → Open/dirty). Dirtier = more pressure.
	var lvl_row := HBoxContainer.new()
	lvl_row.add_child(_dim_label("Containment:"))
	_containment_selector = OptionButton.new()
	for label in CONTAINMENT_LABELS:
		_containment_selector.add_item(label)
	_containment_selector.select(_containment_level)
	_containment_selector.item_selected.connect(_on_containment_level_selected)
	lvl_row.add_child(_containment_selector)
	col.add_child(lvl_row)

	# Consortium menu: one CheckBox per baked contaminant key. Checked keys ride the schedule (registered with the
	# core at reset). These are just file stems — no biology in GDScript (inv #2). Item 2b: a header that explains
	# WHEN these arrive (scheduled airborne immigration only fires at Containment > Sealed) + a per-species role
	# blurb tooltip pulled from the codex (read-only projection; falls back to a static one-liner when the codex is
	# absent), and a note that the Inoculate brush still works manually even at Sealed.
	var consortium_hdr := _dim_label("Airborne immigrants (arrive on schedule when Containment > Sealed):")
	consortium_hdr.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	consortium_hdr.custom_minimum_size = Vector2(248, 0)
	col.add_child(consortium_hdr)
	_consortium_checks.clear()
	for key in _discover_contaminant_keys():
		var cb := CheckBox.new()
		cb.text = key
		cb.tooltip_text = _consortium_blurb(key)
		col.add_child(cb)
		_consortium_checks[key] = cb
	var inoc_note := _dim_label("Manual Inoculate (🦠 brush) seeds at ANY containment, even Sealed; only the scheduled immigrants above need Containment > Sealed.")
	inoc_note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	inoc_note.custom_minimum_size = Vector2(248, 0)
	inoc_note.add_theme_color_override("font_color", Color(0.6, 0.66, 0.6))
	col.add_child(inoc_note)

	# Variant Lab — Slice C: the SAVED VARIANTS section (mirrors the consortium menu above). Each variant the player
	# saved in the SPECIMEN view (💾 Save variant) gets a 🌱 Reseed affordance that registers it with the core (once,
	# tracked like the consortium) + arms it as the ACTIVE Inoculate payload — the next Inoculate brush stroke seeds
	# THAT variant into the painted disc, reusing 100% of the inoculate machinery. Manual seeds work at ANY
	# containment (ADR-019). The list is rebuilt by _refresh_saved_variants_section on each save. GDScript moves only
	# the inert JSON + name (inv #2).
	var saved_hdr := _dim_label("Saved variants (🌱 Reseed → arm, then paint with the 🦠 Inoculate brush):")
	saved_hdr.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	saved_hdr.custom_minimum_size = Vector2(248, 0)
	col.add_child(saved_hdr)
	_saved_variants_box = VBoxContainer.new()
	_saved_variants_box.add_theme_constant_override("separation", 3)
	col.add_child(_saved_variants_box)
	_refresh_saved_variants_section()

	# Schedule pressure: per-immigrant J endowment (radius + horizon are fixed defaults; the level scales frequency
	# in the core). The disc/horizon stay constant so the level is the single legible knob.
	var ej_row := HBoxContainer.new()
	ej_row.add_child(_dim_label("Endow J:"))
	_containment_endow = _make_spin(1000, 100000000, 1000, 120000)
	ej_row.add_child(_containment_endow)
	col.add_child(ej_row)

	# Apply: push the current level + consortium + params to the core (stored on the binding so reset re-applies
	# it), then RE-RESET so the schedule re-expands deterministically from (seed, level, consortium). The handler
	# is _on_apply_containment_pressed.
	var apply_btn := Button.new()
	apply_btn.text = "Apply + reset schedule"
	apply_btn.tooltip_text = "Re-derive the deterministic immigration schedule from seed + level + consortium"
	apply_btn.pressed.connect(_on_apply_containment_pressed)
	col.add_child(apply_btn)

	var fs := _field_screen_size()
	_containment_panel = PanelChrome.new()
	_containment_panel.setup("🦠 CONTAMINATION", body, ui, Vector2(maxf(240.0, fs.x - 290.0), 380.0), _pill_rail)  # below the lowered INTERVENE panel
	_containment_panel.set_active(_live != null)


## The OVERSIGHT panel (ADR-017 S4–S6): the in-game earn→PREVIEW→SPEND surface over the harness CreditLedger. The
## player picks a target species + a growth_ratio_q (the FBA-predicted growth permille of the edited strain), PREVIEWS
## the demand factor it would map to WITHOUT spending (preview_ecoli_edit — read-only, zero SimRng, off-hash), then
## COMMITS when credit suffices (commit_ecoli_edit — the core runs the spend gate, journals the Request/Commit pair,
## and applies the per-species factor; an Applied marker lands on the timeline). The ledger readout (credit/cost/
## accrual) refreshes live from oversight_state() as mineralization earns credit. Renderer-only (inv #2): GDScript
## moves only ints + reads the marshaled VarDictionary — the economy/FBA/spend gate live in the harness/core. Every
## #[func] is has_method-guarded so an older cdylib / file replay degrades gracefully (the panel stays inactive and
## the handlers no-op with a message). Hash-neutral: preview/state draw zero RNG; a commit journals the SAME pair the
## headless OversightEpisode emits, so a saved session replays the deep edit bit-identically (inv #3).
func _build_oversight_ui(ui: CanvasLayer) -> void:
	var body := _dark_panel(0.55)
	body.custom_minimum_size = Vector2(272, 0)
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 5)
	body.add_child(col)

	# Ledger readout (credit / cost / accrued / per-gen cap / committed count). Refreshed live by
	# _refresh_oversight_panel() from oversight_state() — credit accrues as the FlowMatrix mineralizes (S4 economy).
	_oversight_credit_label = _dim_label("")
	_oversight_credit_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_oversight_credit_label.custom_minimum_size = Vector2(258, 0)
	col.add_child(_oversight_credit_label)

	# Target species (E. coli + any editable species), SpeciesId-ordered (inv #3); item metadata = the raw SpeciesId
	# ordinal the core resolves (the same u16-scaffold convention the CRISPR/PCR pickers use).
	var sp_row := HBoxContainer.new()
	sp_row.add_child(_dim_label("Species:"))
	_oversight_species = OptionButton.new()
	for row in _panel_species_list():
		var d: Dictionary = row
		_oversight_species.add_item(str(d.get("name", "species")))
		_oversight_species.set_item_metadata(_oversight_species.item_count - 1, int(d.get("species_id", 0)))
	sp_row.add_child(_oversight_species)
	col.add_child(sp_row)

	# growth_ratio_q control: the FBA growth-ratio PERMILLE (1000 = wild-type / no-op, 0 = growth-lethal knockout).
	# Lower = stronger loss-of-function → a smaller demand factor (toward 0.5×). Just an int — biology stays in core.
	var q_row := HBoxContainer.new()
	q_row.add_child(_dim_label("Growth ratio q:"))
	_oversight_ratio = _make_spin(0, 1000, 10, 0)
	_oversight_ratio.tooltip_text = "FBA growth-ratio permille: 1000 = wild-type (no-op), 0 = growth-lethal knockout.\nLower = stronger loss-of-function → a smaller demand factor (toward 0.5×). The core maps it (inv #2)."
	q_row.add_child(_oversight_ratio)
	col.add_child(q_row)

	# PREVIEW (read-only, spends nothing): the predicted vs current demand factor + whether the spend gate can afford it.
	var preview_btn := Button.new()
	preview_btn.text = "🔎 Preview"
	preview_btn.tooltip_text = "Show the demand factor this growth_ratio_q WOULD map to — without spending credit (the core computes it, inv #2)."
	preview_btn.pressed.connect(_on_oversight_preview_pressed)
	col.add_child(preview_btn)
	_oversight_preview_label = _dim_label("Preview to see the predicted outcome before spending.")
	_oversight_preview_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_oversight_preview_label.custom_minimum_size = Vector2(258, 0)
	col.add_child(_oversight_preview_label)

	# COMMIT (spends one edit's credit): the core runs the spend gate, journals the Request/Commit pair, and applies
	# the per-species factor; an Applied marker lands on the timeline. Enabled only while `affordable` (set in refresh).
	_oversight_commit_button = Button.new()
	_oversight_commit_button.text = "💳 Commit edit"
	_oversight_commit_button.tooltip_text = "Spend one edit's credit + commit the deep edit (the core runs the spend gate + journals it), then mark it on the timeline."
	_oversight_commit_button.pressed.connect(_on_oversight_commit_pressed)
	col.add_child(_oversight_commit_button)

	_oversight_status = _dim_label("")
	_oversight_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_oversight_status.custom_minimum_size = Vector2(258, 0)
	col.add_child(_oversight_status)

	var fs := _field_screen_size()
	_oversight_panel = PanelChrome.new()
	_oversight_panel.setup("⚖ OVERSIGHT", body, ui, Vector2(maxf(220.0, fs.x - 580.0), 176.0), _pill_rail)  # left of INTERVENE
	# has_method-guarded (inv #2): an older cdylib / file replay lacks the oversight #[func]s → the panel stays
	# inactive and its handlers no-op with a graceful message rather than erroring.
	_oversight_panel.set_active(_live != null and _live.has_method("oversight_state"))
	_refresh_oversight_panel()  # seed the ledger readout + the Commit-enabled state


## Pull the OVERSIGHT ledger (oversight_state()) into the credit readout + gate the Commit button on `affordable`.
## Called on build, every live render tick (credit accrues as mineralization earns it), and after a commit (to reflect
## the spend). Read-only marshalling (inv #2): a Dictionary of ints the core marshaled — zero biology in GDScript.
## Degrades gracefully when the oversight #[func]s are absent (older cdylib / file replay): a one-line note + a
## disabled Commit button.
func _refresh_oversight_panel() -> void:
	if _oversight_credit_label == null:
		return
	if _live == null or not _live.has_method("oversight_state"):
		_oversight_credit_label.text = "Credit ledger unavailable (file replay / older build)"
		_oversight_credit_label.add_theme_color_override("font_color", Color(0.7, 0.72, 0.7))
		if _oversight_commit_button != null:
			_oversight_commit_button.disabled = true
		return
	var s: Dictionary = _live.oversight_state()
	var credit := int(s.get("credit", 0))
	var cost := int(s.get("edit_cost", 0))
	var accrued := int(s.get("accrued_total", 0))
	var cap := int(s.get("per_gen_cap", 0))
	var affordable := bool(s.get("affordable", false))
	var committed: Array = s.get("committed", [])
	_oversight_credit_label.text = "Credit %d / cost %d   ·   accrued %d   ·   +%d/gen   ·   %d committed" % [
		credit, cost, accrued, cap, committed.size()]
	_oversight_credit_label.add_theme_color_override(
		"font_color", Color(0.6, 0.9, 0.62) if affordable else Color(0.86, 0.8, 0.5))
	if _oversight_commit_button != null:
		_oversight_commit_button.disabled = not affordable


## PREVIEW the deep edit without spending (ADR-017 S6): map the picked growth_ratio_q to its demand factor + show
## the current factor + affordability. Read-only (inv #2): preview_ecoli_edit draws zero SimRng and never mutates.
func _on_oversight_preview_pressed() -> void:
	if _oversight_preview_label == null:
		return
	if _live == null or not _live.has_method("preview_ecoli_edit"):
		_oversight_preview_label.text = "Preview unavailable (file replay / older build)"
		_oversight_preview_label.add_theme_color_override("font_color", Color(0.86, 0.8, 0.5))
		return
	var sid := _picker_species_id(_oversight_species)
	var q := int(_oversight_ratio.value) if _oversight_ratio != null else 1000
	var p: Dictionary = _live.preview_ecoli_edit(sid, q)
	var predicted := int(p.get("predicted_factor_q", 1000))
	var current := int(p.get("current_factor_q", 1000))
	var affordable := bool(p.get("affordable", false))
	# Factors are permille (1000 = 1.0×); /1000 surfaces the human-readable demand multiplier in [0.5×, 1.5×].
	_oversight_preview_label.text = "q=%d → demand factor %.3f× (now %.3f×)%s" % [
		q, float(predicted) / 1000.0, float(current) / 1000.0,
		"" if affordable else "   · NOT affordable yet"]
	_oversight_preview_label.add_theme_color_override(
		"font_color", Color(0.7, 0.86, 0.95) if affordable else Color(0.86, 0.8, 0.5))


## COMMIT the deep edit (ADR-017 S6 — the load-bearing OVERSIGHT wire): the core runs the spend gate, journals the
## Request/Commit pair, and applies the per-species factor; on accept we drop an Applied timeline marker and reflect
## the spent credit. due_epoch 0 → the core Tick-counts the earliest legal commit epoch (epoch_of(gen)+lead); never
## wall-clock (inv #3). Renderer requests, core decides (inv #2): GDScript reads only the marshaled verdict ints.
func _on_oversight_commit_pressed() -> void:
	if _live == null or not _live.has_method("commit_ecoli_edit"):
		_set_oversight_status("✗ Commit unavailable (file replay / older build)", false)
		return
	var sid := _picker_species_id(_oversight_species)
	var q := int(_oversight_ratio.value) if _oversight_ratio != null else 1000
	var outcome: Dictionary = _live.commit_ecoli_edit(sid, q, 0)
	var applied := bool(outcome.get("applied", false))
	if applied:
		var factor := int(outcome.get("factor_q", 1000))
		var due_epoch := int(outcome.get("due_epoch", 0))
		var req_id := int(outcome.get("req_id", 0))
		# Mark it where the player committed (the journaled Request/Commit pair lands here); the Tick-counted
		# due_epoch the edit takes effect at is surfaced in the marker label (the timeline axis is per-generation).
		var gen := int(_live.observe().get("generation", 0)) if _live.has_method("observe") else 0
		_record_oversight_marker(
			gen, "OVERSIGHT q=%d → %.3f× · due epoch %d · req %d" % [q, float(factor) / 1000.0, due_epoch, req_id])
		_set_oversight_status(
			"✓ committed q=%d → %.3f× (req %d, due epoch %d)" % [q, float(factor) / 1000.0, req_id, due_epoch], true)
	else:
		_set_oversight_status("✗ %s" % str(outcome.get("reason", "rejected")), false)
	_refresh_oversight_panel()  # reflect the spent credit + re-gate the Commit button immediately


## Drop an Applied OVERSIGHT marker on the timeline (tool tag "oversight" → its own colour/glyph in timeline.gd;
## an older timeline falls back to the CRISPR look). Mirrors _record_tool_outcome's marker shape but stays off the
## 6-palette TOOL_KEYS index (OVERSIGHT is not a brush tool). Read-only (inv #2): a projection of the core's verdict.
func _record_oversight_marker(generation: int, label: String) -> void:
	_injections.append({
		"generation": generation,
		"tool": "oversight",
		"applied": true,
		"label": label,
	})
	if _timeline != null:
		_timeline.set_markers(_injections)


## Show an OVERSIGHT verdict in its own status line (green ok / red fail). Separate from the INTERVENE status so the
## two panels never clobber each other's last message.
func _set_oversight_status(text: String, ok: bool) -> void:
	if _oversight_status != null:
		_oversight_status.text = text
		_oversight_status.add_theme_color_override(
			"font_color", Color(0.5, 0.92, 0.52) if ok else Color(0.96, 0.55, 0.5))


## The ContainmentLevel selector hook: store the level (0 Sealed · 1 Clean · 2 Lab · 3 Open). The schedule is only
## (re)derived on Apply (a reset is required to re-expand it deterministically), so this just records the choice.
func _on_containment_level_selected(idx: int) -> void:
	_containment_level = clampi(idx, 0, CONTAINMENT_LABELS.size() - 1)


## Apply the containment config + re-derive the immigration schedule (ADR-019 S3). set_containment stores the level
## + consortium config on the live env, and the schedule expands deterministically at reset off the off-stream IMMG
## family; so we push the config, then re-reset from the SAME seed (inv #3 — same seed + level + consortium →
## identical schedule). Registers each checked contaminant first so a fired schedule event can resolve its key.
func _on_apply_containment_pressed() -> void:
	if _live == null or not _live.has_method("set_containment"):
		_flash_status("✗ Containment unsupported by this build", false)
		return
	# Collect the checked consortium keys whose res:// JSON exists (file-existence check only — the core does the
	# real serde/build at register time; no biology in GDScript, inv #2).
	var keys := PackedStringArray()
	for key in _discover_contaminant_keys():
		var cb: CheckBox = _consortium_checks.get(key, null)
		if cb != null and cb.button_pressed:
			if FileAccess.file_exists("res://data/species/%s.json" % key):
				keys.append(key)
			else:
				push_warning("contaminant '%s' skipped (res:// JSON missing)" % key)
	var endow := int(_containment_endow.value) if _containment_endow != null else 0
	# Push the config (stored on the binding so reset re-applies it), then re-reset so the schedule re-expands
	# deterministically from (seed, level, consortium) off the off-stream IMMG family (inv #3).
	_live.set_containment(_containment_level, keys, _containment_radius, endow, _containment_horizon)
	_do_reset(_seed)
	# The fresh reset rebuilt the core env (empty consortium) + cleared the registry. Register the consortium NOW
	# so a fired schedule event (or a post-reset manual seed) can resolve its key against a loaded genome.
	for key in keys:
		_ensure_contaminant_registered(str(key))
	var lvl_name := str(CONTAINMENT_LABELS[_containment_level]) if _containment_level < CONTAINMENT_LABELS.size() else "?"
	_flash_status("🦠 containment → %s · %d in consortium" % [lvl_name, keys.size()], true)


## Variant Lab — Slice C: (re)build the CONTAMINATION panel's "Saved variants" list from _saved_variants. One row per
## saved variant: a morphotype glyph (key+role) + the name + a 🌱 Reseed button (reads "Armed ✓" for the armed one).
## Pure renderer state (inv #2). Safe before the panel exists (no-op) and on an empty store (shows a hint).
func _refresh_saved_variants_section() -> void:
	if _saved_variants_box == null:
		return
	for c in _saved_variants_box.get_children():
		_saved_variants_box.remove_child(c)
		c.queue_free()
	if _saved_variants.is_empty():
		var empty := _dim_label("— none yet · save one in the 🌱 SPECIMEN view (💾 Save variant)")
		empty.add_theme_color_override("font_color", Color(0.55, 0.58, 0.55))
		_saved_variants_box.add_child(empty)
		return
	for i in _saved_variants.size():
		var v: Dictionary = _saved_variants[i]
		var rowh := HBoxContainer.new()
		rowh.add_theme_constant_override("separation", 6)
		var glyph := Label.new()
		glyph.text = GlyphFactory.emoji_for(str(v.get("key", "default")), str(v.get("role", "")))
		rowh.add_child(glyph)
		var nm := Label.new()
		nm.text = str(v.get("name", "variant"))
		nm.custom_minimum_size = Vector2(132, 0)
		nm.clip_text = true
		nm.add_theme_font_size_override("font_size", 11)
		nm.add_theme_color_override("font_color", Color(0.9, 0.94, 0.9))
		rowh.add_child(nm)
		var btn := Button.new()
		btn.text = "🌱 Armed ✓" if i == _armed_variant_idx else "🌱 Reseed"
		btn.tooltip_text = "Register + arm this variant as the Inoculate payload, then paint the 🦠 Inoculate brush to seed it"
		btn.pressed.connect(_on_reseed_variant_pressed.bind(i))
		rowh.add_child(btn)
		_saved_variants_box.add_child(rowh)


## Variant Lab — Slice C: REGISTER + ARM a saved variant as the active Inoculate payload (ADR-019 manual seed —
## works at ANY containment). Registers the variant's opaque JSON with the core (the same register_contaminant_json
## res:// boundary the consortium uses — inv #2: GDScript hands over inert text, the core does serde + build), tracks
## it like _registered_contaminants, then arms the Inoculate tool + brush so the NEXT stroke seeds this variant. The
## core REPLACES a same-key entry, so reseeding a different variant of the same species overwrites cleanly.
func _on_reseed_variant_pressed(idx: int) -> void:
	if idx < 0 or idx >= _saved_variants.size():
		return
	if _live == null or not _live.has_method("register_contaminant_json") or not _live.has_method("inoculate"):
		_flash_status("✗ Reseed unsupported by this build", false)
		return
	var v: Dictionary = _saved_variants[idx]
	var key := str(v.get("key", ""))
	if key == "" or not _live.register_contaminant_json(str(v.get("json", ""))):
		_flash_status("✗ variant '%s' failed to register" % v.get("name", "?"), false)
		return
	_registered_contaminants[key] = true  # tracked like the consortium (idempotent across reseeds)
	_armed_variant_idx = idx
	# Arm the Inoculate brush: select the tool, reflect the palette radio, and turn the brush on so the next stroke
	# seeds this variant. _select_tool also swaps the visible per-tool param panel.
	if _tool_buttons.size() == TOOL_KEYS.size():
		_tool_buttons[TOOL_INOCULATE].set_pressed_no_signal(true)
	_select_tool(TOOL_INOCULATE)
	_set_brush_mode(true)
	_refresh_saved_variants_section()  # repaint so the armed row reads "Armed ✓"
	_flash_status("🌱 armed '%s' — paint the 🦠 Inoculate brush to seed it" % v.get("name", "?"), true)


## CRISPR param sub-panel: the EXISTING Cas / Locus / Guide pickers verbatim (fires Action::ApplyEditRegion).
func _build_crispr_params() -> VBoxContainer:
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 4)
	# Variant-Lab A: the TARGET-SPECIES picker for the whole-species edit (mirrors _pcr_species in the PCR panel;
	# populated by _populate_species_pickers from observe_species(), SpeciesId order). The chosen ordinal flows
	# through to LiveSim.apply_edit; with no explicit selection it resolves to species 0 (the resident primary).
	var r0 := HBoxContainer.new()
	r0.add_child(_dim_label("Species:"))
	_crispr_species = OptionButton.new()
	r0.add_child(_crispr_species)
	col.add_child(r0)
	var r1 := HBoxContainer.new()
	r1.add_child(_dim_label("Cas:"))
	_cas_picker = OptionButton.new()
	r1.add_child(_cas_picker)
	col.add_child(r1)
	var r2 := HBoxContainer.new()
	r2.add_child(_dim_label("Locus:"))
	_locus_picker = OptionButton.new()
	r2.add_child(_locus_picker)
	col.add_child(r2)
	var r3 := HBoxContainer.new()
	r3.add_child(_dim_label("Guide:"))
	_guide_edit = LineEdit.new()
	_guide_edit.text = "ACGTGGACGTTTTAGGCCGG"
	_guide_edit.custom_minimum_size = Vector2(150, 0)
	_guide_edit.text_submitted.connect(_on_guide_submitted)
	r3.add_child(_guide_edit)
	col.add_child(r3)
	# Item 1 (UX): an EXPLICIT whole-species inject button. The whole-species CRISPR edit — the only edit that
	# appends a NEW specimen variant — used to fire ONLY on Enter in the Guide field (undiscoverable). This button
	# makes it visible; it calls the same _on_inject_pressed the Enter hook does. It lives in the CRISPR sub-panel so
	# it's contextual (shown only while the CRISPR tool is active), distinct from the shared region BRUSH below.
	# Renderer-only (inv #2): it only asks the core to apply the edit it already exposes; biology stays in the core.
	_inject_button = Button.new()
	_inject_button.text = "💉 Inject (whole species)"
	_inject_button.tooltip_text = "Apply this Cas / Locus / Guide to the ENTIRE active species (not a region).\nThis is the edit that appends a new variant to the specimen view.\n(Same as pressing Enter in the Guide field.)"
	_inject_button.pressed.connect(_on_inject_pressed)
	col.add_child(_inject_button)
	return col


## PCR param sub-panel: target species + clone count + per-clone J endowment (fires LiveSim.pcr_amplify).
func _build_pcr_params() -> VBoxContainer:
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 4)
	var r1 := HBoxContainer.new()
	r1.add_child(_dim_label("Species:"))
	_pcr_species = OptionButton.new()
	r1.add_child(_pcr_species)
	col.add_child(r1)
	var r2 := HBoxContainer.new()
	r2.add_child(_dim_label("Clones:"))
	_pcr_count = _make_spin(1, 256, 1, 8)
	r2.add_child(_pcr_count)
	col.add_child(r2)
	var r3 := HBoxContainer.new()
	r3.add_child(_dim_label("Endow J:"))
	_pcr_endow = _make_spin(1000, 100000000, 1000, 200000)
	r3.add_child(_pcr_endow)
	col.add_child(r3)
	return col


## Antibiotic (cull) param sub-panel: target species + a permille [0,1000] kill-fraction (fires LiveSim.cull).
func _build_cull_params() -> VBoxContainer:
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 4)
	var r1 := HBoxContainer.new()
	r1.add_child(_dim_label("Species:"))
	_cull_species = OptionButton.new()
	r1.add_child(_cull_species)
	col.add_child(r1)
	var r2 := HBoxContainer.new()
	r2.add_child(_dim_label("Kill:"))
	_cull_strength = HSlider.new()
	_cull_strength.min_value = 0
	_cull_strength.max_value = 1000
	_cull_strength.step = 10
	_cull_strength.value = 500
	_cull_strength.custom_minimum_size = Vector2(120, 0)
	_cull_strength.value_changed.connect(_on_cull_strength_changed)
	r2.add_child(_cull_strength)
	_cull_strength_label = _dim_label("50%")
	r2.add_child(_cull_strength_label)
	col.add_child(r2)
	return col


## Nutrient param sub-panel: a pool-plane channel {Light, Nutrient, Detritus} + the J amount (fires LiveSim.nutrient).
func _build_nutrient_params() -> VBoxContainer:
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 4)
	var r1 := HBoxContainer.new()
	r1.add_child(_dim_label("Channel:"))
	_nutrient_channel = OptionButton.new()
	_nutrient_channel.add_item("Light")      # 0
	_nutrient_channel.add_item("Nutrient")   # 1 (free_nutrient)
	_nutrient_channel.add_item("Detritus")   # 2
	_nutrient_channel.select(1)
	r1.add_child(_nutrient_channel)
	col.add_child(r1)
	var r2 := HBoxContainer.new()
	r2.add_child(_dim_label("Amount J:"))
	_nutrient_amount = _make_spin(1000, 100000000, 1000, 800000)
	r2.add_child(_nutrient_amount)
	col.add_child(r2)
	return col


## Toxin param sub-panel: a chem-field channel {Toxin, Kin, Alarm} + the milli amount (fires LiveSim.toxin).
func _build_toxin_params() -> VBoxContainer:
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 4)
	var r1 := HBoxContainer.new()
	r1.add_child(_dim_label("Channel:"))
	_toxin_channel = OptionButton.new()
	_toxin_channel.add_item("Toxin")  # 0
	_toxin_channel.add_item("Kin")    # 1
	_toxin_channel.add_item("Alarm")  # 2
	_toxin_channel.select(0)
	r1.add_child(_toxin_channel)
	col.add_child(r1)
	var r2 := HBoxContainer.new()
	r2.add_child(_dim_label("Amount:"))
	_toxin_amount = _make_spin(1000, 100000000, 1000, 500000)
	r2.add_child(_toxin_amount)
	col.add_child(r2)
	return col


## Inoculate (ADR-019 S3 contamination seed) param sub-panel: which baked contaminant to drop + the per-disc
## organism count + per-organism J endowment (fires LiveSim.inoculate, J minted from the core's `immigration`
## tap). The species picker offers the kebab contaminant keys; on dispatch the JSON is lazily registered via
## register_contaminant_json (the res:// boundary, inv #2). Biology stays in the core — this is just file stems +
## ints. POSITION MATTERS: the brush cell becomes the RegionInoculate disc centre.
func _build_inoculate_params() -> VBoxContainer:
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 4)
	var r1 := HBoxContainer.new()
	r1.add_child(_dim_label("Contaminant:"))
	_inoc_species = OptionButton.new()
	for key in _discover_contaminant_keys():
		_inoc_species.add_item(key)
		_inoc_species.set_item_metadata(_inoc_species.item_count - 1, key)
	r1.add_child(_inoc_species)
	col.add_child(r1)
	var r2 := HBoxContainer.new()
	r2.add_child(_dim_label("Count:"))
	_inoc_count = _make_spin(1, 256, 1, 12)
	r2.add_child(_inoc_count)
	col.add_child(r2)
	var r3 := HBoxContainer.new()
	r3.add_child(_dim_label("Endow J:"))
	_inoc_endow = _make_spin(1000, 100000000, 1000, 150000)
	r3.add_child(_inoc_endow)
	col.add_child(r3)
	return col


## A configured SpinBox (helper for the per-tool integer params). Renderer-only widget plumbing.
func _make_spin(lo: float, hi: float, step: float, val: float) -> SpinBox:
	var s := SpinBox.new()
	s.min_value = lo
	s.max_value = hi
	s.step = step
	s.value = val
	s.custom_minimum_size = Vector2(120, 0)
	return s


## Fill the per-tool target-species OptionButtons from the live per-species observation (observe_species() order =
## SpeciesId order, inv #3). The item's metadata carries the raw SpeciesId ordinal the core resolves (the
## RequestEcoliEdit / RegionInoculate u16-scaffold convention). Called at UI build + after a species change/reset.
func _populate_species_pickers() -> void:
	for ob in [_crispr_species, _pcr_species, _cull_species]:
		if ob == null:
			continue
		var prev: int = ob.selected
		ob.clear()
		for row in _panel_species_list():
			var d: Dictionary = row
			ob.add_item(str(d.get("name", "species")))
			ob.set_item_metadata(ob.item_count - 1, int(d.get("species_id", 0)))
		if prev >= 0 and prev < ob.item_count:
			ob.select(prev)


## The selected raw SpeciesId ordinal in a species OptionButton (the u16 scaffold the core resolves at the step
## boundary). Defaults to 0 when nothing is selected.
func _picker_species_id(ob: OptionButton) -> int:
	if ob == null or ob.selected < 0:
		return 0
	return int(ob.get_item_metadata(ob.selected))


## Whether a species picker has an EXPLICIT, valid target selected (Item 2c). Without this guard PCR/Antibiotic
## silently default to species 0 (implicit) and a no-selection click reads as a successful hit on the wrong
## species — a silent mistarget. Returns false for a null/empty/unselected picker so the caller can surface
## feedback instead of dispatching. Pure read of UI state (inv #2).
func _has_target_species(ob: OptionButton) -> bool:
	return ob != null and ob.item_count > 0 and ob.selected >= 0


## Surface a clear "no target species selected" verdict for a target-picker tool (Item 2c): flashes the status
## line + drops a NOT-applied timeline marker (via _record_tool_outcome) so the player gets the same feedback
## channel a real outcome uses, instead of a silent no-op. Renderer-only (inv #2).
func _record_no_target(tool: int) -> void:
	_record_tool_outcome(tool, {
		"applied": false,
		"detail": "no target species selected — pick one in the %s panel" % TOOL_KEYS[tool],
		"generation": int(_live.observe().get("generation", 0)) if (_live != null and _live.has_method("observe")) else 0,
	})


## A human-readable label for the species a picker resolves to (R2-minor: surface the RESOLVED target so a
## reordered roster / an implicit default-to-0 is legible in the status line, not a silent mistarget). Maps the
## picker's selected SpeciesId metadata back to its name+key via _panel_species_list (SpeciesId-ordered). When the
## picker has no explicit selection it resolves to species 0 — the label says so ("species 0 (default)"). Pure
## read of already-observed core data (inv #2); the core still owns the actual targeting.
func _picker_target_label(ob: OptionButton) -> String:
	var sid := _picker_species_id(ob)
	var implicit := (ob == null or ob.selected < 0)
	for row in _panel_species_list():
		var d: Dictionary = row
		if int(d.get("species_id", -1)) == sid:
			var nm := str(d.get("name", "species"))
			var key := str(d.get("key", "default"))
			return "%s [%s]%s" % [nm, key, "  (implicit default)" if implicit else ""]
	return "species %d%s" % [sid, "  (implicit default)" if implicit else ""]


## Annotate a core tool outcome's `detail` with the RESOLVED target species (R2-minor) so the status line shows
## WHAT actually got hit, not just the core's verdict. Pure renderer-side string prefix on a copy of the outcome
## (inv #2 — the core's targeting is unchanged; this only makes it legible). Leaves a non-Dictionary outcome alone.
func _with_target(outcome: Dictionary, ob: OptionButton) -> Dictionary:
	var d := outcome.duplicate()
	d["detail"] = "→ %s · %s" % [_picker_target_label(ob), str(outcome.get("detail", ""))]
	return d


func _on_guide_submitted(_text: String) -> void:
	_on_inject_pressed()


## Request a CRISPR edit from the running LiveSim, show the outcome, and mark it on the timeline.
func _on_inject_pressed() -> void:
	if _live == null or _cas_picker.selected < 0 or _locus_picker.selected < 0 or not _can_spend_edit():
		return
	var cas_id := int(_cas_ids[_cas_picker.selected])
	var locus_id := int(_locus_ids[_locus_picker.selected])
	# Issue 4: capture the PRE-edit genome as the baseline specimen BEFORE mutating it. Otherwise an inject that
	# fires before the specimen view was ever opened (e.g. the --inject --view specimen smoke, or a click straight
	# after launch) logs only the post-edit genome — the baseline variant is then lost and only the "edit" glyph
	# shows. Seeding it here guarantees baseline + edit N both appear in the row. (_log_live_genome dedups, so this
	# is a no-op once a baseline already exists.)
	_log_live_genome("baseline — gen %d" % int(_live.observe().get("generation", 0)))
	# Variant-Lab A: the whole-species edit targets the species the CRISPR panel's picker resolves to (default the
	# active/primary when nothing is explicitly selected). The same ordinal is the one we attribute the appended
	# specimen variant to below — so a multi-species roster's history grows on the EDITED row, not always the primary.
	var target_sid := _picker_species_id(_crispr_species) if _has_target_species(_crispr_species) else _active_species_id()
	var outcome: Dictionary = _live.apply_edit(cas_id, locus_id, _guide_edit.text, max(target_sid, 0))
	_record_edit_outcome(outcome)
	if bool(outcome.get("applied", false)):
		# Issue 4 (regression): a whole-species edit is a deliberate user action → it must ALWAYS append a NEW
		# specimen variant to the EDITED species' history, even when its 5-trait projection is visually unchanged
		# this frame (the core may re-express the phenotype only on the next generation, OR the edited param maps
		# to a trait already saturated / outside the projection). The old path logged via _log_live_genome, whose
		# dedup-on-traits then SUPPRESSED the variant ("Před tím přibývaly" → stopped). _append_edit_variant_for
		# force-appends to the EDITED species (target_sid), so baseline + edit 1 + … always grow to the right on the
		# right row. The passive refresh path (_log_live_genome on view re-entry) keeps its trait dedup so it never
		# double-logs.
		_append_edit_variant_for(target_sid if target_sid >= 0 else _active_species_id(), "edit", int(outcome.get("generation", 0)))
		# Re-render the specimen view NOW so the new variant glyph appears immediately (it used to only show on a
		# view re-entry). Cheap + idempotent when not in the specimen view (the root is hidden); keeps the picker +
		# framing in sync with the freshly-appended variant.
		if _view_mode == VIEW_SPECIMEN:
			_render_specimens()
			_update_trait_readout()
			_emphasise_focus()
	if _mission_on:
		_edits_used += 1


## Show an edit outcome (whole-species or region) in the status line + drop a timeline marker. Shared by the
## "Inject" button and the CRISPR brush — defaults to the CRISPR tool tag (the other tools call
## `_record_tool_outcome` with their own tag).
func _record_edit_outcome(outcome: Dictionary) -> void:
	_record_tool_outcome(TOOL_CRISPR, outcome)


## Generalized per-tool outcome readout (SP-3.6/3.7): show the verdict in the status line + drop a PER-TOOL
## timeline marker `{generation, tool, applied, label}`. Every tool routes through here so the status line + the
## timeline stay in lock-step. Read-only (inv #2) — `outcome` is the core's verdict, marshaled by the LiveSim
## #[func]; this only displays it.
func _record_tool_outcome(tool: int, outcome: Dictionary) -> void:
	var applied := bool(outcome.get("applied", false))
	var detail := str(outcome.get("detail", ""))
	if _inject_status != null:
		_inject_status.text = ("✓ " if applied else "✗ ") + detail
		_inject_status.add_theme_color_override(
			"font_color", Color(0.5, 0.92, 0.52) if applied else Color(0.96, 0.55, 0.5))
	_injections.append({
		"generation": int(outcome.get("generation", 0)),
		"tool": TOOL_KEYS[tool],
		"applied": applied,
		"label": detail,
	})
	if _timeline != null:
		_timeline.set_markers(_injections)


## Select an intervention tool (SP-3.6): swap its param sub-panel into view + retint the brush so the painted disc
## colour signals which tool will land. Pure renderer state (inv #2). `_on_tool_selected` is the button hook.
func _on_tool_selected(tool: int) -> void:
	_select_tool(tool)


func _select_tool(tool: int) -> void:
	_active_tool = clampi(tool, 0, _tool_panels.size() - 1)
	for i in _tool_panels.size():
		if _tool_panels[i] != null:
			_tool_panels[i].visible = (i == _active_tool)
	if _brush != null:
		_brush.set_tint(TOOL_TINTS[_active_tool])  # the disc COLOUR = the active tool


func _on_cull_strength_changed(v: float) -> void:
	if _cull_strength_label != null:
		_cull_strength_label.text = "%d%%" % int(round(v / 10.0))  # permille → %


## Toggle the selective brush mode (key B / the panel button). Live-mode only; clears the overlay when off and
## re-tints to the active tool when on.
func _set_brush_mode(on: bool) -> void:
	_brush_on = on and _live != null
	if _brush_button != null:
		_brush_button.set_pressed_no_signal(_brush_on)
		_brush_button.text = "🖌 Brush: on" if _brush_on else "🖌 Brush: off"
	if _brush != null:
		if _brush_on:
			_brush.set_tint(TOOL_TINTS[_active_tool])
		else:
			_brush.clear()


func _on_brush_toggled(pressed: bool) -> void:
	_set_brush_mode(pressed)


## Apply a region-scoped edit centred on the current brush cell, using the panel's Cas/locus/guide selection.
## (Kept for the --shot demo path; the live brush dispatches via _apply_active_tool.)
func _apply_brush() -> void:
	if _live == null or _brush_cell.x < 0 or _cas_picker == null or _cas_picker.selected < 0 \
			or _locus_picker.selected < 0:
		return
	if not _can_spend_edit():
		return
	var cas_id := int(_cas_ids[_cas_picker.selected])
	var locus_id := int(_locus_ids[_locus_picker.selected])
	var outcome: Dictionary = _live.apply_edit_region(
		cas_id, locus_id, _guide_edit.text, _brush_cell.x, _brush_cell.y, _brush_radius)
	_record_tool_outcome(TOOL_CRISPR, outcome)
	# Item 2(a): a brush CRISPR stroke ALSO surfaces a "region edit" variant in the specimen view, attributed to the
	# DOMINANT species at the painted cell (the species the player visually painted on, read from the GSS5
	# dominant_species_id plane). This lets a region edit be inspected as a glyph for investigation — even though it
	# changes per-individual alleles, not the species' whole genome. Renderer-only (inv #2): a projection of the
	# core's already-exported phenotype + per-cell dominant id, no biology in GDScript.
	if bool(outcome.get("applied", false)):
		var sid := _dominant_species_at(_brush_cell)
		# ADR-029 S4: journal this brush stroke renderer-side — the gen, the painted species, the disc, and the
		# PRIOR (pre-edit) dominant Variant in the disc (= the parent colony). The CORE (S1) mints the child Variant
		# id and stamps the covered orgs; that id appears in the disc in the NEXT snapshot, where
		# _resolve_pending_colonies binds it to this stroke → a named, family-coloured district. inv #2: we record
		# a render scope; GDScript mints nothing. inv #3: the existing apply_edit_region #[func] is the only wire.
		_pending_brush.append({
			"gen": int(outcome.get("generation", 0)),
			"species": sid if sid >= 0 else _active_species_id(),
			"cx": _brush_cell.x, "cy": _brush_cell.y, "r": _brush_radius,
			"parent": maxi(0, _dominant_variant_at(_brush_cell)), "tries": 0,
		})
		_append_edit_variant_for(sid if sid >= 0 else _active_species_id(), "region edit", int(outcome.get("generation", 0)))
		if _view_mode == VIEW_SPECIMEN:
			_render_specimens()
			_update_trait_readout()
			_emphasise_focus()
	if _mission_on:
		_edits_used += 1


## The dominant SpeciesId at a world cell, read from the latest snapshot's GSS5 dominant_species_id plane (the
## per-cell most-populous species the core exports). -1 when there's no snapshot, the cell is out of range, or the
## build predates GSS5 (forward-compat). Pure read (inv #2): an index into an already-exported plane, no biology.
func _dominant_species_at(cell: Vector2i) -> int:
	if _snaps.is_empty() or cell.x < 0 or cell.y < 0:
		return -1
	var snap = _snaps[_snaps.size() - 1]
	if not ("dominant_species_id" in snap) or cell.x >= snap.width or cell.y >= snap.height:
		return -1
	var plane: PackedFloat32Array = snap.dominant_species_id
	var w := int(snap.width)  # snap is an untyped Array element → annotate so the index type is inferred (not Variant)
	var i: int = cell.y * w + cell.x
	if i < 0 or i >= plane.size():
		return -1
	return int(round(plane[i]))


## ADR-029 S4: the dominant Variant id at a world cell (GSS6 dominant_variant_id plane) — the sibling of
## _dominant_species_at, used to read a brush's PARENT colony before the edit + to compose the packed selection
## key. -1 when no snapshot / out of range / pre-GSS6. Pure read (inv #2): an index into an exported plane.
func _dominant_variant_at(cell: Vector2i) -> int:
	if _snaps.is_empty() or cell.x < 0 or cell.y < 0:
		return -1
	var snap = _snaps[_snaps.size() - 1]
	if not ("dominant_variant_id" in snap) or cell.x >= snap.width or cell.y >= snap.height:
		return -1
	var plane: PackedFloat32Array = snap.dominant_variant_id
	var w := int(snap.width)
	var i: int = cell.y * w + cell.x
	if i < 0 or i >= plane.size():
		return -1
	return int(round(plane[i]))


## ADR-029 S4: select a colony by its packed (species*65536+variant) key (-1 = clear) and push it to both render
## layers, which pop the clicked district open to its members regardless of zoom (capped) while neighbours stay
## aggregated. Guarded against a no-op reselect. Pure presentation routing (inv #2): a click → a render scope.
func _set_selected_colony(key: int) -> void:
	if key == _selected_colony_id:
		return
	_selected_colony_id = key
	if _colonies != null and _colonies.has_method("set_selected_colony"):
		_colonies.set_selected_colony(key)
	if _organisms != null and _organisms.has_method("set_selected_colony"):
		_organisms.set_selected_colony(key)


## ADR-029 S4: resolve fired-but-unminted brush strokes against the latest snapshot. For each pending stroke,
## tally the Variant ids inside its disc (row-major) and pick the most-populous id that is NOT the recorded parent
## — that is the freshly-minted child the CORE stamped (S1). Register it (name/colour/lineage) for the district
## label. Bounded retries (the child may take a frame to dominate the disc). ORDERED passes only — the pending
## list + disc are iterated in order; the tally Dict is read via SORTED keys; the registry is keyed-access only
## (inv #3). inv #2: GDScript reads inert per-cell ordinals + observe_species names — it computes no biology.
func _resolve_pending_colonies(snap) -> void:
	if _pending_brush.is_empty() or not ("dominant_variant_id" in snap):
		return
	var plane: PackedFloat32Array = snap.dominant_variant_id
	var w := int(snap.width)
	var h := int(snap.height)
	var still: Array = []
	for stroke in _pending_brush:
		var cx: int = int(stroke["cx"])
		var cy: int = int(stroke["cy"])
		var r: int = int(stroke["r"])
		var parent: int = int(stroke["parent"])
		var tally: Dictionary = {}  # variant_id -> count; KEYED access only (read via sorted keys below; inv #3)
		var r2 := r * r
		for dy in range(-r, r + 1):
			for dx in range(-r, r + 1):
				if dx * dx + dy * dy > r2:
					continue
				var x := cx + dx
				var y := cy + dy
				if x < 0 or y < 0 or x >= w or y >= h:
					continue
				var i := y * w + x
				if i < 0 or i >= plane.size():
					continue
				var vid := int(round(plane[i]))
				tally[vid] = int(tally.get(vid, 0)) + 1
		var keys: Array = tally.keys()
		keys.sort()  # ordinal-sorted → deterministic tiebreak (lowest id wins on a tie; inv #3)
		var best_vid := -1
		var best_cnt := 0
		for vid in keys:
			if vid == parent or vid == 0:
				continue
			var cnt: int = tally[vid]
			if cnt > best_cnt:
				best_cnt = cnt
				best_vid = vid
		if best_vid > 0:
			_register_colony(best_vid, stroke)
		else:
			stroke["tries"] = int(stroke.get("tries", 0)) + 1
			if int(stroke["tries"]) < 6:  # bounded retry: the child may need a frame to dominate the disc
				still.append(stroke)
	_pending_brush = still


## ADR-029 S4: add a brush-minted child Variant to the registry: its species, a STABLE family colour (the species
## base colour hue-shifted by the variant — reads as the same species, not a foreign one), the gen it was created,
## and its parent Variant. Keyed write only; idempotent. Read-only metadata (inv #2): no biology, no hash.
func _register_colony(vid: int, stroke: Dictionary) -> void:
	if _colony_registry.has(vid):
		return
	var sid: int = int(stroke["species"])
	var meta: Dictionary = _species_id_meta().get(sid, {})
	var key := str(meta.get("key", "default"))
	var role := str(meta.get("role", ""))
	var base: Color = SpeciesVisualMap.color_for(key, role)
	_colony_registry[vid] = {
		"species": sid,
		"label": _species_name_for(sid),
		"color": _family_color(base, vid),
		"gen_created": int(stroke.get("gen", 0)),
		"parent": int(stroke.get("parent", 0)),
	}


## The display name of a species id (observe_species() rows, an ordered Array — no hash-order iteration; inv #3).
func _species_name_for(sid: int) -> String:
	if _live != null and _live.has_method("observe_species"):
		for sp in _live.observe_species():
			if int((sp as Dictionary).get("species_id", -1)) == sid:
				return str((sp as Dictionary).get("name", "species"))
	return "species"


## A brushed child district's STABLE family colour: the species base colour, hue-shifted a bounded, deterministic
## amount by the variant id (matches colonies.gd's _variant_hue_shift / VARIANT_HUE_RANGE so the two agree). A
## Color transform only — no biology (inv #2).
func _family_color(base: Color, vid: int) -> Color:
	var shift := (fposmod(float(vid) * 0.137, 1.0) - 0.5) * 0.18  # ±VARIANT_HUE_RANGE/2 (colonies.gd: 0.18)
	return Color.from_hsv(fposmod(base.h + shift, 1.0), base.s, base.v, 1.0)


## Dispatch the ACTIVE tool at the current brush cell (SP-3.6). POSITION MATTERS end-to-end: brush cell →
## RegionSpec → Region::contains in the core selects the orgs/cells. Each tool issues ONE journaled Action via its
## LiveSim #[func] and records the per-tool verdict (status line + timeline marker). Biology stays in the core
## (inv #2). The four substrate/clone #[func]s are has_method-guarded so the renderer degrades gracefully against
## an older cdylib (before SP-3.5 lands) — exactly the forward-compat probe used for observe_species/flow_matrix.
func _apply_active_tool() -> void:
	if _live == null or _brush_cell.x < 0:
		return
	var cx := _brush_cell.x
	var cy := _brush_cell.y
	var r := _brush_radius
	match _active_tool:
		TOOL_CRISPR:
			_apply_brush()  # the existing CRISPR region edit (spends a mission edit, gates on budget)
		TOOL_PCR:
			if not _live.has_method("pcr_amplify"):
				_flash_status("✗ PCR unsupported by this build", false)
				return
			if not _has_target_species(_pcr_species):
				_record_no_target(TOOL_PCR)
				return
			var sid := _picker_species_id(_pcr_species)
			var cnt := int(_pcr_count.value) if _pcr_count != null else 1
			var endow := int(_pcr_endow.value) if _pcr_endow != null else 0
			# R2-minor: surface the RESOLVED target in the status so a reordered roster / implicit default-to-0
			# is legible, not a silent mistarget. Renderer-only annotation; the core owns the actual targeting.
			_record_tool_outcome(TOOL_PCR, _with_target(_live.pcr_amplify(sid, cx, cy, r, cnt, endow), _pcr_species))
		TOOL_ANTIBIOTIC:
			if not _live.has_method("cull"):
				_flash_status("✗ Antibiotic unsupported by this build", false)
				return
			if not _has_target_species(_cull_species):
				_record_no_target(TOOL_ANTIBIOTIC)
				return
			var sid2 := _picker_species_id(_cull_species)
			var strength := int(_cull_strength.value) if _cull_strength != null else 0
			# R2-minor: surface the RESOLVED cull target in the status (see PCR above).
			_record_tool_outcome(TOOL_ANTIBIOTIC, _with_target(_live.cull(sid2, cx, cy, r, strength), _cull_species))
		TOOL_NUTRIENT:
			if not _live.has_method("nutrient"):
				_flash_status("✗ Nutrient unsupported by this build", false)
				return
			var ch := _nutrient_channel.selected if _nutrient_channel != null else 1
			var amt := int(_nutrient_amount.value) if _nutrient_amount != null else 0
			_record_tool_outcome(TOOL_NUTRIENT, _live.nutrient(ch, cx, cy, r, amt))
		TOOL_TOXIN:
			if not _live.has_method("toxin"):
				_flash_status("✗ Toxin unsupported by this build", false)
				return
			var ch2 := _toxin_channel.selected if _toxin_channel != null else 0
			var amt2 := int(_toxin_amount.value) if _toxin_amount != null else 0
			_record_tool_outcome(TOOL_TOXIN, _live.toxin(ch2, cx, cy, r, amt2))
		TOOL_INOCULATE:
			if not _live.has_method("inoculate"):
				_flash_status("✗ Inoculate unsupported by this build", false)
				return
			_inoculate_at(cx, cy, r)


## Issue ONE manual RegionInoculate at the painted disc (ADR-019 S3): lazily register the selected contaminant
## SpeciesSpec (res:// JSON bytes → core, inv #2), then fire LiveSim.inoculate (J minted from the `immigration`
## tap, conserved, journaled). Records a per-tool timeline marker so a manual seed shows on the timeline exactly
## like a fired schedule event. POSITION MATTERS: (cx, cy, r) is the disc the core spawns into. Returns nothing —
## establish/displace/die emerges from the core economy (this only supplies the arrival).
func _inoculate_at(cx: int, cy: int, r: int) -> void:
	# Slice C: an ARMED saved variant takes precedence over the contaminant picker. It is NOT a res:// file, so it is
	# re-registered from its stored JSON each stroke (robust across a _do_reset that cleared the core consortium /
	# _registered_contaminants) instead of going through _ensure_contaminant_registered (which reads res://).
	var key: String
	var label: String
	if _armed_variant_idx >= 0 and _armed_variant_idx < _saved_variants.size():
		var v: Dictionary = _saved_variants[_armed_variant_idx]
		key = str(v.get("key", ""))
		if key == "" or not _live.has_method("register_contaminant_json") \
				or not _live.register_contaminant_json(str(v.get("json", ""))):
			_flash_status("✗ armed variant could not be re-registered", false)
			return
		_registered_contaminants[key] = true
		label = str(v.get("name", key))
	else:
		key = _inoc_selected_key()
		if key == "" or not _ensure_contaminant_registered(key):
			_flash_status("✗ contaminant '%s' could not be registered" % key, false)
			return
		label = key
	var cnt := int(_inoc_count.value) if _inoc_count != null else 1
	var endow := int(_inoc_endow.value) if _inoc_endow != null else 0
	var minted: int = int(_live.inoculate(key, cx, cy, r, cnt, endow))
	var gen := int(_live.observe().get("generation", 0)) if _live.has_method("observe") else 0
	_record_tool_outcome(TOOL_INOCULATE, {
		"applied": cnt > 0,
		"detail": "🦠 %s ×%d @ (%d,%d) r%d · tap %d J" % [label, cnt, cx, cy, r, minted],
		"generation": gen,
	})


## The contaminant file-stem currently selected in the seed-brush picker ("" if none). Pure read.
func _inoc_selected_key() -> String:
	if _inoc_species == null or _inoc_species.selected < 0:
		return ""
	return str(_inoc_species.get_item_metadata(_inoc_species.selected))


## Lazily register a contaminant's SpeciesSpec with the core so a later inoculate (manual OR a fired schedule
## event) can resolve its `species_key` (ADR-019 S1/S3). Reads the res:// JSON bytes and hands them to
## LiveSim.register_contaminant_json — the res:// boundary (inv #2/#4): GDScript moves only the inert string, the
## core does serde + SpeciesSpec::build. Idempotent (registers once per run per key). Returns true if the key is
## (now) registered. Graceful: a missing/invalid file → warning → false (the seed no-ops, the run stays valid).
func _ensure_contaminant_registered(key: String) -> bool:
	if key == "":
		return false
	if _registered_contaminants.get(key, false):
		return true
	if _live == null or not _live.has_method("register_contaminant_json"):
		return false
	var path := "res://data/species/%s.json" % key
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		push_warning("contaminant '%s' not found at %s" % [key, path])
		return false
	var text := f.get_as_text()  # whole JSON; FileAccess (RefCounted) closes on free
	if not _live.register_contaminant_json(text):
		push_warning("contaminant '%s' failed to validate" % key)
		return false
	_registered_contaminants[key] = true
	return true


## Discover the baked AIRBORNE contaminant stems by scanning res://data/species/ (the docstring's promise, made
## real — R1/R4): list the *.json stems, drop NON_AIRBORNE_STEMS (the player species + the obligate symbionts,
## which can never airborne-arrive), and return the rest sorted (stable, deterministic UI order — inv #3 hygiene).
## These are just file stems; the core builds every genome from the JSON bytes (inv #2). Falls back to the 7
## known baked keys if the directory cannot be opened (an odd export). Memoized into _contaminant_keys.
func _discover_contaminant_keys() -> Array:
	if not _contaminant_keys.is_empty():
		return _contaminant_keys
	var found: Array = []
	var dir := DirAccess.open("res://data/species")
	if dir != null:
		for fname in dir.get_files():
			if not fname.ends_with(".json"):
				continue
			var stem := fname.get_basename()
			if NON_AIRBORNE_STEMS.has(stem):
				continue
			found.append(stem)
		found.sort()  # stable UI order regardless of the filesystem's listing order (inv #3 hygiene)
	if found.is_empty():
		found = CONTAMINANT_KEYS_FALLBACK.duplicate()  # scan unavailable → the known baked airborne set
	_contaminant_keys = found
	return _contaminant_keys


## A short role/trait one-liner for a consortium contaminant `key`, for its checkbox tooltip (Item 2b). Reads the
## codex (display_name + headline — a read-only off-hash projection, inv #2) when loaded; else falls back to a
## static blurb keyed by the file stem, then to the bare key. Presentation only — the core owns every genome.
func _consortium_blurb(key: String) -> String:
	var cx := _codex.species_for(key)
	if not cx.is_empty():
		var nm := str(cx.get("display_name", key))
		var head := str(cx.get("headline", cx.get("sim_role", "")))
		return "%s\n%s" % [nm, head] if head != "" else nm
	return CONSORTIUM_FALLBACK_BLURBS.get(key, "Airborne contaminant '%s' — rides the immigration schedule." % key)


## Update the hovered brush cell from the mouse (world → cell, clamped to the world/live grid) + refresh preview.
func _update_brush_cell() -> void:
	if _brush == null:
		return
	var cc := _cell_at(get_global_mouse_position())
	_brush_cell = Vector2i(clampi(cc.x, 0, LIVE_GRID.x - 1), clampi(cc.y, 0, LIVE_GRID.y - 1))
	_brush.set_brush(_brush_cell, _brush_radius)


func _set_brush_radius(r: int) -> void:
	_brush_radius = clampi(r, 1, 16)
	if _brush != null and _brush_cell.x >= 0:
		_brush.set_brush(_brush_cell, _brush_radius)


# ──────────────────────────── mission / gamification (S-G2, renderer-side game rules) ────────────────────

## A left-rail Mission panel + a centred win/lose banner. The goal, current zone reading, edit budget, and
## deadline are all game RULES over the core-exported snapshot — no biology is computed here (inv #2). Live only.
func _build_mission_ui(ui: CanvasLayer) -> void:
	var body := _dark_panel(0.8)
	body.custom_minimum_size = Vector2(246, 0)
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 3)
	body.add_child(col)
	_mission_label = Label.new()
	_mission_label.add_theme_font_size_override("font_size", 13)
	_mission_label.add_theme_color_override("font_color", Color(0.9, 0.95, 0.95))
	_mission_label.custom_minimum_size = Vector2(232, 0)
	_mission_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	col.add_child(_mission_label)
	_mission_panel = PanelChrome.new()
	_mission_panel.setup("🎯 MISSION", body, ui, Vector2(12, 286), _pill_rail)
	_mission_panel.set_active(_mission_on)
	if _mission_marker != null and _mission_on:
		_mission_marker.set_brush(_mission_zone, _mission_radius)  # paint the static cyan goal zone (mission only)

	_mission_banner = Label.new()
	_mission_banner.position = Vector2(_field_screen_size().x * 0.5 - 170.0, 78.0)
	_mission_banner.add_theme_font_size_override("font_size", 28)
	_mission_banner.visible = false
	ui.add_child(_mission_banner)


## Whether a (mission) edit can be spent; updates the status line when the budget is exhausted.
func _can_spend_edit() -> bool:
	if _mission_on and _edits_used >= _edit_budget:
		if _inject_status != null:
			_inject_status.text = "✗ out of edits (budget %d)" % _edit_budget
			_inject_status.add_theme_color_override("font_color", Color(0.96, 0.55, 0.5))
		return false
	return true


## Evaluate the mission from the current snapshot: the mean allele_freq over the POPULATED cells of the target
## zone, plus the win (zone ≤ target before the deadline) / lose (deadline passed) check + score. Read-only.
func _eval_mission() -> void:
	if not _mission_on or _snaps.is_empty():
		return
	var snap = _snaps[_idx]
	var w: int = snap.width
	var zone_allele := 0.0
	var n := 0
	if _live != null:
		# CORE-computed zone read (invariant #2): the mission's biology now lives in the Rust core, not
		# GDScript — LiveSim.region_allele returns the same mean-of-populated-cell-means over the disc.
		var rd: Dictionary = _live.region_allele(_mission_zone.x, _mission_zone.y, _mission_radius, w, snap.height)
		zone_allele = float(rd.get("mean", 0.0))
		n = int(rd.get("populated", 0))
	else:
		# Replay fallback (no LiveSim node): the legacy GDScript snapshot loop over the same disc.
		var sum := 0.0
		var r2 := _mission_radius * _mission_radius
		for dy in range(-_mission_radius, _mission_radius + 1):
			for dx in range(-_mission_radius, _mission_radius + 1):
				if dx * dx + dy * dy > r2:
					continue
				var cx := _mission_zone.x + dx
				var cy := _mission_zone.y + dy
				if cx < 0 or cy < 0 or cx >= w or cy >= snap.height:
					continue
				var i: int = cy * w + cx
				if snap.density[i] > 0.0:
					sum += snap.allele_freq[i]
					n += 1
		zone_allele = (sum / float(n)) if n > 0 else 0.0
	var gen: int = snap.generation
	if _mission_status == 0:
		if n > 0 and zone_allele <= _mission_target:
			_mission_status = 1
			var score := (_edit_budget - _edits_used) * 10 + maxi(0, _mission_deadline - gen)
			_show_mission_banner("✓ MISSION COMPLETE   ·   score %d" % score, Color(0.45, 0.95, 0.5))
		elif gen > _mission_deadline:
			_mission_status = 2
			_show_mission_banner("✗ MISSION FAILED   ·   deadline passed", Color(0.96, 0.5, 0.45))
	if _mission_label != null:
		_mission_label.text = (
			"Suppress allele in the cyan zone ≤ %.2f.\nzone %.2f   ·   edits %d/%d   ·   gen %d/%d"
			% [_mission_target, zone_allele, _edits_used, _edit_budget, gen, _mission_deadline])


func _show_mission_banner(text: String, color: Color) -> void:
	if _mission_banner == null:
		return
	_mission_banner.text = text
	_mission_banner.add_theme_color_override("font_color", color)
	_mission_banner.visible = true


## Item (c): a transient top-centre HUD flash for per-species extinction/boom events (--live). A single Label
## pinned below the title bar; _poll_population_alerts fills it + sets the auto-hide deadline. Presentation only.
func _build_alert_ui(ui: CanvasLayer) -> void:
	_alert_label = Label.new()
	_alert_label.position = Vector2(_field_screen_size().x * 0.5 - 150.0, 116.0)
	_alert_label.add_theme_font_size_override("font_size", 22)
	_alert_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_alert_label.add_theme_color_override("font_color", Color(0.98, 0.86, 0.4))
	# A subtle dark outline so the flash reads over any data layer underneath.
	_alert_label.add_theme_color_override("font_outline_color", Color(0.0, 0.0, 0.0, 0.85))
	_alert_label.add_theme_constant_override("outline_size", 6)
	_alert_label.visible = false
	ui.add_child(_alert_label)


## Item (c): poll the per-species population (observe_species().population_size) against the PRIOR render frame's
## cache and flash an extinction ("✗ <name> extinct") or boom ("📈 <name> boom") notification on a 0-crossing or a
## ≥10× jump. This surfaces the emergent "the predator ate them all" / "the prey bloomed" moment. Read-only (inv
## #2): the population integers are a pure core export (no biology here — only a prev-vs-now compare on already-
## measured counts); deterministic w.r.t. the snapshot (inv #3), the flash itself is wall-clock UI only.
func _poll_population_alerts() -> void:
	if _live == null or not _live.has_method("observe_species"):
		return
	var events: PackedStringArray = PackedStringArray()
	var seen: Dictionary = {}  # species_id → population this frame (becomes the next _prev_pop)
	for sp in _live.observe_species():
		var d: Dictionary = sp
		var sid := int(d.get("species_id", 0))
		var pop := int(d.get("population_size", 0))
		var nm := str(d.get("name", "species"))
		seen[sid] = pop
		if pop > 0:
			_ever_alive[sid] = true  # Item 2(b): remember it was alive (so extinction = ever-alive then gone)
		if not _prev_pop.has(sid):
			continue  # first sighting of this species — establish a baseline, don't flash
		var prev := int(_prev_pop[sid])
		if prev > 0 and pop == 0:
			events.append("✗ %s extinct" % nm)
		elif prev >= ALERT_BOOM_FLOOR and pop >= prev * ALERT_BOOM_FACTOR:
			events.append("📈 %s boom" % nm)
	_prev_pop = seen
	# Item 2(b): recompute the persistent extinct set across EVERY species that owns a specimen glyph. A species is
	# extinct when it was ever alive but is now absent from observe_species() (it can be dropped when empty) OR shows
	# population 0; it un-marks the moment it's alive again (a spore bank regerminating — pop 0→N). The specimen view
	# keeps the glyph either way and strikes it while extinct. No biology here (inv #2) — a prev/now compare only.
	for sid in _live_species_logs:
		var key_sid := int(sid)
		var alive: bool = seen.has(key_sid) and int(seen[key_sid]) > 0
		if alive:
			_extinct.erase(key_sid)
		elif _ever_alive.has(key_sid):
			_extinct[key_sid] = true
	if events.size() > 0:
		# Newest event wins the single line (multiple at once are joined). Colour by kind: red = extinction.
		var any_ext := false
		for e in events:
			if e.begins_with("✗"):
				any_ext = true
		_alert_label.text = "   ·   ".join(events)
		_alert_label.add_theme_color_override(
			"font_color", Color(0.96, 0.45, 0.42) if any_ext else Color(0.45, 0.92, 0.55))
		_alert_label.visible = true
		_alert_until = float(Time.get_ticks_msec()) / 1000.0 + ALERT_HOLD_SEC


## Hide an expired alert flash (called per render frame). Wall-clock only — never touches the sim or its hash.
func _tick_alert_flash() -> void:
	if _alert_label != null and _alert_label.visible:
		if float(Time.get_ticks_msec()) / 1000.0 >= _alert_until:
			_alert_label.visible = false


# ──────────────────────────── scene construction (read-only presentation) ────────────────────────────

func _build_scene() -> void:
	var first = _snaps[0]
	var w: int = first.width
	var h: int = first.height
	_cell = maxf(3.0, floorf(TARGET_FIELD_PX / float(max(w, h))))
	_field_px = Vector2(float(w) * _cell, float(h) * _cell)

	# Isometric mode (P3): a CPU-diamond ground + iso-projected organisms, instead of the ortho TileMap +
	# axis-aligned shader overlay. ISOMETRIC is now the DEFAULT; pass --ortho to opt into the flat view.
	# (--iso is still accepted as a no-op for back-compat.) Read-only presentation (#2).
	if not _has_flag("--ortho"):
		_iso = Iso.new()
		var b: Rect2 = _iso.field_bounds(w, h, _cell)
		_iso.origin = -b.position + Vector2(20, 20)  # shift the negative-x left edge fully on-screen
	print("ecosystem mode: %s" % ("ISOMETRIC (default)" if _iso != null else "orthographic (--ortho)"))

	# Ecosystem layers live under _world so the whole view can be toggled off for the specimen view.
	_world = Node2D.new()
	add_child(_world)

	if _iso != null:
		_iso_ground = IsoGround.new()
		_iso_ground.setup(w, h, _cell, _iso)
		_world.add_child(_iso_ground)
	else:
		_terrain = _build_terrain(w, h, int(_cell))
		_world.add_child(_terrain)

	_overlay = Sprite2D.new()
	_overlay.centered = false
	_overlay.texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST  # one data texel = one crisp cell block
	var mat := ShaderMaterial.new()
	mat.shader = DataLayerShader
	_overlay.visible = (_iso == null)  # iso draws the data overlay into the diamonds instead
	_world.add_child(_overlay)

	_organisms = Organisms.new()
	_organisms.set_iso(_iso)
	_world.add_child(_organisms)

	# ADR-029 S2: colony polygon layer, a sibling drawn just above organisms in _world. At Field scope it
	# replaces the per-cell dot spam with O(#colonies) district polygons (the de-spam); _update_scope_layers
	# swaps which of the two layers is visible by zoom. Pure presentation geometry (inv #2).
	_colonies = Colonies.new()
	_colonies.set_iso(_iso)
	_world.add_child(_colonies)

	# Selective-edit brush overlay (drawn above organisms, in world space). Only used in --live mode.
	_brush = Brush.new()
	_brush.setup(_iso, _cell, LIVE_GRID)
	_world.add_child(_brush)

	# Mission target-zone marker (cyan, static) — the gamification goal area (ADR-011 S-G2), live only.
	_mission_marker = Brush.new()
	_mission_marker.setup(_iso, _cell, LIVE_GRID)
	_mission_marker.set_tint(Color(0.3, 0.85, 0.95, 0.22))
	_world.add_child(_mission_marker)

	# L-system specimen view (S4.5) — hidden until toggled.
	_specimen_root = Node2D.new()
	_specimen_root.visible = false
	add_child(_specimen_root)

	# Relations view (Rel-UI.0) — hidden until toggled. A parallel root to _specimen_root; the heatmap itself is a
	# fixed Control built into the panel chrome (in _build_relations_ui), not world-space, so this root just gates
	# visibility symmetrically with the others.
	_relations_root = Node2D.new()
	_relations_root.visible = false
	add_child(_relations_root)

	# A camera framing the whole field; S4.4 adds zoom scopes on top of this.
	_cam = Camera2D.new()
	_cam.position = _field_center()
	add_child(_cam)
	_cam.make_current()  # must be in-tree first

	# Screen-space edge vignette (S4). It sits ABOVE the world (Node2D, effective layer 0) but BELOW the UI:
	# there is no integer between 0 and 1, so we set explicit layers — vignette=1, UI=2 — and rely on layer
	# order, not tree order. Hidden in the specimen view (clean dark backdrop there).
	_build_vignette()

	# HUD + controls on their own CanvasLayer (layer 2) so they sit above the vignette and ignore the camera.
	var ui := CanvasLayer.new()
	ui.layer = 2
	add_child(ui)
	# UI positions key off the on-screen field size (the iso diamond bbox under --iso, else the ortho rect).
	var field_screen := _field_screen_size()
	# The pill rail (minimized-panel dock above the timeline) must exist before the panel builders so they can
	# be handed it (Phase U). It anchors by preset, so tree order vs the timeline doesn't matter.
	_pill_rail = PillRail.new()
	_pill_rail.setup(ui)
	_build_titlebar(ui)
	_build_hud(ui, field_screen)
	_build_vitals_ui(ui)
	_build_controls(ui, field_screen)
	_build_codex_ui(ui, field_screen)  # full-window browsable encyclopedia; built BEFORE the viewscope so the always-on VIEW switcher draws on top of it (the codex panel must never cover its own escape hatch)
	_build_viewscope_ui(ui)  # the always-on top-right VIEW + SCOPE switcher (built after _build_controls owns _scope_buttons)
	_build_specimen_ui(ui, field_screen)
	_build_relations_ui(ui, field_screen)
	_build_interaction_ui(ui)
	_build_timeline(ui)
	_build_intervention_ui(ui)
	_build_contamination_ui(ui)
	_build_oversight_ui(ui)
	_build_mission_ui(ui)
	_build_alert_ui(ui)
	# --live was requested but the LiveSim cdylib failed to load → show why (we fell back to file replay).
	if _has_flag("--live") and _live == null:
		var np := _dark_panel(0.82)
		np.position = Vector2(238, 46)
		var notice := Label.new()
		notice.text = "⚠  --live needs the LiveSim cdylib — build it:\n   cargo build --manifest-path crates/godot-sim/Cargo.toml\n   (showing file replay for now)"
		notice.add_theme_color_override("font_color", Color(0.98, 0.8, 0.4))
		notice.add_theme_font_size_override("font_size", 13)
		np.add_child(notice)
		ui.add_child(np)

	# Size the window to the field (+ margin) when we have a display.
	if DisplayServer.get_name() != "headless":
		# Bottom margin clears the control deck (~150) + the pill rail (window-100) + the timeline (window-54)
		# stacked below the field without overlap (Phase U review fix).
		var win := (_field_screen_size() + Vector2(40, 290)).max(Vector2(820, 680))
		DisplayServer.window_set_size(Vector2i(int(win.x), int(win.y)))
	RenderingServer.set_default_clear_color(Color(0.06, 0.08, 0.07))


## A tiled grass field: a small procedurally-generated atlas of green shades placed with hash variation.
## This is the "2D TileMap ecosystem view of one scope" (a field) — pure backdrop, no biology.
func _build_terrain(w: int, h: int, cell: int) -> TileMapLayer:
	var shades := [
		Color(0.18, 0.33, 0.17), Color(0.21, 0.37, 0.19),
		Color(0.16, 0.30, 0.16), Color(0.24, 0.41, 0.22),
		Color(0.19, 0.34, 0.18), Color(0.15, 0.27, 0.15),  # last = slightly darker soil patch (not black)
	]
	var n := shades.size()
	var atlas := Image.create(cell * n, cell, false, Image.FORMAT_RGBA8)
	for ti in n:
		for yy in cell:
			for xx in cell:
				# Per-pixel speckle + a per-column green streak so tiles read as grass blades, not flat blocks.
				var speckle := (_hash01(xx, yy, ti) - 0.5) * 0.04
				var blade := (_hash01(xx, ti * 7 + 1, 3) - 0.5) * 0.05  # vertical blade streaks (mostly green)
				var c: Color = shades[ti]
				atlas.set_pixel(ti * cell + xx, yy, Color(
					clampf(c.r + speckle, 0.0, 1.0),
					clampf(c.g + speckle + blade * 1.4, 0.0, 1.0),
					clampf(c.b + speckle, 0.0, 1.0)))
	var tex := ImageTexture.create_from_image(atlas)

	var ts := TileSet.new()
	ts.tile_size = Vector2i(cell, cell)
	var src := TileSetAtlasSource.new()
	src.texture = tex
	src.texture_region_size = Vector2i(cell, cell)
	for ti in n:
		src.create_tile(Vector2i(ti, 0))
	var sid := ts.add_source(src)

	var layer := TileMapLayer.new()
	layer.tile_set = ts
	for y in h:
		for x in w:
			# Shade from a COARSE block so the field reads as grassy patches, not per-tile checker noise,
			# with an occasional single-cell speckle for texture.
			var ti := int(_hash01(int(x / 3.0), int(y / 3.0), 7) * float(n))
			if _hash01(x, y, 11) > 0.86:
				ti = int(_hash01(x, y, 13) * float(n))
			layer.set_cell(Vector2i(x, y), sid, Vector2i(ti % n, 0))
	return layer


## Screen-space edge vignette (layer 1, between world and UI). MOUSE_FILTER_IGNORE so it never eats the
## wheel-zoom / button clicks that pass to the world or the UI above it.
func _build_vignette() -> void:
	_vignette = CanvasLayer.new()
	_vignette.layer = 1
	add_child(_vignette)
	var tr := TextureRect.new()
	tr.texture = _vignette_texture()
	tr.set_anchors_preset(Control.PRESET_FULL_RECT)
	tr.stretch_mode = TextureRect.STRETCH_SCALE
	tr.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	tr.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_vignette.add_child(tr)


## A radial gradient: transparent centre → soft dark frame at the edges/corners (CPU image, headless-safe).
func _vignette_texture() -> ImageTexture:
	var n := 256
	var img := Image.create(n, n, false, Image.FORMAT_RGBA8)
	var c := Vector2(n - 1, n - 1) * 0.5
	var maxd := c.length()
	for y in n:
		for x in n:
			var d := (Vector2(x, y) - c).length() / maxd  # 0 centre … 1 corner
			var a := pow(clampf((d - 0.5) / 0.5, 0.0, 1.0), 1.6) * 0.5
			img.set_pixel(x, y, Color(0, 0, 0, a))
	return ImageTexture.create_from_image(img)


# ──────────────────────────── HUD + legend ────────────────────────────

## Build the status line (in a translucent panel) and the colormap legend (bottom-left).
# ──────────────────────────── title bar + vitals scoreboard (S3, read-only) ────────────────────────────

## Full-width top header: game title (left) + a run-state chip strip (right). Replaces the old dense floating
## HUD string. Read-only presentation (inv #2) — every chip is a number the core exported.
func _build_titlebar(ui: CanvasLayer) -> void:
	_titlebar = PanelContainer.new()
	_titlebar.set_anchors_preset(Control.PRESET_TOP_WIDE)
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.04, 0.06, 0.05, 0.9)
	sb.set_content_margin_all(7)
	sb.border_width_bottom = 2
	sb.border_color = Color(0.2, 0.45, 0.3, 0.7)
	_titlebar.add_theme_stylebox_override("panel", sb)
	ui.add_child(_titlebar)

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)
	_titlebar.add_child(row)
	var title := Label.new()
	title.text = "GENE-SIM   ·   CRISPR Ecosystem"
	title.add_theme_font_size_override("font_size", 18)
	title.add_theme_color_override("font_color", Color(0.85, 0.95, 0.8))
	row.add_child(title)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(spacer)
	_title_badge = Label.new()
	_title_badge.add_theme_font_size_override("font_size", 15)
	row.add_child(_title_badge)
	# RichTextLabel (not a plain Label) so the per-chip ▲/▼/→ trend glyphs can be color-coded inline via BBCode
	# (green up / red down / neutral flat). Sized to its content + single-line so it sits in the title row like a
	# Label. The `.text` setter still accepts plain captions (specimen/relations views) — BBCode only kicks in on tags.
	_title_status = RichTextLabel.new()
	_title_status.bbcode_enabled = true
	_title_status.fit_content = true
	_title_status.scroll_active = false
	_title_status.autowrap_mode = TextServer.AUTOWRAP_OFF
	_title_status.custom_minimum_size = Vector2(360, 0)
	_title_status.add_theme_font_size_override("normal_font_size", 15)
	_title_status.add_theme_color_override("default_color", Color(0.88, 0.92, 0.88))
	row.add_child(_title_status)


## Top-left Vitals scoreboard: Population / Mean fitness / Allele freq with ▲▼ trend, plus a recent-trend
## sparkline. Fed from LiveSim.observe() in --live, else from snapshot field-means over populated cells. The
## single game scoreboard. Read-only (inv #2): the core exports these numbers; the sparkline plots recorded
## data (inv #3, no RNG).
func _build_vitals_ui(ui: CanvasLayer) -> void:
	var body := _dark_panel(0.74)
	body.custom_minimum_size = Vector2(214, 0)
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 4)
	body.add_child(col)
	_vitals_pop = _vital_label()
	_vitals_fit = _vital_label()
	_vitals_allele = _vital_label()
	col.add_child(_vitals_pop)
	col.add_child(_vitals_fit)
	col.add_child(_vitals_allele)
	_sparkline = Sparkline.new()
	_sparkline.custom_minimum_size = Vector2(198, 40)
	col.add_child(_sparkline)
	var cap := Label.new()
	cap.text = "fitness / allele — recent trend"
	cap.add_theme_font_size_override("font_size", 11)
	cap.add_theme_color_override("font_color", Color(0.6, 0.66, 0.6))
	col.add_child(cap)
	# Wrap in the draggable/minimizable panel chrome (Phase U). The wrapper becomes _vitals_panel, so the
	# existing set_active(m==0) toggle still hides chrome + body together.
	_vitals_panel = PanelChrome.new()
	_vitals_panel.setup("📊 VITALS", body, ui, Vector2(12, 46), _pill_rail)


func _vital_label() -> Label:
	var l := Label.new()
	l.add_theme_font_size_override("font_size", 16)
	l.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
	return l


## Current vitals: {generation, population, fitness, allele}. Live → LiveSim.observe() (+ snapshot fitness
## mean); replay → snapshot field-means over POPULATED cells. Pure reads of core-exported data (inv #2).
func _vitals_source() -> Dictionary:
	if _live != null:
		var o: Dictionary = _live.observe()
		return {
			"generation": int(o.get("generation", 0)), "population": int(o.get("population", 0)),
			"allele": clampf(float(o.get("allele_freq", 0.0)), 0.0, 1.0), "fitness": _mean_pop_now(true)}
	if not _snaps.is_empty():
		var s = _snaps[_idx]
		return {
			"generation": s.generation, "population": s.population,
			"allele": _mean_pop(s.allele_freq, s.density), "fitness": _mean_pop(s.fitness, s.density)}
	return {}


## Mean of `values` over cells where density > 0 (the populated field). Read-only aggregate, no biology.
func _mean_pop(values: PackedFloat32Array, density: PackedFloat32Array) -> float:
	var sum := 0.0
	var n := 0
	for i in values.size():
		if i < density.size() and density[i] > 0.0:
			sum += values[i]
			n += 1
	return (sum / float(n)) if n > 0 else 0.0


func _mean_pop_now(_want_fitness: bool) -> float:
	if _snaps.is_empty():
		return 0.0
	var s = _snaps[_idx]
	return _mean_pop(s.fitness, s.density)


## ▲ / ▼ / = trend of `now` vs the previous tick's value for `key` (deterministic last-vs-now, no RNG).
func _trend(now: float, key: String) -> String:
	if not _prev_obs.has(key):
		return "·"
	var prev := float(_prev_obs[key])
	if absf(now - prev) <= maxf(0.0005, absf(prev) * 0.001):
		return "="
	return "▲" if now > prev else "▼"


## Item (b): a COLOR-CODED ▲/▼/→ trend glyph (green up / red down / neutral flat) as a BBCode span for the
## RichTextLabel title bar — the same deterministic last-vs-now comparison as _trend(), so the glyph agrees with
## the Vitals panel. Returns "→" (grey) before the first prior reading exists. Pure presentation (inv #2/#3).
func _trend_bbcode(now: float, key: String) -> String:
	if not _prev_obs.has(key):
		return "[color=#8a9690]→[/color]"
	var prev := float(_prev_obs[key])
	if absf(now - prev) <= maxf(0.0005, absf(prev) * 0.001):
		return "[color=#8a9690]→[/color]"
	if now > prev:
		return "[color=#54e35a]▲[/color]"
	return "[color=#e86464]▼[/color]"


## Refresh the title-bar chips + Vitals scoreboard + sparkline from the current vitals source.
func _refresh_vitals() -> void:
	var v := _vitals_source()
	if v.is_empty():
		return
	if _title_badge != null:
		_title_badge.text = "● LIVE" if _live != null else "REPLAY"
		_title_badge.add_theme_color_override(
			"font_color", Color(0.45, 0.92, 0.5) if _live != null else Color(0.7, 0.72, 0.74))
	if _title_status != null and _view_mode == 0:
		# Item (b): a color-coded ▲/▼/→ trend glyph next to each live vital chip (pop/fit/allele) so direction
		# reads at a glance — "population crashing" / "fitness diverging" — without opening the Vitals panel. The
		# glyphs use the SAME prev-vs-now comparison as the Vitals scoreboard, so they always agree. gen/seed get
		# no glyph (gen only ever climbs; seed is constant). BBCode → the RichTextLabel colorizes the spans.
		_title_status.text = "seed %d     gen %d     pop %d %s    fit %.2f %s    allele %.2f %s" % [
			_seed, int(v.generation),
			int(v.population), _trend_bbcode(float(v.population), "population"),
			float(v.fitness), _trend_bbcode(float(v.fitness), "fitness"),
			float(v.allele), _trend_bbcode(float(v.allele), "allele")]
	if _vitals_pop != null:
		_vitals_pop.text = "%s  Population    %d" % [_trend(float(v.population), "population"), int(v.population)]
		_vitals_fit.text = "%s  Mean fitness  %.2f" % [_trend(float(v.fitness), "fitness"), float(v.fitness)]
		_vitals_allele.text = "%s  Allele freq   %.2f" % [_trend(float(v.allele), "allele"), float(v.allele)]
	if _live == null:  # replay: plot the whole run; live appends per render in _publish_frame
		_fit_history = []
		_allele_history = []
		for s in _snaps:
			_fit_history.append(_mean_pop(s.fitness, s.density))
			_allele_history.append(_mean_pop(s.allele_freq, s.density))
	if _sparkline != null:
		_sparkline.set_series(_fit_history, _allele_history)
	_prev_obs = v


func _build_hud(ui: CanvasLayer, field_px: Vector2) -> void:
	# The dense status line lives in the title bar + Vitals panel now (S3); _build_hud only owns the legend.
	# Colormap legend: the active layer's name + the inferno gradient bar (low → high).
	var body := PanelContainer.new()
	var lsb := StyleBoxFlat.new()
	lsb.bg_color = Color(0.0, 0.0, 0.0, 0.42)
	lsb.set_corner_radius_all(6)
	lsb.set_content_margin_all(8)
	body.add_theme_stylebox_override("panel", lsb)
	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 3)
	body.add_child(col)
	_legend_label = Label.new()
	_legend_label.add_theme_font_size_override("font_size", 14)
	_legend_label.add_theme_color_override("font_color", Color(0.9, 0.94, 0.9))
	col.add_child(_legend_label)
	var bar := TextureRect.new()
	bar.texture = _legend_texture()
	bar.custom_minimum_size = Vector2(208, 12)
	bar.stretch_mode = TextureRect.STRETCH_SCALE
	col.add_child(bar)
	_legend = PanelChrome.new()
	_legend.setup("🎨 LEGEND", body, ui, Vector2(12, maxf(120.0, field_px.y - 52.0)), _pill_rail)


## 1-D inferno gradient texture matching data_layer.gdshader (low → high).
func _legend_texture() -> ImageTexture:
	var n := 208
	var img := Image.create(n, 12, false, Image.FORMAT_RGBA8)
	for x in n:
		var c := _inferno(float(x) / float(n - 1))
		for y in 12:
			img.set_pixel(x, y, c)
	return ImageTexture.create_from_image(img)


## CPU mirror of the shader's inferno ramp (for the legend bar only).
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


# ──────────────────────────── controls bar ────────────────────────────

## A bottom control bar (two rows): row 1 = view toggle / play-pause / step / data-layer picker; row 2 =
## playback-speed slider / zoom-scope buttons / generation scrubber. All change VIEW state only — no biology
## (invariant #2). Mirrors the keyboard shortcuts so the UI is discoverable.
func _build_controls(ui: CanvasLayer, field_px: Vector2) -> void:
	# Polished control deck (Phase U): a raised rounded card with a border + soft shadow, not a flat black slab.
	var body := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.06, 0.09, 0.08, 0.86)
	sb.set_corner_radius_all(8)
	sb.set_border_width_all(1)
	sb.border_color = Color(0.18, 0.4, 0.28, 0.55)
	sb.shadow_size = 6
	sb.shadow_color = Color(0.0, 0.0, 0.0, 0.35)
	sb.shadow_offset = Vector2(0.0, 2.0)
	sb.set_content_margin_all(10)
	body.add_theme_stylebox_override("panel", sb)

	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 8)
	body.add_child(rows)

	# Row 1 — playback / step / layer. (VIEW switching moved to the dedicated top-right VIEW+SCOPE panel.)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	rows.add_child(row)

	_play_button = Button.new()
	_play_button.text = "⏸ Pause"
	_play_button.pressed.connect(_on_play_pressed)
	row.add_child(_play_button)

	_prev_button = Button.new()
	_prev_button.text = "◀"
	_prev_button.pressed.connect(_step_rel.bind(-1))
	row.add_child(_prev_button)

	_next_button = Button.new()
	_next_button.text = "▶"
	_next_button.pressed.connect(_step_rel.bind(1))
	row.add_child(_next_button)

	var lbl := Label.new()
	lbl.text = "  Layer:"
	lbl.add_theme_color_override("font_color", Color(0.9, 0.94, 0.9))
	row.add_child(lbl)

	_layer_picker = OptionButton.new()
	for nm in OVERLAY_NAMES:
		_layer_picker.add_item(nm)
	_layer_picker.selected = _overlay_mode
	_layer_picker.item_selected.connect(_on_layer_selected)
	row.add_child(_layer_picker)

	# Row 2 — playback speed. (Zoom SCOPE moved to the dedicated top-right VIEW+SCOPE panel.)
	var row2 := HBoxContainer.new()
	row2.add_theme_constant_override("separation", 8)
	rows.add_child(row2)

	row2.add_child(_dim_label("Speed:"))
	_speed_slider = HSlider.new()
	_speed_slider.min_value = 0.5  # 0.5× … 4× playback speed
	_speed_slider.max_value = 4.0
	_speed_slider.step = 0.1
	_speed_slider.value = 1.0
	_speed_slider.custom_minimum_size = Vector2(90, 0)
	_speed_slider.value_changed.connect(_on_speed_changed)
	row2.add_child(_speed_slider)

	# (The generation scrubber is gone — the bottom timeline owns seek with a play-head + labels.)

	# Row 3 — run lifecycle (live only): Restart (same seed) / New run (Seed field) — deterministic re-runs.
	var row3 := HBoxContainer.new()
	row3.add_theme_constant_override("separation", 8)
	rows.add_child(row3)
	var live := _live != null
	_restart_button = Button.new()
	_restart_button.text = "⟳ Restart"
	_restart_button.tooltip_text = "Re-run from the same seed (deterministic)"
	_restart_button.pressed.connect(_on_restart_pressed)
	_restart_button.disabled = not live
	row3.add_child(_restart_button)
	_newrun_button = Button.new()
	_newrun_button.text = "✦ New run"
	_newrun_button.tooltip_text = "Start a fresh run from the Seed field"
	_newrun_button.pressed.connect(_on_newrun_pressed)
	_newrun_button.disabled = not live
	row3.add_child(_newrun_button)
	row3.add_child(_dim_label("  Seed:"))
	_seed_edit = LineEdit.new()
	_seed_edit.text = str(_seed)
	_seed_edit.custom_minimum_size = Vector2(110, 0)
	_seed_edit.editable = live
	_seed_edit.text_submitted.connect(func(_t): _on_newrun_pressed())
	row3.add_child(_seed_edit)
	var save_btn := Button.new()
	save_btn.text = "💾 Save"
	save_btn.tooltip_text = "Save this session's progress (the seeded action journal)"
	save_btn.pressed.connect(_on_save_pressed)
	save_btn.disabled = not live
	row3.add_child(save_btn)
	var load_btn := Button.new()
	load_btn.text = "📂 Load"
	load_btn.tooltip_text = "Restore the saved session (deterministic replay)"
	load_btn.pressed.connect(_on_load_pressed)
	load_btn.disabled = not live
	row3.add_child(load_btn)
	if not live:
		row3.add_child(_dim_label("  — launch with --live to restart / save / load"))

	# Wrap the deck in the panel chrome (drag handle + minimize), docked bottom-left above the timeline.
	_controls_panel = PanelChrome.new()
	_controls_panel.setup("🎛 CONTROLS", body, ui, Vector2(12, field_px.y + 16), _pill_rail)
	_sync_controls()


## The always-on top-right VIEW + SCOPE switcher (UI rework): a segmented VIEW row (Ecosystem / Specimen / Relations)
## on top, the zoom SCOPE row (Field / Patch / Cells) below it — SEPARATED from the CONTROLS deck so view + scope are
## always reachable in the same spot, in every view. Anchored to the top-right corner (grows leftward) so it hugs the
## corner at any window size. Renderer-only (inv #2): pure view/camera state, no biology. Built AFTER _build_controls
## (which no longer owns these) so _scope_buttons / _view_buttons live here.
func _build_viewscope_ui(ui: CanvasLayer) -> void:
	var body := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.06, 0.09, 0.08, 0.9)
	sb.set_corner_radius_all(8)
	sb.set_border_width_all(1)
	sb.border_color = Color(0.2, 0.46, 0.32, 0.7)
	sb.shadow_size = 6
	sb.shadow_color = Color(0.0, 0.0, 0.0, 0.4)
	sb.set_content_margin_all(8)
	body.add_theme_stylebox_override("panel", sb)

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 5)
	body.add_child(col)

	# VIEW switcher — 3 segmented toggles (one active, synced by _set_view_mode → _sync_view_buttons).
	var vcap := _dim_label("VIEW")
	vcap.add_theme_font_size_override("font_size", 10)
	col.add_child(vcap)
	var view_row := HBoxContainer.new()
	view_row.add_theme_constant_override("separation", 3)
	col.add_child(view_row)
	var vgrp := ButtonGroup.new()
	_view_buttons.clear()
	for i in VIEW_NAMES.size():
		var vb := Button.new()
		vb.text = VIEW_NAMES[i]
		vb.toggle_mode = true
		vb.button_group = vgrp
		vb.custom_minimum_size = Vector2(80, 0)
		vb.pressed.connect(_on_view_selected.bind(i))
		view_row.add_child(vb)
		_view_buttons.append(vb)
	_sync_view_buttons()

	# SCOPE — the zoom presets (pure camera zoom; _scope_buttons synced by _sync_scope_buttons() on any zoom change).
	# allow_unpress so an in-between (wheel) zoom shows none active. The scope drives the camera in every view.
	var scap := _dim_label("SCOPE")
	scap.add_theme_font_size_override("font_size", 10)
	col.add_child(scap)
	var scope_row := HBoxContainer.new()
	scope_row.add_theme_constant_override("separation", 3)
	col.add_child(scope_row)
	var sgrp := ButtonGroup.new()
	sgrp.allow_unpress = true
	_scope_buttons.clear()
	for i in SCOPES.size():
		var sbn := Button.new()
		sbn.text = str(SCOPES[i]["name"]).capitalize()
		sbn.toggle_mode = true
		sbn.button_group = sgrp
		sbn.custom_minimum_size = Vector2(62, 0)
		sbn.pressed.connect(_set_scope.bind(i))
		scope_row.add_child(sbn)
		_scope_buttons.append(sbn)

	# Pin to the top-right corner BELOW the title bar, sizing to content: all four anchors at the corner point +
	# grow LEFT/DOWN, with a zero-size offset rect (offset_left==offset_right, offset_top==offset_bottom) that the
	# container's min-size then expands. (Setting only 2 of 4 offsets leaves a degenerate rect → invisible.)
	ui.add_child(body)
	body.anchor_left = 1.0
	body.anchor_right = 1.0
	body.anchor_top = 0.0
	body.anchor_bottom = 0.0
	body.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	body.grow_vertical = Control.GROW_DIRECTION_END
	body.offset_left = -12
	body.offset_right = -12
	body.offset_top = 44  # clear the full-width title bar
	body.offset_bottom = 44
	_viewscope_panel = body
	_sync_scope_buttons()


## A small dimmed label used as an inline caption in the control bar.
func _dim_label(text: String) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_color_override("font_color", Color(0.82, 0.86, 0.82))
	return l


func _on_speed_changed(v: float) -> void:
	# Higher slider = faster. LIVE scales the decoupled step rate (_process); FILE replay scales the Timer.
	var speed := maxf(0.1, v)
	_steps_per_second = STEPS_PER_SECOND_BASE * speed
	_frame_seconds = FRAME_SECONDS / speed
	if _timer != null:
		_timer.wait_time = _frame_seconds
	_sync_controls()


## Restart the live run from the SAME seed (deterministic re-run, inv #3). Live-only.
func _on_restart_pressed() -> void:
	_do_reset(_seed)


## Start a fresh live run from the Seed field (or _seed+1 if blank/invalid). Live-only.
func _on_newrun_pressed() -> void:
	var txt := _seed_edit.text.strip_edges() if _seed_edit != null else ""
	_do_reset(int(txt) if txt.is_valid_int() else _seed + 1)


## Re-reset the LiveSim with `seed` and clear all presentation buffers (history, markers, timeline). The core
## re-seeds its single ChaCha8 stream, so the same seed → identical bytes (inv #3). Renderer requests; core
## computes (inv #2). No-op without a live sim.
func _do_reset(seed: int) -> void:
	if _live == null:
		return
	_seed = seed
	if _seed_edit != null:
		_seed_edit.text = str(seed)
	_live.reset(seed)
	_resync_to_live()


## Rebuild the renderer's rolling state from the live env's CURRENT state (after a reset or a load): one
## snapshot, cleared history/markers/timeline, unpaused. The mission progress (S-G2) is left intact.
func _resync_to_live() -> void:
	var snap = SnapshotReader.parse_bytes(_live.snapshot(LIVE_GRID.x, LIVE_GRID.y))
	if snap == null:
		return
	_snaps = [snap]
	_idx = 0
	_injections = []
	_registered_contaminants = {}  # a fresh core env has an empty consortium → re-register on next seed/schedule (ADR-019)
	_fit_history = []
	_allele_history = []
	_live_specimen_log = []  # fresh run → fresh specimen history
	_live_species_logs = {}
	_live_species_order = []
	_log_live_genome("baseline — gen 0")
	_prev_obs = {}
	_prev_pop = {}  # Item (c): fresh run → drop the per-species pop baseline so we don't falsely flash on reset
	if _alert_label != null:
		_alert_label.visible = false
	_paused = false
	_step_carry = 0.0  # fresh run → no owed steps / render backlog (the decoupled live loop, _process)
	_render_carry = 0.0
	if _timeline != null:
		_timeline.setup([snap.generation])
		_timeline.set_markers(_injections)
	_show(0)
	_update_play_button()


## Save the live session (the seeded action journal) to the default slot. Live-only.
func _on_save_pressed() -> void:
	if _live == null:
		return
	var ok: bool = _live.save_session(ProjectSettings.globalize_path(SAVE_DIR))
	_flash_status("💾 saved" if ok else "✗ save failed", ok)


## Load the saved session: LiveSim restores it by deterministic replay, then resync the renderer. Live-only.
func _on_load_pressed() -> void:
	if _live == null:
		return
	var r: Dictionary = _live.load_session(ProjectSettings.globalize_path(SAVE_DIR))
	if not bool(r.get("ok", false)):
		_flash_status("✗ load failed: " + str(r.get("detail", "no save")), false)
		return
	_resync_to_live()  # clears _injections; the markers are re-derived from the restored journal below
	_rebuild_markers_from_journal()
	_flash_status("📂 loaded — gen %d, %d actions" % [int(r.get("generation", 0)), int(r.get("actions", 0))], true)


## Re-derive the timeline intervention markers from the RESTORED journal after a load (SP-3.7). The journal is the
## source of truth: each region Action maps to the generation = the running sum of the preceding `Advance` counts.
## The markers are thus a DETERMINISTIC PROJECTION of the replayed journal, so a scrubbed/replayed session shows
## every intervention exactly where it fired. Read-only (inv #2): GDScript only reads the ordered Action tags the
## core exports; it computes no biology. Uses the forward-compat `journal_actions` #[func] when the cdylib exposes
## it (the same has_method probe used for observe_species/flow_matrix); without it the markers stay empty (a load
## still replays correctly — only the visual markers are absent until the export lands).
func _rebuild_markers_from_journal() -> void:
	_injections = []
	if _live != null and _live.has_method("journal_actions"):
		var gen := 0
		for entry in _live.journal_actions():
			var d: Dictionary = entry
			var kind := str(d.get("kind", ""))
			if kind == "advance":
				gen += int(d.get("n", 0))
				continue
			var tool := _journal_kind_to_tool(kind)
			if tool < 0:
				continue  # a non-marker action (e.g. a bare Advance) — nothing to place on the axis
			_injections.append({
				"generation": gen,
				"tool": TOOL_KEYS[tool],
				"applied": true,  # a journaled Action replayed → it landed (the journal records what fired)
				"label": str(d.get("detail", "")),
			})
	if _timeline != null:
		_timeline.set_markers(_injections)


## Map a journal Action `kind` tag (the LiveSim journal_actions export, when present) to a palette tool index, or
## -1 for actions that are not one of the five palette interventions. Pure string mapping (inv #2).
func _journal_kind_to_tool(kind: String) -> int:
	match kind:
		"apply_edit_region", "crispr": return TOOL_CRISPR
		"pcr_amplify", "pcr": return TOOL_PCR
		"cull", "region_cull": return TOOL_ANTIBIOTIC
		"nutrient", "region_nutrient": return TOOL_NUTRIENT
		"toxin", "region_toxin": return TOOL_TOXIN
		"inoculate", "region_inoculate": return TOOL_INOCULATE  # ADR-019: manual OR scheduled immigration marker
		_: return -1


## Flash a short message in the intervention status line (shared by save/load + edits).
func _flash_status(text: String, ok: bool) -> void:
	if _inject_status != null:
		_inject_status.text = text
		_inject_status.add_theme_color_override(
			"font_color", Color(0.5, 0.92, 0.52) if ok else Color(0.96, 0.55, 0.5))


## Push current state INTO the row-2 widgets without re-triggering their signals (re-entrancy guarded).
func _sync_controls() -> void:
	_syncing = true
	var eco := _view_mode == 0
	if _prev_button != null:
		_prev_button.disabled = not eco
	if _next_button != null:
		_next_button.disabled = not eco
	if _speed_slider != null:
		_speed_slider.editable = eco
	if _timeline != null:
		_timeline.set_index(_idx)
	_sync_scope_buttons()
	_syncing = false


## Reflect the current zoom scope in the toggle buttons (one pressed, or none at in-between zooms).
func _sync_scope_buttons() -> void:
	if _scope_buttons.is_empty():
		return
	var active := _scope_label()  # 'field' / 'patch' / 'cells'
	for i in _scope_buttons.size():
		(_scope_buttons[i] as Button).set_pressed_no_signal(str(SCOPES[i]["name"]) == active)


func _on_view_selected(m: int) -> void:
	_set_view_mode(m)


## Reflect the current view in the segmented VIEW toggles (one pressed). Synced by _set_view_mode so KEY_V cycling
## and the --view shot flag keep the top-right switcher in step.
func _sync_view_buttons() -> void:
	for i in _view_buttons.size():
		(_view_buttons[i] as Button).set_pressed_no_signal(i == _view_mode)


func _on_play_pressed() -> void:
	_paused = not _paused
	_update_play_button()
	_refresh_hud()


func _on_layer_selected(idx: int) -> void:
	_overlay_mode = idx
	if _view_mode == 0:
		_show(_idx)


func _update_play_button() -> void:
	if _play_button != null:
		_play_button.text = "▶ Play" if _paused else "⏸ Pause"


## Step one snapshot relative (ecosystem view only); pauses playback.
func _step_rel(delta: int) -> void:
	if _view_mode != 0 or _snaps.is_empty():
		return
	_paused = true
	_update_play_button()
	_show((_idx + delta + _snaps.size()) % _snaps.size())


# ──────────────────────────── specimen (L-system) view ────────────────────────────

## Switch the active view (0 ecosystem · 1 specimen · 2 relations). Pure VIEW state (inv #2): toggles node
## visibility + panel set_active; computes no biology. The Relations branch refreshes the FlowMatrix heatmap.
func _set_view_mode(m: int) -> void:
	_view_mode = m
	_world.visible = (m == VIEW_ECOSYSTEM)
	_specimen_root.visible = (m == VIEW_SPECIMEN)
	if _relations_root != null:
		_relations_root.visible = (m == VIEW_RELATIONS)
	if _relations_full != null:
		_relations_full.visible = (m == VIEW_RELATIONS)  # the full-window graph/heatmap content (like _specimen_root)
	if _codex_full != null:
		_codex_full.visible = (m == VIEW_CODEX)  # the full-window browsable encyclopedia (renderer-only, inv #2)
	if _vignette != null:
		_vignette.visible = (m == VIEW_ECOSYSTEM)  # screen-space edge darkening only suits the field view
	if _detail_panel != null:
		_detail_panel.visible = false  # clear stale inspection on view switch
	if _tooltip != null:
		_tooltip.visible = false
	if _timeline != null:
		# The matrix is per-generation (like the snapshot index), so the timeline stays visible in Relations too.
		_timeline.visible = (m == VIEW_ECOSYSTEM or m == VIEW_RELATIONS)
	if _intervention_panel != null:
		_intervention_panel.set_active(_live != null and m == VIEW_ECOSYSTEM)
	if _vitals_panel != null:
		_vitals_panel.set_active(m == VIEW_ECOSYSTEM)
		if m != VIEW_ECOSYSTEM:
			_set_brush_mode(false)  # the brush only makes sense in the ecosystem view
		if _mission_panel != null:
			_mission_panel.set_active(_mission_on and m == VIEW_ECOSYSTEM)
	_sync_view_buttons()  # keep the top-right segmented VIEW switcher in step (KEY_V cycle / --view flag / button)
	if _layer_picker != null:
		_layer_picker.disabled = (m != VIEW_ECOSYSTEM)  # the data-layer picker only drives the ecosystem overlay
	if _specimen_panel != null:
		_specimen_panel.set_active(m == VIEW_SPECIMEN)
	if _relations_panel != null:
		_relations_panel.set_active(m == VIEW_RELATIONS)
	if _containment_panel != null:
		# CONTAMINATION is an ecosystem-mode control (consortium schedule); gate it to the field view like the
		# intervention panel, so its (tall) body never overlaps the SPECIMEN / RELATIONS panels (both top-right).
		_containment_panel.set_active(_live != null and m == VIEW_ECOSYSTEM)
	if _oversight_panel != null:
		# OVERSIGHT is an ecosystem-mode economy control; gate it to the field view (like INTERVENE/CONTAMINATION)
		# and only when the oversight #[func]s exist (older cdylib / file replay → stays inactive, inv #2).
		_oversight_panel.set_active(_live != null and _live.has_method("oversight_state") and m == VIEW_ECOSYSTEM)
	if m == VIEW_SPECIMEN:
		# The specimen view now renders a GENUINE per-species body: a Microbe rod glyph for E. coli, the L-system
		# plant for the abstract species (branched in _render_specimens). No placeholder.
		_refresh_live_specimens()  # in --live there is no specimens.json — build one from the live genome
		_render_specimens()  # also repopulates the picker
		_update_trait_readout()
		_emphasise_focus()
		# Issue 2: frame the WHOLE grid ONCE on view-entry only. Switching the active specimen afterwards
		# (_on_specimen_selected) no longer re-zooms the camera — it just moves the highlight + the readout, so
		# the user's chosen zoom/pan is preserved while they browse species/variants.
		_frame_specimens()
	elif m == VIEW_RELATIONS:
		_refresh_relations()  # pull species names + the flat FlowMatrix, feed the heatmap (degrades gracefully)
		_frame_world()  # the relations graph/heatmap are full-window Controls (screen-space) — keep the world camera neutral
	elif m == VIEW_CODEX:
		_refresh_codex()  # (re)build the browsable entry list from codex.gd; degrades to an "unavailable" note
	else:
		_frame_world()
	_sync_controls()  # enable/disable scrubber + step for the new mode
	_refresh_hud()


## Flat list of specimens to draw: baseline first, then each edited genome.
## In --live mode there is no specimens.json, so synthesise the specimen list from the LIVE species genome's
## expressed phenotype (LiveSim.observe()). The plant's shape then reflects the current genome and updates as
## the player edits it. Read-only (inv #2): observe() exports the traits; the renderer only maps them to shape.
## The Debug-cased → snake_case trait-key translation (plant 9 + microbe 5), shared by every capture path.
## A species' phenotype dict carries ONLY that species' bound traits, so iterating its keys and translating each
## drops nothing (growth_rate is shared). Defined once here (was inline in _capture_live_traits).
const TRAIT_KEY_MAP := {
	# plant (9)
	"GrowthRate": "growth_rate", "Stature": "stature", "Branchiness": "branchiness",
	"LeafSize": "leaf_size", "LeafHue": "leaf_hue", "Reflectance": "reflectance",
	"Fecundity": "fecundity", "DroughtTolerance": "drought_tolerance",
	"KillSwitchLinkage": "kill_switch_linkage",
	# microbe (5) — E. coli phenotype (ecoli_trait_map); GrowthRate is shared with the plant set above.
	"GlucoseUptake": "glucose_uptake", "RespirationMode": "respiration_mode",
	"AcetateOverflow": "acetate_overflow", "FermentationCapacity": "fermentation_capacity",
	# predator / spore-former / obligate-symbiont diagnostic traits (SP-4 — previously DROPPED from the readout
	# because they were absent here, so a predator/spore-former/symbiont row showed no attack/sporulation/symbiosis
	# bar). These cross the boundary via observe_species()'s Debug-cased phenotype; add them so they render.
	"PredationCapacity": "predation_capacity", "SporulationCapacity": "sporulation_capacity",
	"SymbiosisCapacity": "symbiosis_capacity",
}


## Translate ANY core-exported phenotype dict (Debug-cased keys) into the snake_case keys the specimen view uses.
## Works for the active observe() phenotype AND every per-species observe_species() phenotype — pure renaming
## (no biology, inv #2). Factored out of the old _capture_live_traits so the multi-species fan-out reuses it.
func _capture_traits_from(pheno: Dictionary) -> Dictionary:
	var traits := {}
	for k in pheno:
		if TRAIT_KEY_MAP.has(k):
			traits[TRAIT_KEY_MAP[k]] = float(pheno[k])
	return traits


## The PRIMARY (active observe()) species' translated phenotype — back-compat thin wrapper over the factored map.
func _capture_live_traits() -> Dictionary:
	return _capture_traits_from(_live.observe().get("phenotype", {}))


## The {species_id:int -> {key, role}} map the GSS5 species-map visual table is built from. Authoritative source
## is the live observe_species() (current roster, in SpeciesId order); falls back to the per-species log
## (_live_species_logs) which carries the same {key, role}. Empty on file-replay / older cdylib → the field draws
## the default plant visual (graceful). Pure reads (inv #2): no biology, just id → display metadata.
func _species_id_meta() -> Dictionary:
	var meta: Dictionary = {}
	if _live != null and _live.has_method("observe_species"):
		for sp in _live.observe_species():
			var d: Dictionary = sp
			meta[int(d.get("species_id", 0))] = {
				"key": str(d.get("key", "default")),
				"role": str(d.get("role", "")),
			}
	# Backfill from the per-species log for any id not in the live observation (e.g. just-extinct species).
	for sid in _live_species_logs:
		if not meta.has(int(sid)):
			var le: Dictionary = _live_species_logs[sid]
			meta[int(sid)] = {"key": str(le.get("key", "default")), "role": str(le.get("role", ""))}
	return meta


## The species template the ECOSYSTEM field renders with: {traits, key}. This sets the run-level glyph TEMPLATE
## (the active species' phenotype → sprite morph). The per-cell SPECIES IDENTITY now rides on the GSS5
## dominant_species_id plane (sized/coloured via SpeciesVisualMap in _show), so a multi-species field draws each
## cell's dominant species at its real-cell scale on top of this template. Live → the active observe() species
## (with its core key); file-replay → the specimens.json baseline (plant). Pure reads (inv #2).
func _ecosystem_species_traits() -> Dictionary:
	if _live != null:
		var key := "default"
		if _live.has_method("species_key"):
			var k := String(_live.species_key())
			if k != "":
				key = k
		return {"traits": _capture_live_traits(), "key": key}
	# File-replay: the plant baseline from specimens.json (key defaults to plant).
	if _specimens.has("baseline"):
		return {"traits": (_specimens["baseline"] as Dictionary).get("traits", {}), "key": "default"}
	return {"traits": {}, "key": "default"}


## Append the current genome state of EVERY species to its own per-species log — but only entries that DIFFER
## from that species' last entry. The species genome changes only on a WHOLE-species CRISPR edit (selection
## drives per-individual alleles, not the genome), and an edit targets the ACTIVE species, so a CRISPR edit logs
## a new specimen under exactly the edited species (the others' traits are unchanged → no duplicate). Fed by the
## read-only observe_species() (every species' baseline+edits cross the boundary; inv #2/#3). The `label` carries
## the edit/gen context; per-species entries are suffixed so the picker reads "Species — baseline / edit N".
func _log_live_genome(label: String) -> void:
	if _live == null:
		return
	if not _live.has_method("observe_species"):
		# Older cdylib without the per-species export → fall back to the single primary-species log.
		_log_primary_genome(label)
		return
	for sp in _live.observe_species():
		var spd: Dictionary = sp
		var sid := int(spd.get("species_id", 0))
		var key := str(spd.get("key", "default"))
		var sname := str(spd.get("name", "species"))
		var role := str(spd.get("role", ""))  # SP-4: the Debug-cased TrophicRole, for the glyph + codex join
		var traits := _capture_traits_from(spd.get("phenotype", {}))
		if not _live_species_logs.has(sid):
			_live_species_logs[sid] = {"key": key, "name": sname, "role": role, "entries": []}
			_live_species_order.append(sid)
		var log_entry: Dictionary = _live_species_logs[sid]
		var entries: Array = log_entry["entries"]
		if not entries.is_empty() and (entries.back() as Dictionary).get("traits", {}) == traits:
			continue  # unchanged genome for this species — don't log a duplicate
		# Label: "baseline — …" for the first entry; "edit N — …" thereafter (per-species edit count).
		var per_label := label
		if not entries.is_empty():
			per_label = "edit %d — gen %d" % [entries.size(), int(_live.observe().get("generation", 0))]
		entries.append({"label": per_label, "traits": traits})
	_live_species_order.sort()  # stable SpeciesId order (inv #3)
	# Keep the legacy flat log mirroring the PRIMARY species (species_id 0) for any back-compat reader.
	if _live_species_order.size() > 0:
		_live_specimen_log = (_live_species_logs[_live_species_order[0]] as Dictionary)["entries"]


## Issue 4: FORCE-append a new "edit N" variant to the ACTIVE (just-edited) species' history — regardless of the
## trait dedup — because a whole-species inject is a deliberate user action whose new genome must always show as a
## new glyph, even when its 5-trait projection is unchanged this frame. First make sure every species' baseline is
## logged (so the edited species has a "baseline" entry to grow from), then append the freshly re-expressed traits
## of the active species under an "edit N" caption. The active species is the one whose `key` matches the core's
## species_key(); falls back to the primary (first SpeciesId). Read-only (inv #2): pure projection of the core's
## already-edited phenotype + species key into the renderer's history list.
func _append_active_edit_variant(generation: int) -> void:
	# The active species = the one whose key matches the core's species_key() (the whole-species inject targets it).
	_append_edit_variant_for(_active_species_id(), "edit", generation)


## The SpeciesId of the active species (its key == the core's species_key()); the primary (first id) when there's
## no match or no species_key() export. Pure read (inv #2): a string compare over the already-logged species.
func _active_species_id() -> int:
	if _live_species_order.is_empty():
		return -1
	var active_key := ""
	if _live != null and _live.has_method("species_key"):
		active_key = String(_live.species_key())
	if active_key != "":
		for sid in _live_species_order:
			var le: Dictionary = _live_species_logs[sid]
			if str(le.get("key", "")) == active_key:
				return int(sid)
	return int(_live_species_order[0])


## Item 2(a)/Issue 4: FORCE-append a new variant glyph to ONE species' history (`target_sid`), captioned
## "<kind> N — gen G" (kind = "edit" for a whole-species inject, "region edit" for a brush stroke). A deliberate
## user intervention must ALWAYS surface a fresh specimen glyph for investigation — even when the species'
## 5-trait projection is visually unchanged this frame (a whole-species edit may re-express only next generation;
## a region edit changes per-individual alleles, not the species genome at all). The trait-dedup paths
## (_log_live_genome) would suppress that, so this bypasses dedup. Read-only (inv #2): pure projection of the
## core's already-expressed phenotype + species key into the renderer's history list — no biology in GDScript.
func _append_edit_variant_for(target_sid: int, kind: String, generation: int) -> void:
	if _live == null:
		return
	# Ensure baselines exist for every species (no-op once seeded; dedups internally).
	if _live_species_order.is_empty():
		_log_live_genome("baseline — gen %d" % generation)
	if _live_species_order.is_empty():
		return  # observe_species() returned nothing AND _log_primary_genome could not seed — give up gracefully
	# Resolve the target log; an unknown/absent id falls back to the primary species so a stroke always lands.
	if not _live_species_logs.has(target_sid):
		target_sid = int(_live_species_order[0])
	var log_entry: Dictionary = _live_species_logs[target_sid]
	var entries: Array = log_entry["entries"]
	# Capture the target species' CURRENT (post-edit, re-expressed) traits from its observe_species row if present,
	# else from the active observe() phenotype — either is the core's authoritative projection (inv #2).
	var traits := _capture_live_traits()
	if _live.has_method("observe_species"):
		for sp in _live.observe_species():
			var spd: Dictionary = sp
			if int(spd.get("species_id", -1)) == target_sid:
				traits = _capture_traits_from(spd.get("phenotype", {}))
				break
	var n := entries.size()  # the first edit after the baseline is "<kind> 1"
	entries.append({"label": "%s %d — gen %d" % [kind, maxi(1, n), generation], "traits": traits})
	# Keep the legacy flat mirror in sync (primary species).
	if _live_species_order.size() > 0:
		_live_specimen_log = (_live_species_logs[_live_species_order[0]] as Dictionary)["entries"]


## Single-active-species fallback (older cdylib without observe_species): mirrors the pre-fan-out behaviour, but
## ALSO seeds the per-species structures (_live_species_logs / _live_species_order) so _specimen_list() takes the
## per-species path and renders the ACTUAL species' baseline glyph at gen 0 — never falling through to the empty
## file-replay branch (which is {} in --live) and showing a blank/default-plant specimen view. The single synthetic
## species row is tagged with the CORE species_key() (the authoritative glyph tiebreak — "bacillus"/"ecoli-core"/
## "default") so GlyphFactory draws the right morphotype with zero steps. Read-only (inv #2): species_key()/observe()
## are pure core projections; the renderer only maps the already-expressed phenotype to a glyph.
func _log_primary_genome(label: String) -> void:
	var traits := _capture_live_traits()
	# Resolve the active species' core key (glyph tiebreak) + a display name, with graceful fallbacks for an older
	# cdylib that lacks species_key() (then key "" → the plant L-system, the historical single-species look).
	var key := ""
	if _live != null and _live.has_method("species_key"):
		key = String(_live.species_key())
	var sname := _species_display_name(key)
	if _live_species_order.is_empty():
		_live_species_logs[0] = {"key": key, "name": sname, "role": "", "entries": []}
		_live_species_order = [0]
	var entry: Dictionary = _live_species_logs[0]
	var entries: Array = entry["entries"]
	if not entries.is_empty() and (entries.back() as Dictionary).get("traits", {}) == traits:
		return  # unchanged genome — don't log a duplicate
	var per_label := label
	if not entries.is_empty():
		per_label = "edit %d — gen %d" % [entries.size(), int(_live.observe().get("generation", 0))]
	entries.append({"label": per_label, "traits": traits})
	# Keep the legacy flat log mirroring this primary species for any back-compat reader.
	_live_specimen_log = entries

## A human-readable display name for the primary species from its core key (presentation only, inv #2). Falls back
## to a Titled key, or "Plant" for the abstract "" / "default" species (the historical single-species caption).
func _species_display_name(key: String) -> String:
	if key == "" or key == "default":
		return "Plant"
	return key.capitalize()


func _refresh_live_specimens() -> void:
	if _live == null:
		return
	if _live_specimen_log.is_empty() and _live_species_logs.is_empty():
		_log_live_genome("baseline — gen %d" % int(_live.observe().get("generation", 0)))
	# Guarantee a non-empty specimen list at gen 0 (with ZERO steps): _log_live_genome should have populated the
	# per-species log from observe_species(), but if that export is absent OR returned an empty roster, the
	# per-species path would be empty and _specimen_list() would fall through to the file-replay branch — which is
	# {} in --live → a BLANK specimen view. Seed the active species' baseline directly from its observe row so the
	# registered species' baseline morphology always renders immediately. Read-only (inv #2).
	if _live_species_order.is_empty():
		_log_primary_genome("baseline — gen %d" % int(_live.observe().get("generation", 0)))
	# _specimen_list() now flattens the per-species logs directly; clamp focus into the new flat range.
	_focus = clampi(_focus, 0, maxi(0, _specimen_list().size() - 1))


## The flat, ordered specimen row: every species' baseline + edits, grouped by species (SpeciesId order), each
## entry carrying its own `key` so the per-row glyph (microbe rod vs L-system) and the readout dispatch per
## species. File-replay (specimens.json, plant-only) keeps its baseline/edits shape, tagged key "default".
func _specimen_list() -> Array:
	var out: Array = []
	# --live: walk per-species logs in SpeciesId order; caption each "Species — baseline / edit N".
	if not _live_species_order.is_empty():
		for sid in _live_species_order:
			var log_entry: Dictionary = _live_species_logs[sid]
			var key := str(log_entry.get("key", "default"))
			var sname := str(log_entry.get("name", "species"))
			var role := str(log_entry.get("role", ""))
			for e in (log_entry["entries"] as Array):
				var ed: Dictionary = e
				out.append({
					"label": "%s — %s" % [sname, str(ed.get("label", ""))],
					"traits": ed.get("traits", {}),
					"key": key,
					"role": role,  # SP-4: the per-row trophic role (glyph fallback + codex role join)
					"group": sid,  # issue 5: the SpeciesId — entries sharing it form one grid ROW (species)
					"species_name": sname,  # the bare species name (for the per-row caption)
				})
		return out
	# File-replay / single-species fallback: the specimens.json baseline+edits, all plant ("default").
	if _specimens.has("baseline"):
		out.append(_with_key(_specimens["baseline"]))
	if _specimens.has("edits"):
		for e in _specimens["edits"]:
			out.append(_with_key(e))
	return out


## Tag a legacy (plant) specimen dict with the plant key so per-row dispatch treats it as a plant.
func _with_key(spec: Variant) -> Dictionary:
	var d: Dictionary = (spec as Dictionary).duplicate()
	if not d.has("key"):
		d["key"] = "default"
	if not d.has("group"):
		d["group"] = 0  # issue 5: file-replay is single-species → all variants share one grid row
	return d


## The under-glyph caption node. Alive → the crisp outlined Label (the existing look). Extinct (Item 2(b)) → a
## greyed, STRUCK-THROUGH RichTextLabel ("✟ … — extinct") so the dead species reads as dead while staying in the
## grid for investigation. Both are `label_w` wide + centered; the caller sets `.position`. Presentation only (inv #2).
func _specimen_caption(text: String, extinct: bool, label_w: float) -> Control:
	if not extinct:
		var label := Label.new()
		label.text = text
		label.add_theme_font_size_override("font_size", 15)
		label.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
		label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.9))
		label.add_theme_constant_override("outline_size", 6)
		label.size = Vector2(label_w, 0)
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		return label
	var rt := RichTextLabel.new()
	rt.bbcode_enabled = true
	# Pin the width to label_w (fit_content OFF so a long "✟ <name> — extinct" can't grow the control past label_w
	# and offset the [center]ed caption); a fixed one-line height matches the alive Label's single row.
	rt.fit_content = false
	rt.scroll_active = false
	rt.autowrap_mode = TextServer.AUTOWRAP_OFF
	rt.custom_minimum_size = Vector2(label_w, 26)
	rt.size = Vector2(label_w, 26)
	rt.add_theme_font_size_override("normal_font_size", 15)
	# [s] strikethrough + a tombstone glyph + a muted grey; centered to match the Label layout.
	rt.text = "[center][color=#9aa0a0][s]✟ %s — extinct[/s][/color][/center]" % text
	return rt


## Build one glyph per specimen (via the key-led GlyphFactory), laid out as a 2D GRID with a caption under each:
## species stack VERTICALLY (one row each), their variations run HORIZONTALLY to the right (issue 5). The glyph
## geometry comes from the core-exported trait vector + role + key (presentation mapping — no biology, inv #2).
func _render_specimens() -> void:
	# Synchronous teardown: queue_free() is DEFERRED, so the stale holders would linger in get_children()
	# this same frame and _emphasise_focus / _frame_specimens (run right after on a view re-entry) would
	# index the wrong (old) holder and dim the real focused plant. remove_child + free drops them at once.
	for c in _specimen_root.get_children():
		_specimen_root.remove_child(c)
		c.free()
	var list := _specimen_list()
	_specimen_bounds = Rect2()
	if list.is_empty():
		return
	# Build every glyph FIRST so we can lay them out with ADAPTIVE spacing: glyph sizes now vary wildly (a tiny
	# symbiont speck vs a tall mold conidiophore vs a plant), so a flat 300 either loses the specks or overlaps
	# the molds. We place each cell using the running cursor + the previous/next half-widths from bounds().
	var glyphs: Array = []  # [Node2D] one built glyph per specimen, in list order
	for i in list.size():
		var spec: Dictionary = list[i]
		# Per-ROW dispatch on the specimen's OWN species key + role (not the global active species) via the
		# key-led GlyphFactory: a mixed roster draws a rod for E. coli, a comma for Bdellovibrio, a mold for
		# Aspergillus, a speck for the symbiont, an L-system tree for the plant — ALL in the same view. Each
		# glyph honours the Node2D + build(Dictionary) + bounds()->Rect2 contract. Presentation only (inv #2).
		var key := str(spec.get("key", "default"))
		var role := str(spec.get("role", ""))
		glyphs.append(GlyphFactory.make(key, role, spec.get("traits", {}), spec, i + 1))

	# Issue 5: a 2D GRID — each SPECIES is its own ROW (stacked vertically, one under another); within a species
	# its VARIATIONS (baseline + edit 1 + edit 2 …) advance HORIZONTALLY to the right. Entries arrive grouped by
	# species (_specimen_list walks per-species in SpeciesId order), so a change in the `group` field marks a new
	# row. Holders are still added in flat list order, so child index == focus index (the picker + _emphasise_focus
	# map unchanged). X uses the same ADAPTIVE half-width spacing as before (glyph sizes vary wildly); each new row
	# steps Y down by the PREVIOUS row's measured height so a tall mold row never collides with the next.
	const GAP := 120.0  # breathing room between adjacent glyph bounding boxes within a row
	const LABEL_W := 220.0
	const ROW_GAP := 90.0  # vertical breathing room between species rows
	var union := Rect2()
	var has_union := false
	var cursor := 0.0  # x of the current cell's ORIGIN within the active row
	var prev_half_right := 0.0  # how far the previous glyph extended to the right of its origin (this row)
	var row_y := 0.0  # y of the current row's glyph origins
	var row_top := 0.0  # topmost extent of the current row (most-negative y), measured as we place
	var row_bottom := 0.0  # bottommost extent of the current row (caption baseline included)
	var row_started := false
	var cur_group: Variant = null  # the SpeciesId of the active row (null until the first entry)
	for i in list.size():
		var spec: Dictionary = list[i]
		var glyph: Node2D = glyphs[i]
		var pb: Rect2 = glyph.bounds()
		var grp: Variant = spec.get("group", 0)
		# A glyph extends pb.position.x .. pb.position.x+pb.size.x around its origin. The label box spans ±LABEL_W/2.
		var half_left := maxf(LABEL_W * 0.5, -pb.position.x)
		var half_right := maxf(LABEL_W * 0.5, pb.position.x + pb.size.x)

		var new_row: bool = row_started and grp != cur_group
		if new_row:
			# Drop to the next row: advance Y below the previous row's lowest extent + the row gap, reset the X cursor.
			row_y = row_bottom + ROW_GAP - pb.position.y  # align the new glyph's TOP just under the prior row
			cursor = 0.0
			prev_half_right = 0.0
			row_top = row_y + pb.position.y
			row_bottom = row_y + pb.position.y
		elif not row_started:
			row_y = -pb.position.y if pb.position.y < 0.0 else 0.0  # seed the first row at the top
			row_top = row_y + pb.position.y
			row_bottom = row_y + pb.position.y
		if row_started and not new_row:
			cursor += prev_half_right + GAP + half_left
		prev_half_right = half_right
		cur_group = grp
		row_started = true

		var holder := Node2D.new()
		holder.position = Vector2(cursor, row_y)
		_specimen_root.add_child(holder)
		holder.add_child(glyph)

		# Item 2(b): a species whose population has crashed to 0 stays in the grid (for investigation) but reads as
		# DEAD — its caption is struck-through + greyed and its glyph is desaturated/dimmed. The extinct set is keyed
		# by SpeciesId (the row `group`), maintained by _poll_population_alerts. Presentation only (inv #2).
		var is_extinct: bool = _extinct.has(int(spec.get("group", 0)))
		var label := _specimen_caption(str(spec.get("label", "specimen")), is_extinct, LABEL_W)
		label.position = Vector2(-LABEL_W * 0.5, maxf(24.0, pb.position.y + pb.size.y + 12.0))
		holder.add_child(label)
		if is_extinct:
			# Bake the dimming onto the GLYPH node (not the holder) so it survives _emphasise_focus's holder modulate
			# — an extinct species is desaturated/dim whether or not it's the focused specimen.
			glyph.modulate = Color(0.62, 0.62, 0.62, 0.72)

		# Track this row's vertical extent (glyph top + caption bottom) so the next row drops below it.
		row_top = minf(row_top, holder.position.y + pb.position.y)
		row_bottom = maxf(row_bottom, holder.position.y + label.position.y + 44.0)

		var wb := Rect2(holder.position + pb.position, pb.size).merge(
			Rect2(holder.position + label.position, Vector2(LABEL_W, 44)))
		if has_union:
			union = union.merge(wb)
		else:
			union = wb
			has_union = true
	_specimen_bounds = union
	_populate_specimen_picker()  # keep the A1 selector in sync with the rebuilt grid


## On-screen size of the ecosystem field — the iso diamond bbox under --iso, else the ortho rectangle.
func _field_screen_size() -> Vector2:
	if _iso != null and not _snaps.is_empty():
		return _iso.field_bounds(_snaps[0].width, _snaps[0].height, _cell).size
	return _field_px


## On-screen centre of the field (camera target).
func _field_center() -> Vector2:
	if _iso != null and not _snaps.is_empty():
		var b: Rect2 = _iso.field_bounds(_snaps[0].width, _snaps[0].height, _cell)
		return b.position + b.size * 0.5
	return _field_px * 0.5


## Grid cell under a world point — inverse iso transform under --iso, else the ortho division.
func _cell_at(world: Vector2) -> Vector2i:
	if _iso != null:
		var c: Vector2 = _iso.screen_to_cell(world, _cell)
		return Vector2i(int(floor(c.x)), int(floor(c.y)))
	return Vector2i(int(floor(world.x / _cell)), int(floor(world.y / _cell)))


func _frame_world() -> void:
	_cam.position = _field_center()
	_cam.zoom = Vector2.ONE


func _frame_specimens() -> void:
	if _specimen_bounds.size == Vector2.ZERO:
		_cam.position = Vector2.ZERO
		_cam.zoom = Vector2.ONE
		return
	var vp := get_viewport_rect().size
	var z := minf(vp.x / _specimen_bounds.size.x, vp.y / _specimen_bounds.size.y) * 0.82
	_cam.zoom = Vector2(z, z)
	_cam.position = _specimen_bounds.position + _specimen_bounds.size * 0.5


# ──────────────────────────── specimen UX panel (A1) ────────────────────────────

## A top-right panel for the specimen view: a picker to focus one specimen + a readout of its 5 trait values
## as bars with a delta-vs-baseline arrow. Reads only the core-exported trait vectors (presentation, inv #2).
func _build_specimen_ui(ui: CanvasLayer, field_px: Vector2) -> void:
	var body := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.5)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(10)
	body.add_theme_stylebox_override("panel", sb)
	body.custom_minimum_size = Vector2(288, 0)

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 6)
	body.add_child(col)

	_specimen_picker = OptionButton.new()
	_specimen_picker.item_selected.connect(_on_specimen_selected)
	_specimen_picker.clip_text = true  # long strain titles ("Escherichia coli K-12 core — baseline — gen 0") ellipsize inside the panel instead of overflowing its right edge
	col.add_child(_specimen_picker)

	# Per-species VITALS row (Rel-UI.1): a compact 3-up Population / Allele / Fitness block, each a value + ▲▼
	# trend, inserted between the picker and the Traits header. Populated in _update_trait_readout via
	# _species_stat: the PRIMARY (active observe()) species reads run-level observe()/snapshot today; non-primary
	# species (and any field the core does not yet expose) render "—" + a "pending core export" note. When the
	# Layer-B core widening lands (population_size/allele_freq/mean_energy on SpeciesObservation) the same label
	# loop shows live numbers for EVERY species with NO further GDScript change (it already reads obs.get(...)).
	_species_vital_rows.clear()
	var vitals_hdr := Label.new()
	vitals_hdr.text = "Vitals"
	vitals_hdr.add_theme_font_size_override("font_size", 12)
	vitals_hdr.add_theme_color_override("font_color", Color(0.7, 0.78, 0.7))
	col.add_child(vitals_hdr)
	# {label, stat-key (the obs.get key for non-primary), %-format} per row, in display order.
	var vital_specs := [
		{"label": "Population", "key": "population_size", "fmt": "%d"},
		{"label": "Allele", "key": "allele_freq", "fmt": "%.2f"},
		{"label": "Fitness", "key": "mean_fitness", "fmt": "%.2f"},
	]
	for vs in vital_specs:
		var vrow := HBoxContainer.new()
		vrow.add_theme_constant_override("separation", 6)
		col.add_child(vrow)
		var vname := Label.new()
		vname.text = str(vs["label"])
		vname.custom_minimum_size = Vector2(86, 0)
		vname.add_theme_font_size_override("font_size", 11)
		vname.add_theme_color_override("font_color", Color(0.86, 0.9, 0.86))
		vrow.add_child(vname)
		var vval := _vital_label()  # value + ▲▼ glyph, reusing the existing vitals label style
		vval.custom_minimum_size = Vector2(150, 0)
		vrow.add_child(vval)
		_species_vital_rows.append({"key": str(vs["key"]), "fmt": str(vs["fmt"]), "value": vval})
	# A one-line note shown only when a non-primary / unexposed stat reads "—" (cleared when all stats are live).
	_species_vital_note = Label.new()
	_species_vital_note.add_theme_font_size_override("font_size", 10)
	_species_vital_note.add_theme_color_override("font_color", Color(0.6, 0.62, 0.6))
	_species_vital_note.visible = false
	col.add_child(_species_vital_note)

	var traits_hdr := Label.new()
	traits_hdr.text = "Traits  (vs baseline)"
	traits_hdr.add_theme_font_size_override("font_size", 12)
	traits_hdr.add_theme_color_override("font_color", Color(0.7, 0.78, 0.7))
	col.add_child(traits_hdr)

	# Build ONE row per trait at the MAX of the plant (9) and microbe (5) sets; _update_trait_readout sets each
	# row's NAME + value per-species and hides the rows beyond the active species' key count. The name text is no
	# longer baked here (it was plant-only) — the readout drives it so the panel shows the active phenotype.
	_trait_rows.clear()
	var max_rows: int = maxi(TRAIT_KEYS.size(), MICROBE_TRAIT_KEYS.size())
	for ri in max_rows:
		var key: String = TRAIT_KEYS[ri] if ri < TRAIT_KEYS.size() else ""
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 6)
		col.add_child(row)

		var name_lbl := Label.new()
		name_lbl.text = key
		name_lbl.custom_minimum_size = Vector2(118, 0)
		name_lbl.add_theme_font_size_override("font_size", 11)
		name_lbl.add_theme_color_override("font_color", Color(0.86, 0.9, 0.86))
		row.add_child(name_lbl)

		var bar := ProgressBar.new()
		bar.min_value = 0.0
		bar.max_value = 1.0
		bar.show_percentage = false
		bar.custom_minimum_size = Vector2(56, 12)
		var bg := StyleBoxFlat.new()
		bg.bg_color = Color(1, 1, 1, 0.10)
		bg.set_corner_radius_all(3)
		var fill := StyleBoxFlat.new()
		fill.bg_color = Color(0.45, 0.78, 0.45)
		fill.set_corner_radius_all(3)
		bar.add_theme_stylebox_override("background", bg)
		bar.add_theme_stylebox_override("fill", fill)
		row.add_child(bar)

		var val_lbl := Label.new()
		val_lbl.custom_minimum_size = Vector2(40, 0)
		val_lbl.add_theme_font_size_override("font_size", 11)
		val_lbl.add_theme_color_override("font_color", Color(0.94, 0.98, 0.94))
		row.add_child(val_lbl)

		var delta_lbl := Label.new()
		delta_lbl.custom_minimum_size = Vector2(54, 0)
		delta_lbl.add_theme_font_size_override("font_size", 11)
		row.add_child(delta_lbl)

		_trait_rows.append({"name": name_lbl, "bar": bar, "value": val_lbl, "delta": delta_lbl})

	# Variant Lab — Slice B: SAVE the focused species' CURRENT (post-edit) genome as a named variant. Exports the
	# opaque SpeciesSpec JSON via LiveSim.export_species_json (the single biology→spec mapping lives in the core,
	# inv #2) into the renderer-side _saved_variants store; the CONTAMINATION panel's "Saved variants" section then
	# offers each one as a 🌱 Reseed inoculate payload (Slice C). Guarded: enabled only in --live with the export
	# #[func] present (a has_method probe, like observe_species) — disabled otherwise (file replay / older cdylib).
	var save_hdr := Label.new()
	save_hdr.text = "Variant Lab"
	save_hdr.add_theme_font_size_override("font_size", 12)
	save_hdr.add_theme_color_override("font_color", Color(0.7, 0.78, 0.7))
	col.add_child(save_hdr)
	var save_row := HBoxContainer.new()
	save_row.add_theme_constant_override("separation", 6)
	col.add_child(save_row)
	_save_variant_name = LineEdit.new()
	_save_variant_name.placeholder_text = "variant name…"
	_save_variant_name.custom_minimum_size = Vector2(150, 0)
	save_row.add_child(_save_variant_name)
	_save_variant_button = Button.new()
	_save_variant_button.text = "💾 Save variant"
	_save_variant_button.tooltip_text = "Snapshot the focused species' current genome as a reusable variant; reseed it from the 🦠 CONTAMINATION panel"
	_save_variant_button.pressed.connect(_on_save_variant_pressed)
	save_row.add_child(_save_variant_button)
	var can_save: bool = _live != null and _live.has_method("export_species_json")
	_save_variant_button.disabled = not can_save
	_save_variant_name.editable = can_save

	_specimen_panel = PanelChrome.new()
	_specimen_panel.setup("🌱 SPECIMEN", body, ui, Vector2(maxf(240.0, field_px.x - 304.0), 176.0), _pill_rail)  # y=176 clears the always-on VIEW+SCOPE panel
	_specimen_panel.set_active(false)


## Build the Relations view chrome (Rel-UI.0): a docked fixed-Control panel holding the S×S FlowMatrix heatmap,
## a degrade-state banner, and a diverging sign/magnitude legend. The heatmap reads core-measured integers only
## (inv #2): the renderer paints them as colored cells + printed numbers and computes no biology.
func _build_relations_ui(ui: CanvasLayer, field_px: Vector2) -> void:
	# FULL-WINDOW relations content (UI rework): the graph + heatmap fill the field area between the title bar and the
	# bottom timeline — like the specimen view — instead of a cramped corner panel. Visible only in the Relations view;
	# the compact info/toggle CARD (below) floats over it. mouse-IGNORE so the floating card still receives clicks.
	_relations_full = Control.new()
	_relations_full.set_anchors_preset(Control.PRESET_FULL_RECT)
	_relations_full.offset_top = 44.0       # clear the full-width title bar
	_relations_full.offset_bottom = -58.0   # clear the bottom timeline
	_relations_full.offset_left = 8.0
	_relations_full.offset_right = -8.0
	_relations_full.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_relations_full.visible = false
	ui.add_child(_relations_full)

	# The node-link GRAPH (default) + the S×S heatmap, BOTH full-rect inside the container; the toggle swaps which is
	# shown. Each reads the core-MEASURED FlowMatrix (+ populations) in _refresh_relations — only the drawing differs.
	_relations_graph = RelationsGraph.new()
	_relations_graph.set_anchors_preset(Control.PRESET_FULL_RECT)
	_relations_graph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_relations_full.add_child(_relations_graph)

	_relations_heatmap = RelationsHeatmap.new()
	_relations_heatmap.set_anchors_preset(Control.PRESET_FULL_RECT)
	_relations_heatmap.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_relations_full.add_child(_relations_heatmap)

	# ── The compact info / toggle CARD (docked top-right, below the always-on VIEW+SCOPE panel) ──
	var body := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.62)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(10)
	body.add_theme_stylebox_override("panel", sb)
	body.custom_minimum_size = Vector2(300, 0)

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 6)
	body.add_child(col)

	# Graph / Matrix segmented toggle (the GRAPH is the default representation; the matrix is one click away).
	var rep_row := HBoxContainer.new()
	rep_row.add_theme_constant_override("separation", 4)
	var rep_grp := ButtonGroup.new()
	_relations_graph_btn = Button.new()
	_relations_graph_btn.text = "🕸 Graph"
	_relations_graph_btn.tooltip_text = "Node-link web: species as nodes (sized by population, coloured by morphotype),\nedges = measured net joule flows (arrow = source→sink, thickness = magnitude)."
	_relations_graph_btn.toggle_mode = true
	_relations_graph_btn.button_group = rep_grp
	_relations_graph_btn.pressed.connect(_on_relations_rep_selected.bind(true))
	rep_row.add_child(_relations_graph_btn)
	_relations_matrix_btn = Button.new()
	_relations_matrix_btn.text = "▦ Matrix"
	_relations_matrix_btn.tooltip_text = "S×S FlowMatrix heatmap (rows = sink/gains, cols = source/gives)."
	_relations_matrix_btn.toggle_mode = true
	_relations_matrix_btn.button_group = rep_grp
	_relations_matrix_btn.pressed.connect(_on_relations_rep_selected.bind(false))
	rep_row.add_child(_relations_matrix_btn)
	col.add_child(rep_row)

	var cap := Label.new()
	cap.text = "rows = sink (gains) · cols = source (gives)"
	cap.add_theme_font_size_override("font_size", 11)
	cap.add_theme_color_override("font_color", Color(0.7, 0.78, 0.7))
	col.add_child(cap)

	# Degrade-state banner: shown in State 1 (no flow_matrix method) / State 2 (present but all-zero); hidden in
	# State 3 (live non-zero). The DATA picks the state (see _refresh_relations) — never a version flag.
	_relations_banner = Label.new()
	_relations_banner.add_theme_font_size_override("font_size", 11)
	_relations_banner.add_theme_color_override("font_color", Color(0.98, 0.8, 0.4))
	_relations_banner.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_relations_banner.custom_minimum_size = Vector2(280, 0)
	_relations_banner.visible = false
	col.add_child(_relations_banner)

	# Plain-language "who's eating whom" summary (Item a): the top few NONZERO inter-species flows parsed from the
	# MEASURED FlowMatrix. GDScript only sorts already-finished integers by magnitude + names them (no biology, inv #2).
	_relations_flowsum = Label.new()
	_relations_flowsum.add_theme_font_size_override("font_size", 11)
	_relations_flowsum.add_theme_color_override("font_color", Color(0.82, 0.93, 0.84))
	_relations_flowsum.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_relations_flowsum.custom_minimum_size = Vector2(280, 0)
	_relations_flowsum.visible = false
	col.add_child(_relations_flowsum)

	# Default representation: GRAPH (the user's expectation). The toggle reflects + swaps it.
	_relations_graph_btn.set_pressed_no_signal(true)
	_apply_relations_rep()

	# Diverging sign/magnitude legend strip (reuses the _legend_label colored-text pattern, diverging variant).
	var legend := HBoxContainer.new()
	legend.add_theme_constant_override("separation", 8)
	col.add_child(legend)
	legend.add_child(_diverging_swatch(Color(0.90, 0.32, 0.30), "− j drains i"))
	legend.add_child(_diverging_swatch(Color(0.13, 0.14, 0.15), "0"))
	legend.add_child(_diverging_swatch(Color(0.30, 0.86, 0.42), "j feeds i"))
	var units := Label.new()
	units.text = "(net J / generation)"
	units.add_theme_font_size_override("font_size", 10)
	units.add_theme_color_override("font_color", Color(0.6, 0.66, 0.6))
	col.add_child(units)

	# ── ADR-014 NEAREST-SPECIES strip (VIEW-ONLY / advisory) ────────────────────────────────────────────
	# A caption + a top-k nearest list per the off-hash metabolic/interaction SIGNATURE similarity. A PROVENANCE
	# BADGE distinct from the heatmap's "MEASURED" framing keeps a viewer from conflating the off-hash similarity
	# overlay with the on-hash FlowMatrix. GDScript only renders finished ordered integers (no biology/index math).
	var caption := Label.new()
	caption.text = "nearest species (metabolic / interaction similarity)"
	caption.add_theme_font_size_override("font_size", 11)
	caption.add_theme_color_override("font_color", Color(0.7, 0.78, 0.9))
	col.add_child(caption)

	var badge := Label.new()
	badge.text = "◆ ADVISORY · metabolic similarity, not measured flows · off-hash signature, view-only"
	badge.add_theme_font_size_override("font_size", 9)
	badge.add_theme_color_override("font_color", Color(0.62, 0.7, 0.86))
	badge.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	badge.custom_minimum_size = Vector2(340, 0)
	col.add_child(badge)

	_relations_nearest = Label.new()
	_relations_nearest.add_theme_font_size_override("font_size", 10)
	_relations_nearest.add_theme_color_override("font_color", Color(0.84, 0.88, 0.94))
	_relations_nearest.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_relations_nearest.custom_minimum_size = Vector2(340, 0)
	_relations_nearest.visible = false
	col.add_child(_relations_nearest)

	_relations_panel = PanelChrome.new()
	_relations_panel.setup("🔗 RELATIONS", body, ui, Vector2(maxf(220.0, field_px.x - 320.0), 176.0), _pill_rail)  # y=176 clears the always-on VIEW+SCOPE panel
	_relations_panel.set_active(false)


# ──────────────────────────── browsable CODEX view (SP-4) ────────────────────────────

## Build the CODEX view chrome: a FULL-WINDOW panel (modelled on _relations_full + the specimen full-rect) holding a
## left entry list + filter and a right scrollable detail pane. Renderer-only (inv #2): every field rendered is a
## STATIC codex.gd lookup keyed on a core-exported id — no genotype→phenotype, no biology. The list is populated in
## _refresh_codex (on view-entry) and degrades to a clear "codex unavailable" note when codex.gd did not load.
func _build_codex_ui(ui: CanvasLayer, _field_px: Vector2) -> void:
	# Full-window container, same offsets the Relations full view uses (under the title bar, above the controls deck).
	_codex_full = Control.new()
	_codex_full.set_anchors_preset(Control.PRESET_FULL_RECT)
	_codex_full.offset_top = 44.0       # clear the full-width title bar
	_codex_full.offset_bottom = -58.0   # clear the bottom controls/timeline band
	_codex_full.offset_left = 8.0
	_codex_full.offset_right = -8.0
	_codex_full.mouse_filter = Control.MOUSE_FILTER_IGNORE  # the interactive widgets below own their own input
	_codex_full.visible = false
	# Same `ui` CanvasLayer as the other views; added before the viewscope (see _build_scene) so the VIEW switcher
	# draws on top and never gets covered by the codex card.
	ui.add_child(_codex_full)

	# Dark card backdrop filling the container (reuses the panel idiom of the other full views).
	var card := PanelContainer.new()
	card.set_anchors_preset(Control.PRESET_FULL_RECT)
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.04, 0.06, 0.05, 0.86)
	sb.set_corner_radius_all(8)
	sb.set_border_width_all(1)
	sb.border_color = Color(0.2, 0.46, 0.32, 0.6)
	sb.set_content_margin_all(12)
	card.add_theme_stylebox_override("panel", sb)
	_codex_full.add_child(card)

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 14)
	card.add_child(row)

	# ── Left column: title + filter + the browsable entry list ──
	var left := VBoxContainer.new()
	left.add_theme_constant_override("separation", 6)
	left.custom_minimum_size = Vector2(300, 0)
	left.size_flags_vertical = Control.SIZE_FILL
	row.add_child(left)

	var title := Label.new()
	title.text = "📖 CODEX — browsable encyclopedia"
	title.add_theme_font_size_override("font_size", 14)
	title.add_theme_color_override("font_color", Color(0.86, 0.92, 0.84))
	left.add_child(title)

	_codex_filter = LineEdit.new()
	_codex_filter.placeholder_text = "filter (species / gene / role)…"
	_codex_filter.clear_button_enabled = true
	_codex_filter.text_changed.connect(_on_codex_filter_changed)
	left.add_child(_codex_filter)

	_codex_list = ItemList.new()
	_codex_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_codex_list.size_flags_horizontal = Control.SIZE_FILL
	_codex_list.auto_height = false
	_codex_list.item_selected.connect(_on_codex_item_selected)
	left.add_child(_codex_list)

	# ── Right column: a scrollable detail pane (vertical scroll only → labels autowrap to the pane width) ──
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_child(scroll)

	_codex_detail_box = VBoxContainer.new()
	_codex_detail_box.add_theme_constant_override("separation", 5)
	_codex_detail_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_codex_detail_box)


## (Re)build the browsable codex list from codex.gd, honouring the current filter. Selects + renders the first match.
## Degrades to a "codex unavailable" note when codex.gd did not load (older build / unstaged mirror) — guarded like
## the other views. Renderer-only (inv #2): reads static codex entries; computes no biology.
func _refresh_codex() -> void:
	if _codex_list == null:
		return
	_rebuild_codex_list(_codex_filter.text if _codex_filter != null else "")


func _on_codex_filter_changed(text: String) -> void:
	_rebuild_codex_list(text)


func _on_codex_item_selected(idx: int) -> void:
	if idx >= 0 and idx < _codex_entries.size():
		_render_codex_detail(_codex_entries[idx])


## Rebuild the left list from the ORDERED codex arrays (species → roles → genes → flows, inv #3), filtered by a
## case-insensitive substring over each row's label/id. Each accepted row appends a {kind,id} to _codex_entries so a
## selection maps straight back to a codex.gd lookup. Empty codex → the unavailable note; no matches → a "no match" note.
func _rebuild_codex_list(filter: String) -> void:
	_codex_list.clear()
	_codex_entries = []
	if not _codex.is_loaded():
		_show_codex_message("Codex unavailable",
			"res://data/codex/codex.json did not load (older build, or the data/codex → godot/data/codex mirror was not staged). Run ./run.sh or tools/check_godot_snapshot.sh to stage it, then reopen the Codex view.")
		return
	var f := filter.strip_edges().to_lower()
	for key in _codex.species_keys():
		var sp := _codex.species_for(key)
		var label := "%s  %s" % [str(sp.get("emoji", "🔬")), str(sp.get("display_name", key))]
		_maybe_add_codex_entry("species", str(key), label, "%s %s %s" % [label, key, str(sp.get("headline", ""))], f)
	for rid in _codex.role_ids():
		var r := _codex.role_for(rid)
		var label := "◆ %s" % str(r.get("title", rid))
		_maybe_add_codex_entry("role", str(rid), label, "%s %s %s" % [label, rid, str(r.get("one_line", ""))], f)
	for sym in _codex.gene_symbols():
		var g := _codex.gene_for_symbol(sym)
		var label := "🧬 %s — %s" % [str(sym), str(g.get("one_line", g.get("go_label", "")))]
		_maybe_add_codex_entry("gene", str(sym), label, "%s %s %s" % [label, sym, str(g.get("go_label", ""))], f)
	for fk in _codex.flow_keys():
		var parts := str(fk).split("|")
		var fl: Dictionary = _codex.flow_for(parts[0], parts[1]) if parts.size() == 2 else {}
		var label := "→ %s" % str(fl.get("title", fk))
		_maybe_add_codex_entry("flow", str(fk), label, "%s %s" % [label, fk], f)
	if _codex_entries.is_empty():
		_show_codex_message("No matches", "No codex entries match \"%s\"." % filter.strip_edges())
		return
	_codex_list.select(0)
	_render_codex_detail(_codex_entries[0])


## Append one list row + its {kind,id} when it passes the (already-lowercased) substring filter. `searchable` is the
## haystack (label + id + a gloss); an empty filter accepts everything. Keeps the list and _codex_entries in lockstep.
func _maybe_add_codex_entry(kind: String, id: String, label: String, searchable: String, f: String) -> void:
	if f != "" and not searchable.to_lower().contains(f):
		return
	_codex_list.add_item(label)
	_codex_entries.append({"kind": kind, "id": id})


## Render the detail pane for a selected {kind,id} entry. Pure presentation (inv #2): each branch is a static
## codex.gd lookup formatted into section labels; no biology is derived.
func _render_codex_detail(entry: Dictionary) -> void:
	for c in _codex_detail_box.get_children():
		c.queue_free()
	match str(entry.get("kind", "")):
		"species": _render_codex_species(str(entry.get("id", "")))
		"role": _render_codex_role(str(entry.get("id", "")))
		"gene": _render_codex_gene(str(entry.get("id", "")))
		"flow": _render_codex_flow(str(entry.get("id", "")))


func _render_codex_species(key: String) -> void:
	var sp := _codex.species_for(key)
	if sp.is_empty():
		_codex_detail_box.add_child(_detail_label("(no codex entry for %s)" % key, 13, Color(0.85, 0.74, 0.6)))
		return
	_codex_detail_box.add_child(_detail_label(
		"%s  %s" % [str(sp.get("emoji", "")), str(sp.get("display_name", key))], 18, Color(0.97, 0.99, 0.96)))
	if sp.has("headline"):
		_codex_detail_box.add_child(_detail_label(str(sp["headline"]), 13, Color(0.86, 0.92, 0.88)))
	_codex_section("Taxonomy", str(sp.get("taxonomy", "")))
	_codex_section("Ontology", str(sp.get("ontology", "")))
	_codex_section("Phenology", str(sp.get("phenology", "")))
	_codex_section("Famous fact", str(sp.get("famous_fact", "")))
	_codex_section("Sim role", str(sp.get("sim_role", "")))
	var anchors: Array = sp.get("anchor_genes", [])
	if not anchors.is_empty():
		_codex_section("Anchor genes", "  ·  ".join(PackedStringArray(anchors)))
	_codex_sources(sp.get("sources", []))


func _render_codex_gene(sym: String) -> void:
	var g := _codex.gene_for_symbol(sym)
	if g.is_empty():
		_codex_detail_box.add_child(_detail_label("(no codex entry for %s)" % sym, 13, Color(0.85, 0.74, 0.6)))
		return
	_codex_detail_box.add_child(_detail_label(
		"🧬 %s — %s" % [sym, str(g.get("go_label", ""))], 18, Color(0.95, 0.92, 0.7)))
	if g.has("one_line"):
		_codex_detail_box.add_child(_detail_label(str(g["one_line"]), 13, Color(0.86, 0.92, 0.88)))
	_codex_section("Identifiers", "GO:%s · SO:%s · trait: %s · species: %s" % [
		str(g.get("go", "—")), str(g.get("so", "—")), str(g.get("trait", "—")), str(g.get("species_key", "—"))])
	_codex_section("Ontology", str(g.get("ontology", "")))
	_codex_section("Famous fact", str(g.get("famous_fact", "")))
	_codex_section("Knockdown", str(g.get("knockdown", "")))
	_codex_sources(g.get("sources", []))


func _render_codex_role(rid: String) -> void:
	var r := _codex.role_for(rid)
	if r.is_empty():
		_codex_detail_box.add_child(_detail_label("(no codex entry for %s)" % rid, 13, Color(0.85, 0.74, 0.6)))
		return
	_codex_detail_box.add_child(_detail_label("◆ %s" % str(r.get("title", rid)), 18, Color(0.84, 0.92, 0.96)))
	if r.has("one_line"):
		_codex_detail_box.add_child(_detail_label(str(r["one_line"]), 13, Color(0.86, 0.92, 0.88)))
	_codex_section("Description", str(r.get("body", "")))
	_codex_section("Trophic position", "typically prey" if bool(r.get("is_prey", false)) else "not typically prey")
	_codex_sources(r.get("sources", []))


func _render_codex_flow(fk: String) -> void:
	var parts := fk.split("|")
	var fl: Dictionary = _codex.flow_for(parts[0], parts[1]) if parts.size() == 2 else {}
	if fl.is_empty():
		_codex_detail_box.add_child(_detail_label("(no codex entry for %s)" % fk, 13, Color(0.85, 0.74, 0.6)))
		return
	_codex_detail_box.add_child(_detail_label("→ %s" % str(fl.get("title", fk)), 18, Color(0.9, 0.88, 0.7)))
	_codex_detail_box.add_child(_detail_label(
		"%s → %s" % [str(fl.get("from_role", "")), str(fl.get("to_role", ""))], 12, Color(0.78, 0.84, 0.78)))
	_codex_section("Phenology", str(fl.get("phenology", "")))
	_codex_section("Description", str(fl.get("body", "")))
	_codex_sources(fl.get("sources", []))


## A titled section (dim header + wrapped body) appended to the detail pane; skipped when the body is empty (graceful).
func _codex_section(title: String, body: String) -> void:
	if body.strip_edges() == "":
		return
	_codex_detail_box.add_child(_detail_label(title, 12, Color(0.66, 0.78, 0.7)))
	_codex_detail_box.add_child(_detail_label(body, 12, Color(0.88, 0.92, 0.88)))


## The "Sources" tail: a dim header + one bullet per URL (graceful when none).
func _codex_sources(sources: Array) -> void:
	if sources.is_empty():
		return
	_codex_detail_box.add_child(_detail_label("Sources", 11, Color(0.6, 0.7, 0.66)))
	for s in sources:
		_codex_detail_box.add_child(_detail_label("• %s" % str(s), 10, Color(0.56, 0.66, 0.78)))


## Replace the detail pane with a single titled message (the unavailable / no-match degrade states).
func _show_codex_message(title: String, body: String) -> void:
	for c in _codex_detail_box.get_children():
		c.queue_free()
	_codex_detail_box.add_child(_detail_label(title, 16, Color(0.95, 0.8, 0.5)))
	_codex_detail_box.add_child(_detail_label(body, 12, Color(0.82, 0.86, 0.8)))


## Item 4: select the relations representation (Graph vs Matrix) + swap which node is visible. The legend / flow
## summary / nearest strips stay shown for BOTH (they annotate the same MEASURED data). Pure renderer state (inv #2).
func _on_relations_rep_selected(show_graph: bool) -> void:
	_relations_show_graph = show_graph
	_apply_relations_rep()


func _apply_relations_rep() -> void:
	if _relations_graph != null:
		_relations_graph.visible = _relations_show_graph
	if _relations_heatmap != null:
		_relations_heatmap.visible = not _relations_show_graph


## Item 4: per-species display arrays in SpeciesId order (= observe_species() order = FlowMatrix index order, by
## construction): {names, keys, roles, pops}. Feeds the relations GRAPH nodes. Pure reads of core exports (inv #2);
## empty (→ a node-less graph) on file-replay / an older cdylib without observe_species().
func _relations_species_arrays() -> Dictionary:
	var names := PackedStringArray()
	var keys := PackedStringArray()
	var roles := PackedStringArray()
	var pops := PackedInt64Array()
	if _live != null and _live.has_method("observe_species"):
		for sp in _live.observe_species():
			var d: Dictionary = sp
			names.append(str(d.get("name", "species")))
			keys.append(str(d.get("key", "default")))
			roles.append(str(d.get("role", "")))
			pops.append(int(d.get("population_size", 0)))
	# File-replay / older cdylib (no observe_species): mirror the heatmap's _species_names() placeholder so the two
	# views agree — the graph then draws a neutral node captioned like the matrix row, not a bare "sp0". Cosmetic
	# parity only (inv #2 — still a pure projection; populations are unknown in replay → 0).
	if names.is_empty():
		for nm in _species_names():
			names.append(nm)
			keys.append("default")
			roles.append("")
			pops.append(0)
	return {"names": names, "keys": keys, "roles": roles, "pops": pops}


## A small color swatch + label for the diverging legend strip (presentation only).
func _diverging_swatch(col: Color, text: String) -> HBoxContainer:
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 4)
	var chip := ColorRect.new()
	chip.color = col
	chip.custom_minimum_size = Vector2(14, 14)
	box.add_child(chip)
	var lbl := Label.new()
	lbl.text = text
	lbl.add_theme_font_size_override("font_size", 10)
	lbl.add_theme_color_override("font_color", Color(0.86, 0.9, 0.86))
	box.add_child(lbl)
	return box


## Tolerant read of the core FlowMatrix export. Returns {} when the LiveSim cdylib has no flow_matrix() method
## (today's cdylib + file-replay where _live == null) — State 1. Otherwise returns {s:int, j:PackedInt64Array}.
## Same forward/back-compat has_method probe used for observe_species/species_key (inv #2: pure read of an export).
func _flow_matrix_or_empty() -> Dictionary:
	if _live != null and _live.has_method("flow_matrix"):
		return _live.flow_matrix() as Dictionary
	return {}


## Tolerant read of the ADR-014 relations overlay export (guild_of + nearest). Returns {} when the LiveSim cdylib
## has no species_relations() method (old cdylib / file-replay where _live == null) — the 4th degrade state. Same
## forward-compat has_method probe used for flow_matrix/observe_species (inv #2: a pure read of an OFF-HASH export;
## the k-NN/clustering ran in the std-only relations-index boundary crate, never in GDScript).
func _species_relations_or_empty() -> Dictionary:
	if _live != null and _live.has_method("species_relations"):
		return _live.species_relations() as Dictionary
	return {}


## Pull species names (observe_species() order = SpeciesId order = FlowMatrix index order, by construction) and the
## flat FlowMatrix, then feed the heatmap. THREE honest degrade states picked purely from the DATA:
##   State 1 — no flow_matrix method → {} → render an S×S all-zero grid sized from the species roster + banner.
##   State 2 — method present, matrix all-zero (F4.0 scaffold) → real cells, neutral, "loop not yet closed" banner.
##   State 3 — method present, non-zero (F4.1 live) → diverging ramp saturates; banner hidden.
## ZERO biology here (inv #2): the renderer only sizes the grid, forwards integers, and selects banner text.
func _refresh_relations() -> void:
	if _relations_heatmap == null:
		return
	var names := _species_names()
	_relations_heatmap.setup(names)
	var fm := _flow_matrix_or_empty()
	var method_present: bool = (_live != null and _live.has_method("flow_matrix"))
	var s := int(fm.get("s", 0))
	var flat: PackedInt64Array = fm.get("j", PackedInt64Array())
	# When the matrix is absent/degenerate, size an all-zero grid from the species roster so the grid + real labels
	# still render (State 1). The heatmap tolerates a zero/short array as a valid degenerate input.
	if s <= 0 or flat.size() != s * s:
		s = names.size()
		flat = PackedInt64Array()
		flat.resize(s * s)  # zero-filled
	_relations_heatmap.set_matrix(flat, s)
	# Item 4: feed the node-link GRAPH the SAME MEASURED FlowMatrix + per-species metadata (names/keys/roles/pops in
	# SpeciesId order = the matrix index order). The graph degrades to nodes-only on a degenerate matrix, exactly like
	# the heatmap. Pure projection of core exports (inv #2) — no biology, no RNG.
	if _relations_graph != null and _relations_graph.has_method("set_data"):
		var meta := _relations_species_arrays()
		_relations_graph.set_data(meta["names"], meta["keys"], meta["roles"], meta["pops"], flat, s)
	# Item (a): narrate the top NONZERO inter-species flows in plain language above the heatmap. Same MEASURED
	# integers the heatmap paints — just sorted by magnitude + named. Hidden when the matrix is all-zero (State 1/2).
	if _relations_flowsum != null:
		var summary := _format_flow_summary(names, flat, s)
		if summary != "":
			_relations_flowsum.text = summary
			_relations_flowsum.visible = true
		else:
			_relations_flowsum.visible = false
	# Banner: distinguish "no export" (State 1) from "export wired, physics off" (State 2) from "live" (State 3).
	if _relations_banner != null:
		if not method_present:
			_relations_banner.text = "Relations not yet coupled — build/run a cdylib with the F4 FlowMatrix (flow_matrix())"
			_relations_banner.visible = true
		elif _all_zero(flat):
			_relations_banner.text = "FlowMatrix present, loop not yet closed (F4.1) — all flows zero"
			_relations_banner.visible = true
		else:
			_relations_banner.visible = false

	# ── ADR-014 OVERLAY (additive, graceful-degrading 4th state) ──────────────────────────────────────────
	# Feed the guild tints + the nearest-species strip from the OFF-HASH species_relations() export. When the
	# method is absent (old cdylib) or the index is empty, the tints + strip simply don't appear; the MEASURED
	# FlowMatrix heatmap above renders EXACTLY as today and is never blocked on the index.
	var rel := _species_relations_or_empty()
	var rel_s := int(rel.get("s", 0))
	var guilds: PackedInt32Array = rel.get("guild_of", PackedInt32Array())
	if _relations_heatmap.has_method("set_guilds"):
		# Only overlay when the guild vector lines up with the rendered roster; else clear (neutral labels).
		if rel_s == s and guilds.size() == s and s > 0:
			_relations_heatmap.set_guilds(guilds)
		else:
			_relations_heatmap.set_guilds(PackedInt32Array())
	# Nearest-species strip: list each focal species' top-k nearest names + a distance pip.
	if _relations_nearest != null:
		var nearest: Dictionary = rel.get("nearest", {})
		if rel_s == s and s > 0 and nearest.size() > 0:
			_relations_nearest.text = _format_nearest(names, nearest)
			_relations_nearest.visible = true
		else:
			_relations_nearest.visible = false


## Format the nearest-species map into an advisory strip: "plant → ecoli(d12.3k), …" per focal species. GDScript
## only forwards finished ordered integers from the boundary index → names + a distance pip; NO biology/index math.
func _format_nearest(names: PackedStringArray, nearest: Dictionary) -> String:
	var lines: PackedStringArray = PackedStringArray()
	for focal in nearest.keys():
		var fi := int(focal)
		var fname := names[fi] if fi >= 0 and fi < names.size() else "sp%d" % fi
		var pairs: PackedInt32Array = nearest[focal]
		var parts: PackedStringArray = PackedStringArray()
		var p := 0
		while p + 1 < pairs.size():
			var sid := int(pairs[p])
			var dist := int(pairs[p + 1])
			var nm := names[sid] if sid >= 0 and sid < names.size() else "sp%d" % sid
			parts.append("%s·d%d" % [nm, dist])
			p += 2
		if parts.size() > 0:
			lines.append("%s → %s" % [fname, ", ".join(parts)])
	return "\n".join(lines)


## Item (a): narrate the top NONZERO inter-species flows from the MEASURED FlowMatrix as a plain-language line,
## e.g. "Primary flows: plant → ecoli (+450 J/gen), ecoli → bdellovibrio (−200 J/gen)". The matrix contract:
## flat[i*s+j] = NET joules that flowed FROM source j INTO sink i. Each physical edge appears twice (the +v at
## (i,j) and its −v mirror at (j,i)); we keep ONE per unordered {i,j} pair (the entry with the larger species
## index as sink, deterministically) so the same flow isn't listed both ways. We render it SOURCE → SINK with the
## net magnitude, signed by whether the higher-indexed member gains (+) or loses (−) — a stable orientation a
## viewer can read. Returns "" when there are no nonzero off-diagonal flows (State 1/2). Pure DISPLAY arithmetic
## (sort finished integers by |magnitude|, name them) — no biology, no index remap (inv #2/#3).
func _format_flow_summary(names: PackedStringArray, flat: PackedInt64Array, s: int) -> String:
	if s <= 1 or flat.size() != s * s:
		return ""
	# Collect one entry per unordered pair {a,b} with a<b: read the OFF-diagonal cell at (b,a) — i.e. net into the
	# higher-indexed species b from the lower-indexed a. v>0 ⇒ a feeds b (source a → sink b); v<0 ⇒ b feeds a.
	var edges: Array = []  # [{src:int, dst:int, v:int}] — v already oriented as net into dst
	for a in s:
		for b in range(a + 1, s):
			var v: int = int(flat[b * s + a])  # net J from a into b (the canonical pair entry)
			if v == 0:
				continue
			if v > 0:
				edges.append({"src": a, "dst": b, "v": v})  # a → b, a gives
			else:
				edges.append({"src": b, "dst": a, "v": -v})  # b → a, b gives (flip so the arrow points to the gainer)
	if edges.is_empty():
		return ""
	edges.sort_custom(func(x, y): return int(x["v"]) > int(y["v"]))  # largest magnitude first (stable display order)
	var parts: PackedStringArray = PackedStringArray()
	var k: int = mini(5, edges.size())  # top 3–5; cap at 5
	for i in k:
		var e: Dictionary = edges[i]
		var sn := names[int(e["src"])] if int(e["src"]) < names.size() else "sp%d" % int(e["src"])
		var dn := names[int(e["dst"])] if int(e["dst"]) < names.size() else "sp%d" % int(e["dst"])
		parts.append("%s → %s (+%d J/gen)" % [sn, dn, int(e["v"])])
	return "Primary flows: " + ", ".join(parts)


## True if every entry of `flat` is zero (or it is empty). Pure display-state check, not biology.
func _all_zero(flat: PackedInt64Array) -> bool:
	for v in flat:
		if v != 0:
			return false
	return true


## The species display names in SpeciesId order (the FlowMatrix / observe_species index order, by construction).
## Live → observe_species() names (degrades to the single observe() species when the per-species export is absent);
## file-replay → a single "species" label so the relations grid still renders 1×1 (State 1). Read-only (inv #2).
func _species_names() -> PackedStringArray:
	var out := PackedStringArray()
	if _live != null and _live.has_method("observe_species"):
		for sp in _live.observe_species():
			out.append(str((sp as Dictionary).get("name", "species")))
		if out.size() > 0:
			return out
	if _live != null:
		out.append(str(_live.observe().get("name", "species")))
		return out
	out.append("species")  # file-replay: a single placeholder row/col so the grid renders
	return out


## A per-species stat for the Vitals block (Rel-UI.1). The PRIMARY (active observe()) species reads RUN-LEVEL
## values today (population_size ← observe().population, allele_freq ← observe().allele_freq, mean_fitness ←
## _mean_pop over the snapshot) so single-species runs read correctly NOW. NON-primary species (and any field the
## core does not yet expose) read obs.get(key, null) → null until the Layer-B core widening lands. Returns null on
## any unexposed value (rendered "—"). Read-only (inv #2): pure projection of already-exported core scalars.
func _species_stat(obs: Dictionary, sid: int, key: String):
	if _is_primary_species(sid):
		match key:
			"population_size":
				if _live != null:
					return int(_live.observe().get("population", 0))
				if not _snaps.is_empty():
					return int((_snaps[_idx]).population)
			"allele_freq":
				if _live != null:
					return clampf(float(_live.observe().get("allele_freq", 0.0)), 0.0, 1.0)
				if not _snaps.is_empty():
					return _mean_pop((_snaps[_idx]).allele_freq, (_snaps[_idx]).density)
			"mean_fitness":
				if not _snaps.is_empty():
					return _mean_pop((_snaps[_idx]).fitness, (_snaps[_idx]).density)
	# Non-primary species, or a key the active observe() path does not cover: read the per-species export if the
	# core has widened SpeciesObservation (Layer B). Absent today → null → "—" + the pending-export note.
	if obs.has(key):
		return obs[key]
	return null


## Whether `sid` is the PRIMARY active species the run-level observe() reports on (R3-B: single-active, id 0 today).
func _is_primary_species(sid: int) -> bool:
	return sid == 0


## Format a stat value for the vitals row. null / NAN → "—" (the honest "not yet exported" marker). The trend
## glyph is prepended by the caller. Pure presentation (inv #2).
func _fmt_stat(v, fmt: String) -> String:
	if v == null:
		return "—"
	var f := float(v)
	if is_nan(f):
		return "—"
	if fmt == "%d":
		return "%d" % int(round(f))
	return fmt % f


## The SpeciesId-ordered per-species observation rows (live → observe_species(); else a single synthetic row for
## the primary species so the panel still reads). Each row: {species_id:int, name:String, key:String, obs:Dict}.
## Read-only (inv #2): observe_species() is a pure core export; this only reshapes it for the panel.
func _panel_species_list() -> Array:
	var out: Array = []
	if _live != null and _live.has_method("observe_species"):
		for sp in _live.observe_species():
			var d: Dictionary = sp
			out.append({
				"species_id": int(d.get("species_id", 0)),
				"name": str(d.get("name", "species")),
				"key": str(d.get("key", "default")),
				"obs": d,
			})
		out.sort_custom(func(a, b): return int(a["species_id"]) < int(b["species_id"]))  # SpeciesId order (inv #3)
		if not out.is_empty():
			return out
	# Fallback: a single primary-species row (the active observe() species, or a file-replay placeholder).
	var key := "default"
	var nm := "species"
	if _live != null:
		if _live.has_method("species_key") and String(_live.species_key()) != "":
			key = String(_live.species_key())
		nm = str(_live.observe().get("name", nm))
	out.append({"species_id": 0, "name": nm, "key": key, "obs": {}})
	return out


## Refill the picker from the current specimen list (baseline first). Clamps _focus into range.
func _populate_specimen_picker() -> void:
	if _specimen_picker == null:
		return
	_specimen_picker.clear()
	var list := _specimen_list()
	for spec in list:
		_specimen_picker.add_item(str((spec as Dictionary).get("label", "specimen")))
	_focus = clampi(_focus, 0, maxi(0, list.size() - 1))
	if list.size() > 0:
		_specimen_picker.select(_focus)


func _on_specimen_selected(idx: int) -> void:
	# Issue 2: focus-CHANGE updates only the highlight + the detail/inspect readout — it does NOT re-frame the
	# camera (the old _frame_focused_specimen() zoom-on-focus is gone), so the current zoom/pan is preserved while
	# the user switches the active species for labels/details.
	_focus = idx
	_update_trait_readout()
	_emphasise_focus()


## Variant Lab — Slice B: snapshot the FOCUSED species' current (post-edit) genome into the renderer-side
## _saved_variants store. Exports the opaque SpeciesSpec JSON via LiveSim.export_species_json (biology→spec lives in
## the core, inv #2); GDScript only carries the inert text + a display name + the species_id/role/traits for the
## list glyph. A blank/duplicate name gets a sensible default + a numeric suffix. Refreshes the CONTAMINATION
## panel's "Saved variants" section so the new variant is immediately reseedable (Slice C).
func _on_save_variant_pressed() -> void:
	if _live == null or not _live.has_method("export_species_json"):
		_flash_status("✗ Save variant unsupported by this build", false)
		return
	var row := _focused_species_row()
	var sid := int(row.get("species_id", 0))
	var json := str(_live.export_species_json(sid))  # opaque SpeciesSpec JSON; never parsed in GDScript (inv #2)
	if json == "":
		_flash_status("✗ could not export species %d" % sid, false)
		return
	var raw := _save_variant_name.text.strip_edges() if _save_variant_name != null else ""
	var vname := _unique_variant_name(raw, str(row.get("name", "species")))
	_saved_variants.append({
		"name": vname,
		"json": json,
		"key": str(row.get("key", _focused_key())),  # the core key the reseed resolves against
		"species_id": sid,
		"role": _focused_role(),
		"traits": _focused_traits(),
	})
	if _save_variant_name != null:
		_save_variant_name.text = ""
	_refresh_saved_variants_section()
	_flash_status("💾 saved variant '%s'" % vname, true)


## A snapshot of the focused specimen's trait dict (for the saved-variant list glyph/role only — presentation, inv #2).
func _focused_traits() -> Dictionary:
	var list := _specimen_list()
	if list.is_empty():
		return {}
	return ((list[clampi(_focus, 0, list.size() - 1)] as Dictionary).get("traits", {}) as Dictionary).duplicate()


## A non-empty, unique saved-variant name: blank → "<species> variant"; a collision gets a " #N" suffix (inv #2).
func _unique_variant_name(raw: String, fallback: String) -> String:
	var base := raw if raw != "" else ("%s variant" % fallback)
	var name := base
	var n := 2
	while _variant_name_taken(name):
		name = "%s #%d" % [base, n]
		n += 1
	return name


func _variant_name_taken(name: String) -> bool:
	for v in _saved_variants:
		if str((v as Dictionary).get("name", "")) == name:
			return true
	return false


## Whether the ACTIVE species is the microbe (E. coli) rather than the abstract plant. Routes on the menu stem
## (already in hand, zero round-trip); falls back to the CORE's authoritative species_key() as a tiebreak if
## stem/key ever diverge. Read-only (inv #2): species_key is a pure read of already-loaded core data, no biology.
func _is_microbe() -> bool:
	if _species_stem == "ecoli":
		return true
	if _live != null and _live.has_method("species_key"):
		return String(_live.species_key()) == "ecoli-core"
	return false


## Whether a SPECIMEN ROW's species key is the microbe (E. coli) — drives the per-row glyph + readout dispatch
## in a mixed run, independent of the globally-active species. The authoritative tiebreak is the core's key.
func _is_microbe_key(key: String) -> bool:
	return key == "ecoli-core"


## The `key` of the currently focused specimen row (drives the readout's trait set + chrome glyph). Falls back
## to the globally-active species when the row has no key (legacy/empty list).
func _focused_key() -> String:
	var list := _specimen_list()
	if not list.is_empty():
		var spec: Dictionary = list[clampi(_focus, 0, list.size() - 1)]
		if spec.has("key"):
			return str(spec["key"])
	return "ecoli-core" if _is_microbe() else "default"


## The trophic role (Debug-cased) of the focused specimen row, for the morphotype + codex join. "" if unknown.
func _focused_role() -> String:
	var list := _specimen_list()
	if not list.is_empty():
		var spec: Dictionary = list[clampi(_focus, 0, list.size() - 1)]
		return str(spec.get("role", ""))
	return ""


## The trait-key list for the FOCUSED specimen's species, picked by MORPHOTYPE so the readout shows exactly that
## species' diagnostic phenotype set (and the previously-DROPPED predation/sporulation/symbiosis rows now render).
func _active_trait_keys() -> Array:
	match GlyphFactory.morph_for(_focused_key(), _focused_role()):
		GlyphFactory.ROD:
			# Spore-forming rod (Bacillus) → the sporulation row; other rods (E. coli/cutibacterium/pseudomonas) →
			# the 5 microbe phenotypes. Detect a spore-former by the focused row carrying a sporulation_capacity > 0.
			return SPORE_TRAIT_KEYS if _focused_has_trait("sporulation_capacity") else MICROBE_TRAIT_KEYS
		GlyphFactory.VIBRIOID:
			return PREDATOR_TRAIT_KEYS
		GlyphFactory.MOLD:
			return SPORE_TRAIT_KEYS
		GlyphFactory.SYMBIONT:
			return SYMBIONT_TRAIT_KEYS
		GlyphFactory.COCCI, GlyphFactory.PLEOMORPH:
			return MICROBE_TRAIT_KEYS
		_:
			return TRAIT_KEYS


## Whether the focused specimen row carries a non-zero value for `trait_key` (used to tell a spore-forming rod
## like Bacillus from a plain rod like E. coli without hard-coding keys).
func _focused_has_trait(trait_key: String) -> bool:
	var list := _specimen_list()
	if list.is_empty():
		return false
	var t: Dictionary = (list[clampi(_focus, 0, list.size() - 1)] as Dictionary).get("traits", {})
	return float(t.get(trait_key, 0.0)) > 0.0


## Rewrite the trait bars/values/deltas for the focused specimen (vs baseline = list index 0). The row COUNT is
## fixed at build (max of the plant/microbe sets); rows beyond the active species' key list are hidden so the
## panel reads as exactly the species' phenotype (5 for the microbe, 9 for the plant).
## The panel-species row (species_id + obs) for the currently focused specimen. Matches the focused specimen's
## name+key back to _panel_species_list() (SpeciesId-ordered); falls back to the primary row. Read-only (inv #2).
func _focused_species_row() -> Dictionary:
	var rows := _panel_species_list()
	if rows.is_empty():
		return {"species_id": 0, "name": "species", "key": "default", "obs": {}}
	var list := _specimen_list()
	if not list.is_empty():
		var spec: Dictionary = list[clampi(_focus, 0, list.size() - 1)]
		var fkey := str(spec.get("key", ""))
		var fname := str(spec.get("label", "")).split(" — ")[0]  # the picker label is "Name — baseline/edit N"
		for r in rows:
			if str(r["key"]) == fkey and str(r["name"]) == fname:
				return r
		for r in rows:  # name may not match (legacy labels); fall back to a key match
			if str(r["key"]) == fkey:
				return r
	return rows[0]


## Fill the 3-up Population / Allele / Fitness vitals block for the FOCUSED species (Rel-UI.1). PRIMARY species
## read run-level values today; non-primary (and any unexposed field) render "—" + a one-line pending-export note.
## Trend ▲▼ is vs the previous tick's value for that species+key. Read-only (inv #2): pure projection of exports.
func _update_species_vitals() -> void:
	if _species_vital_rows.is_empty():
		return
	var row := _focused_species_row()
	var sid := int(row["species_id"])
	var obs: Dictionary = row.get("obs", {})
	var any_pending := false
	for vr in _species_vital_rows:
		var vrow: Dictionary = vr
		var key := str(vrow["key"])
		var v = _species_stat(obs, sid, key)
		var lbl: Label = vrow["value"]
		var trend_key := "%d:%s" % [sid, key]
		if v == null:
			lbl.text = "—"
			any_pending = true
		else:
			var f := float(v)
			lbl.text = "%s  %s" % [_species_stat_trend(f, trend_key), _fmt_stat(v, str(vrow["fmt"]))]
			_prev_species_stats[trend_key] = f
	if _species_vital_note != null:
		_species_vital_note.text = "per-species stat pending core export"
		_species_vital_note.visible = any_pending


## ▲ / ▼ / = trend of `now` vs the previous tick's value for `key` (per-species variant of _trend; no RNG).
func _species_stat_trend(now: float, key: String) -> String:
	if not _prev_species_stats.has(key):
		return "·"
	var prev := float(_prev_species_stats[key])
	if absf(now - prev) <= maxf(0.0005, absf(prev) * 0.001):
		return "="
	return "▲" if now > prev else "▼"


func _update_trait_readout() -> void:
	if _trait_rows.is_empty():
		return
	var list := _specimen_list()
	if list.is_empty():
		return
	# Chrome glyph follows the FOCUSED specimen's MORPHOTYPE (🦠 rod/cocci/vibrioid · 🍄 mold · 🫧 mycoplasma ·
	# 🔬 symbiont · 🌱 plant) for instant identity — via the same key-led table the glyph factory uses.
	if _specimen_panel != null and _specimen_panel.has_method("set_title"):
		_specimen_panel.set_title("%s SPECIMEN" % GlyphFactory.emoji_for(_focused_key(), _focused_role()))
	_update_species_vitals()  # the 3-up Population/Allele/Fitness block for the focused species (Rel-UI.1)
	var keys := _active_trait_keys()
	var focused: Dictionary = (list[clampi(_focus, 0, list.size() - 1)] as Dictionary).get("traits", {})
	var base: Dictionary = (list[0] as Dictionary).get("traits", {})
	for i in _trait_rows.size():
		var row: Dictionary = _trait_rows[i]
		var name_lbl: Label = row["name"]
		if i >= keys.size():
			# No trait for this row under the active species → hide the whole row (the box collapses).
			name_lbl.get_parent().visible = false
			continue
		name_lbl.get_parent().visible = true
		var key: String = keys[i]
		name_lbl.text = key
		var v := clampf(float(focused.get(key, 0.0)), 0.0, 1.0)
		var b := clampf(float(base.get(key, 0.0)), 0.0, 1.0)
		(row["bar"] as ProgressBar).value = v
		(row["value"] as Label).text = "%.3f" % v
		var d := v - b
		var delta: Label = row["delta"]
		if absf(d) < 0.0005:
			delta.text = "="
			delta.add_theme_color_override("font_color", Color(0.6, 0.62, 0.6))
		elif d > 0.0:
			delta.text = "▲ %+.2f" % d
			delta.add_theme_color_override("font_color", Color(0.42, 0.9, 0.46))
		else:
			delta.text = "▼ %+.2f" % d
			delta.add_theme_color_override("font_color", Color(0.95, 0.5, 0.45))


## Brighten the focused specimen; dim the others. Holders are added in _specimen_list() order by _render_specimens.
## Issue 3: the per-specimen world-space NAME LABEL (holder child 1) stays VISIBLE under every glyph now. The old
## "watermark" it produced was an artifact of the auto-zoom-on-focus (issue 2) scaling the world-space text into a
## giant faded banner; with that zoom removed, the label reads cleanly at the default frame directly under the
## model. Presentation only (inv #2).
func _emphasise_focus() -> void:
	if _specimen_root == null:
		return
	var kids := _specimen_root.get_children()
	for i in kids.size():
		var holder := kids[i] as Node2D
		holder.modulate = Color.WHITE if i == _focus else Color(1, 1, 1, 0.3)


# ──────────────────────────── mouse interaction: hover tooltip + click detail ────────────────────────────

## Build the hover tooltip (follows the cursor) and the pinned detail panel (set on click). Both READ-ONLY:
## they surface per-cell snapshot data + the species genome's ontology tags the core exported (invariant #2).
func _build_interaction_ui(ui: CanvasLayer) -> void:
	_tooltip = _dark_panel(0.62)
	_tooltip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_tooltip.visible = false
	_tooltip_label = Label.new()
	_tooltip_label.add_theme_font_size_override("font_size", 12)
	_tooltip_label.add_theme_color_override("font_color", Color(0.95, 0.98, 0.95))
	_tooltip.add_child(_tooltip_label)
	ui.add_child(_tooltip)

	var body := _dark_panel(0.55)
	body.custom_minimum_size = Vector2(250, 0)
	_detail_box = VBoxContainer.new()
	_detail_box.add_theme_constant_override("separation", 3)
	body.add_child(_detail_box)
	# Inspect (cell-click detail) docks BOTTOM-LEFT now (Phase U), above the control deck.
	_detail_panel = PanelChrome.new()
	_detail_panel.setup("🔍 INSPECT", body, ui, Vector2(12, maxf(120.0, _field_screen_size().y - 220.0)), _pill_rail)
	_detail_panel.visible = false


## Full-width bottom timeline: generation axis + play-head + click-to-seek (timeline.gd).
func _build_timeline(ui: CanvasLayer) -> void:
	_timeline = Timeline.new()
	_timeline.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	_timeline.offset_left = 8
	_timeline.offset_right = -8
	_timeline.offset_top = -54
	_timeline.offset_bottom = -6
	_timeline.seek.connect(_on_timeline_seek)
	ui.add_child(_timeline)
	var gens: Array = []
	for s in _snaps:
		gens.append(s.generation)
	_timeline.setup(gens)


func _on_timeline_seek(i: int) -> void:
	if _view_mode != 0 or _snaps.is_empty():
		return
	_paused = true
	_update_play_button()
	_show(i)


## A reusable translucent rounded panel (used by the tooltip + detail panel).
func _dark_panel(alpha: float) -> PanelContainer:
	var p := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, alpha)
	sb.set_corner_radius_all(6)
	sb.set_content_margin_all(8)
	p.add_theme_stylebox_override("panel", sb)
	return p


## Per-cell / per-plant summary that tracks the cursor. Hidden when the cursor is over nothing relevant.
func _update_tooltip() -> void:
	if _tooltip == null:
		return
	var world := get_global_mouse_position()
	var text := ""
	if _view_mode == 0 and not _snaps.is_empty():
		var _cc := _cell_at(world)
		var cx := _cc.x
		var cy := _cc.y
		var snap = _snaps[_idx]
		if cx >= 0 and cy >= 0 and cx < snap.width and cy < snap.height:
			var i: int = cy * snap.width + cx
			text = "(%d,%d)  d %.2f  a %.2f  f %.2f  moist %.2f" % [cx, cy, snap.density[i], snap.allele_freq[i], snap.fitness[i], snap.soil_moisture[i]]
	elif _view_mode == 1:
		var hit := _specimen_at(world)
		if hit >= 0:
			text = _specimen_tooltip(hit)
	if text == "":
		_tooltip.visible = false
		return
	_tooltip_label.text = text
	_tooltip.visible = true
	_tooltip.position = get_viewport().get_mouse_position() + Vector2(16, 14)


## The lazy codex tooltip one-liner for a hovered specimen (SP-4): the label + emoji + headline + role one-line,
## a pure string lookup keyed on the row's key/role (inv #2 — no biology). Degrades to the bare label when the
## codex has no entry for the species (a species can ship before its codex copy).
func _specimen_tooltip(hit: int) -> String:
	var spec: Dictionary = _specimen_list()[hit]
	var key := str(spec.get("key", "default"))
	var role_dbg := str(spec.get("role", ""))
	var label := str(spec.get("label", ""))
	var cx := _codex.species_for(key)
	if cx.is_empty():
		return label
	var emoji := str(cx.get("emoji", GlyphFactory.emoji_for(key, role_dbg)))
	var role_id := _role_id_from_debug(role_dbg, key)
	var role := _codex.role_for(role_id)
	var line := "%s  %s" % [emoji, label]
	if cx.has("headline"):
		line += "\n%s" % str(cx["headline"])
	if not role.is_empty():
		line += "\n%s — %s" % [_role_badge(role_id), str(role.get("one_line", ""))]
	return line


## Index of the specimen whose body (plant | microbe) bounds contain `world`, else -1.
func _specimen_at(world: Vector2) -> int:
	if _specimen_root == null:
		return -1
	var kids := _specimen_root.get_children()
	for i in kids.size():
		var holder := kids[i] as Node2D
		var glyph := holder.get_child(0) as Node2D
		if glyph != null and glyph.has_method("bounds"):
			var pb: Rect2 = glyph.bounds()
			if Rect2(holder.position + pb.position, pb.size).grow(40.0).has_point(world):
				return i
	return -1


## Left-click (not a drag): pin the detail panel for the clicked cell (ecosystem) or focus + detail the
## clicked plant (specimen).
func _on_click() -> void:
	var world := get_global_mouse_position()
	if _view_mode == 0:
		if _snaps.is_empty():
			return
		var snap = _snaps[_idx]
		var _cc := _cell_at(world)
		var cx := _cc.x
		var cy := _cc.y
		if cx >= 0 and cy >= 0 and cx < snap.width and cy < snap.height:
			var i: int = cy * snap.width + cx
			_fill_detail("Cell (%d, %d)" % [cx, cy], _cell_lines(snap, i))
			# ADR-029 S4: clicking a populated cell SELECTS its colony (packed species*65536+variant key) → that
			# district pops open to its members regardless of zoom (capped) while neighbours stay aggregated;
			# clicking empty ground clears the selection. Pure render scope (inv #2): a read of the inert planes.
			if snap.density[i] > 0.0 and "dominant_species_id" in snap:
				var sid := int(round(snap.dominant_species_id[i]))
				var vid := int(round(snap.dominant_variant_id[i])) if ("dominant_variant_id" in snap and i < snap.dominant_variant_id.size()) else 0
				_set_selected_colony(sid * 65536 + vid)
			else:
				_set_selected_colony(-1)
		else:
			_detail_panel.visible = false
			_set_selected_colony(-1)
	else:
		var hit := _specimen_at(world)
		if hit >= 0:
			_focus = hit
			if _specimen_picker != null:
				_specimen_picker.select(_focus)
			_on_specimen_selected(_focus)
			_fill_specimen_detail(hit)  # SP-4: the rich 6-section codex card for the FOCUSED species


## The per-cell stat lines (population channels + R1.0 soil channels + GSS3 pool channels) for the detail panel.
func _cell_lines(snap, i: int) -> Array:
	return [
		"density        %.3f" % snap.density[i],
		"allele_freq    %.3f" % snap.allele_freq[i],
		"fitness        %.3f" % snap.fitness[i],
		"soil moisture  %.3f" % snap.soil_moisture[i],
		"soil nutrients %.3f" % snap.soil_nutrients[i],
		"soil pH        %.3f" % snap.soil_ph[i],
		"light          %.3f" % snap.light[i],
		"free_nutrient  %.3f" % snap.free_nutrient[i],
		"detritus       %.3f" % snap.detritus[i],
		"toxin          %.3f" % snap.toxin[i],
		"kin            %.3f" % snap.kin[i],
		"alarm          %.3f" % snap.alarm[i],
	]


## Rewrite the detail panel: a title, optional stat lines, then the species-genome ontology (track-B prep).
func _fill_detail(title: String, stat_lines: Array) -> void:
	for c in _detail_box.get_children():
		c.queue_free()
	_detail_box.add_child(_detail_label(title, 15, Color(0.96, 0.99, 0.96)))
	for s in stat_lines:
		_detail_box.add_child(_detail_label(str(s), 12, Color(0.9, 0.94, 0.9)))
	var loci: Array = (_specimens.get("genome", {}) as Dictionary).get("loci", [])
	if not loci.is_empty():
		_detail_box.add_child(_detail_label("Genome (species) · ontology", 12, Color(0.7, 0.78, 0.7)))
		for l in loci:
			var ld: Dictionary = l
			var go: Array = ld.get("go_refs", [])
			_detail_box.add_child(_detail_label(
				"• %s   %s   %s" % [ld.get("name", ""), ld.get("so_term", ""), ", ".join(go)],
				11, Color(0.86, 0.9, 0.86)))
	_detail_panel.visible = true


func _detail_label(text: String, size: int, color: Color) -> Label:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_color", color)
	l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	l.custom_minimum_size = Vector2(236, 0)
	return l


# ──────────────────────────── SP-4 rich per-specimen INSPECT card ────────────────────────────

## The rich 6-section inspect card for the FOCUSED specimen (replaces the title-only specimen pin + the
## file-replay-only genome block of _fill_detail). Reads ONLY core-exported ids joined to the static codex
## (inv #2 — annotation, never derivation): the specimen row {key,label,traits,role}, the per-species
## observe_species() row, the widened _live.loci() ontology, and codex.gd lookups. Every section degrades
## gracefully — a missing codex entry falls back to bare ids; file-replay keeps the specimens.json genome path.
## FIXES the confirmed live-mode bug: the old block read loci ONLY from _specimens.genome.loci (the file-replay
## plant), so in --live it showed zero/wrong loci regardless of the focused species.
func _fill_specimen_detail(focus: int) -> void:
	for c in _detail_box.get_children():
		c.queue_free()
	var list := _specimen_list()
	if list.is_empty() or focus < 0 or focus >= list.size():
		_fill_detail("specimen", [])
		return
	var spec: Dictionary = list[focus]
	var key := str(spec.get("key", "default"))
	var role_dbg := str(spec.get("role", ""))  # Debug-cased TrophicRole ("Decomposer"); "" in file-replay
	var role_id := _role_id_from_debug(role_dbg, key)  # the codex/gp role id ("decomposer")
	var traits: Dictionary = spec.get("traits", {})
	var cx := species_for_or_empty(key)

	# 1. HEADER — emoji + display name + trophic-role badge.
	var emoji := str(cx.get("emoji", GlyphFactory.emoji_for(key, role_dbg)))
	var disp := str(cx.get("display_name", str(spec.get("label", key)).split(" — ")[0]))
	var role_title := _role_badge(role_id)
	_detail_box.add_child(_detail_label("%s  %s — %s" % [emoji, disp, role_title], 15, Color(0.97, 0.99, 0.96)))

	# 2. CODEX BLURB — headline + a short taxonomy/phenology line (with a "Codex ▸" affordance).
	if cx.has("headline"):
		_detail_box.add_child(_detail_label(str(cx["headline"]), 11, Color(0.86, 0.92, 0.88)))
	if cx.has("taxonomy"):
		_detail_box.add_child(_detail_label("Codex ▸  %s" % str(cx["taxonomy"]), 10, Color(0.66, 0.74, 0.70)))

	# 3. GENOME (loci/genes) — the FOCUSED species' loci with anchors FIRST/highlighted, then the rest.
	_fill_genome_section(key, cx)

	# 4. TRAITS WITH VALUES + GLOSS — value + delta-vs-baseline + a per-trait codex gene gloss.
	_fill_traits_section(key, traits, (list[0] as Dictionary).get("traits", {}))

	# 5. TROPHIC ROLE — the badge + the codex role one-liner.
	var role_entry := _codex.role_for(role_id)
	_detail_box.add_child(_detail_label("Trophic role", 12, Color(0.7, 0.78, 0.7)))
	var role_line := str(role_entry.get("one_line", "")) if not role_entry.is_empty() else "—"
	_detail_box.add_child(_detail_label("• %s — %s" % [role_title, role_line], 11, Color(0.86, 0.9, 0.86)))

	# 6. GENE ANCHORS + EDIT LINEAGE — anchor-gene chips + the per-species edit trail.
	_fill_anchors_and_lineage(key, cx, spec)

	_detail_panel.visible = true


## Genome section: the FOCUSED species' loci, anchors (codex anchor_genes order) first/highlighted, then a
## scrollable-style tail (capped). Live → _live.loci() when the focused species is the active one (widened with
## so_term+go_refs); else fall back to the codex anchor list; file-replay → _specimens.genome.loci. Each enriched
## row joins gene_for_go(go_refs[0]) → + go_label + one_line gloss.
func _fill_genome_section(key: String, cx: Dictionary) -> void:
	var loci := _loci_for_focus(key)
	_detail_box.add_child(_detail_label("Genome · ontology (%d loci)" % loci.size(), 12, Color(0.7, 0.78, 0.7)))
	if loci.is_empty():
		# No loci available (a non-active species in a multi-species run) → surface the codex anchor genes instead.
		var anchors: Array = cx.get("anchor_genes", [])
		for sym in anchors:
			var g := _codex.gene_for_symbol(str(sym))
			var gloss := (" — %s" % str(g.get("one_line", ""))) if not g.is_empty() else ""
			_detail_box.add_child(_detail_label("• %s%s" % [str(sym), gloss], 11, Color(0.9, 0.86, 0.62)))
		return
	# Order: anchor loci (by the codex anchor_genes order) first/highlighted, then the rest, capped for sanity.
	var anchors: Array = cx.get("anchor_genes", [])
	var anchor_set := {}
	for a in anchors:
		anchor_set[str(a)] = true
	var head: Array = []  # anchor loci
	var tail: Array = []  # the rest
	for l in loci:
		if anchor_set.has(str((l as Dictionary).get("name", ""))):
			head.append(l)
		else:
			tail.append(l)
	for l in head:
		_detail_box.add_child(_locus_row(l, true))
	const TAIL_CAP := 24  # keep the panel sane for E. coli's 136 genes; the anchors (the levers) always show
	var shown := 0
	for l in tail:
		if shown >= TAIL_CAP:
			_detail_box.add_child(_detail_label("  … +%d more loci" % (tail.size() - shown), 10, Color(0.6, 0.66, 0.62)))
			break
		_detail_box.add_child(_locus_row(l, false))
		shown += 1


## One enriched locus row: "• <name>  SO:<so>  GO:<go>" + codex gloss (go_label + one_line) when present.
func _locus_row(l: Variant, anchor: bool) -> Label:
	var ld: Dictionary = l
	var name := str(ld.get("name", ""))
	var so := int(ld.get("so_term", 0))
	var go_refs: Array = ld.get("go_refs", [])
	var go0 := int(go_refs[0]) if not go_refs.is_empty() else 0
	var text := "• %s  SO:%d" % [name, so]
	if go0 > 0:
		text += "  GO:%d" % go0
		var g := _codex.gene_for_go(go0)
		if not g.is_empty():
			text += "  %s — %s" % [str(g.get("go_label", "")), str(g.get("one_line", ""))]
	var col := Color(0.95, 0.88, 0.55) if anchor else Color(0.84, 0.88, 0.84)
	return _detail_label(text, 11 if anchor else 10, col)


## Traits section: the focused species' traits with value + delta-vs-baseline + a codex gloss via the trait←gene
## join ("RespirationMode ← pflB (pyruvate formate-lyase)"). Uses _active_trait_keys() so the right set shows.
func _fill_traits_section(key: String, focused: Dictionary, base: Dictionary) -> void:
	_detail_box.add_child(_detail_label("Traits", 12, Color(0.7, 0.78, 0.7)))
	for snake in _active_trait_keys():
		var v := clampf(float(focused.get(snake, 0.0)), 0.0, 1.0)
		var b := clampf(float(base.get(snake, 0.0)), 0.0, 1.0)
		var d := v - b
		var dtxt := "" if absf(d) < 0.0005 else ("  (%+.2f)" % d)
		var line := "• %s  %.3f%s" % [snake, v, dtxt]
		var g := _codex.gene_for_trait(snake, key)
		if not g.is_empty():
			line += "   ← %s (%s)" % [str(g.get("symbol", "")), str(g.get("go_label", g.get("one_line", "")))]
		_detail_box.add_child(_detail_label(line, 10, Color(0.86, 0.9, 0.86)))


## Anchors + lineage section: the codex anchor_genes as chips, then the per-species edit trail from
## _live_species_logs[sid].entries (baseline → edit 1 → edit 2 …, each label carrying the gen).
func _fill_anchors_and_lineage(key: String, cx: Dictionary, spec: Dictionary) -> void:
	var anchors: Array = cx.get("anchor_genes", [])
	if not anchors.is_empty():
		_detail_box.add_child(_detail_label("Gene anchors", 12, Color(0.7, 0.78, 0.7)))
		_detail_box.add_child(_detail_label("  " + "  ·  ".join(PackedStringArray(anchors)), 11, Color(0.9, 0.86, 0.62)))
	# Edit lineage — the focused species' per-species log entries (baseline → edits), labels carry the gen.
	var sname := str(spec.get("label", "")).split(" — ")[0]
	var entries := _lineage_entries_for(key, sname)
	if entries.size() >= 1:
		_detail_box.add_child(_detail_label("Lineage / edit history", 12, Color(0.7, 0.78, 0.7)))
		for e in entries:
			_detail_box.add_child(_detail_label("  → %s" % str((e as Dictionary).get("label", "")), 10, Color(0.82, 0.86, 0.88)))


## The per-species log entries (baseline + edits) for a focused species (live only); [] in file-replay.
func _lineage_entries_for(key: String, sname: String) -> Array:
	for sid in _live_species_order:
		var log_entry: Dictionary = _live_species_logs[sid]
		if str(log_entry.get("key", "")) == key:
			return log_entry.get("entries", [])
	return []


## The loci to show for the focused species. Live: _live.loci() when the focused species is the active selected
## one (the only genome the boundary exposes); file-replay: _specimens.genome.loci. [] when neither applies (a
## non-active species in a multi-species live run — the genome section then falls back to codex anchors).
func _loci_for_focus(key: String) -> Array:
	if _live != null and _live.has_method("loci"):
		var active := "default"
		if _live.has_method("species_key"):
			var k := String(_live.species_key())
			if k != "":
				active = k
		if key == active:
			return _live.loci()
		# Active species' loci only — if the focused row IS the default/active, still show; else fall back below.
		if key == "default" and active == "default":
			return _live.loci()
		return []
	return (_specimens.get("genome", {}) as Dictionary).get("loci", [])


## The species codex entry, or {} (graceful). Thin wrapper so the section helpers read clean.
func species_for_or_empty(key: String) -> Dictionary:
	return _codex.species_for(key)


## A human role badge for a codex role id (title-cased), falling back to the raw id.
func _role_badge(role_id: String) -> String:
	var r := _codex.role_for(role_id)
	if not r.is_empty():
		return str(r.get("title", role_id)).split(" (")[0]
	return role_id.capitalize() if role_id != "" else "—"


## Normalize a Debug-cased TrophicRole ("Decomposer"/"ObligateSymbiont") to the gp/codex role id
## ("decomposer"/"symbiont"). Falls back to the species key→role for file-replay (no role string).
func _role_id_from_debug(role_dbg: String, key: String) -> String:
	match role_dbg:
		"Autotroph": return "autotroph"
		"Heterotroph": return "heterotroph"
		"Mixotroph": return "mixotroph"
		"Decomposer": return "decomposer"
		"Predator": return "predator"
		"ObligateSymbiont": return "symbiont"
		_:
			# File-replay / unknown: a small key→role map mirroring the species JSONs (graceful, no biology).
			match key:
				"ecoli-core", "bacillus", "cutibacterium", "aspergillus-niger", "penicillium": return "decomposer"
				"bdellovibrio": return "predator"
				"mycoplasma", "staph": return "heterotroph"
				"pseudomonas": return "mixotroph"
				"carsonella", "syn3": return "symbiont"
				_: return "autotroph"


# ──────────────────────────── SP-4 headless --check guards ────────────────────────────

## Build EVERY baked species' glyph via the key-led factory with a representative trait vector, so a parse error
## or a malformed polygon in ANY morphotype body (rod/cocci/vibrioid/spore-former/wall-less/symbiont/mold/plant)
## goes RED at build time — never only under a GPU (inv #4). Returns the count of glyphs built. The factory's
## build() precomputes all geometry, so this exercises the full geometry path without _draw().
func _check_build_all_glyphs() -> int:
	# (key, role) for each baked species — mirrors the species JSONs (key-led table; role for the fallback path).
	var roster := [
		["default", "Autotroph"], ["ecoli-core", "Decomposer"], ["bdellovibrio", "Predator"],
		["staph", "Heterotroph"], ["cutibacterium", "Decomposer"], ["pseudomonas", "Mixotroph"],
		["bacillus", "Decomposer"], ["aspergillus-niger", "Decomposer"], ["penicillium", "Decomposer"],
		["mycoplasma", "Heterotroph"], ["carsonella", "ObligateSymbiont"], ["syn3", "ObligateSymbiont"],
		# An UNKNOWN key → role fallback must still draw SOMETHING (graceful degrade).
		["future-unknown-species", "Heterotroph"],
	]
	# A representative trait vector touching every lever a morphotype reads (so the spore/predation/symbiosis
	# branches all run): all set to mid so endospore/biofilm/conidia/tether/curvature are exercised.
	var t := {
		"growth_rate": 0.7, "stature": 0.6, "branchiness": 0.6, "leaf_size": 0.6, "leaf_hue": 0.5,
		"reflectance": 0.5, "fecundity": 0.5, "drought_tolerance": 0.5, "kill_switch_linkage": 0.3,
		"glucose_uptake": 0.6, "respiration_mode": 0.5, "acetate_overflow": 0.5, "fermentation_capacity": 0.5,
		"predation_capacity": 0.7, "sporulation_capacity": 0.6, "symbiosis_capacity": 0.6,
	}
	var built := 0
	for r in roster:
		var spec := {"key": r[0], "role": r[1], "traits": t, "loci_count": 16}
		var g := GlyphFactory.make(str(r[0]), str(r[1]), t, spec, built + 1)
		# Touch bounds() so a glyph that built no geometry surfaces (a Rect2() is fine; a crash here is RED).
		var _b: Rect2 = g.bounds()
		g.free()
		built += 1
	return built


## Exercise the codex-enriched inspect join headlessly with a real species (E. coli), so a garbled codex.json or
## a broken join (species_for / gene_for_go / role_for / gene_for_trait) goes RED. Returns true if the codex
## loaded AND the E. coli join resolved (the species entry + the gltA gene by GO + the decomposer role).
func _check_codex_inspect() -> bool:
	if not _codex.is_loaded():
		push_warning("--check: codex.gd did not load res://data/codex/codex.json")
		return false
	# The joins the inspect card relies on — each must resolve for the shipped content.
	var sp := _codex.species_for("ecoli-core")
	var gltA := _codex.gene_for_go(4108)  # gltA's GO ref (a locus go_refs[0] in ecoli.json)
	var role := _codex.role_for("decomposer")
	var by_trait := _codex.gene_for_trait("growth_rate", "ecoli-core")
	var ok := not sp.is_empty() and not gltA.is_empty() and not role.is_empty() and not by_trait.is_empty()
	if not ok:
		push_warning("--check: codex inspect join failed (species=%s gene=%s role=%s trait=%s)" % [
			not sp.is_empty(), not gltA.is_empty(), not role.is_empty(), not by_trait.is_empty()])
	return ok


# ──────────────────────────── per-snapshot update ────────────────────────────

func _show(i: int) -> void:
	if i < 0 or i >= _snaps.size():
		return
	_idx = i
	var snap = _snaps[i]
	# Feed the ecosystem sprites the run-level species visual template BEFORE the snapshot, so the per-cell draw
	# reads the precomputed glyph params. Pure presentation (inv #2): traits are already-expressed core scalars.
	if _organisms.has_method("set_species_traits"):
		var st := _ecosystem_species_traits()
		_organisms.set_species_traits(st.get("traits", {}), str(st.get("key", "default")))
	# GSS5 species-map: hand the organism layer the per-cell dominant SpeciesId plane + the species-id → visual
	# table (size/colour by key/role), so each cell's organisms render at their dominant species' real-cell scale
	# (plant LARGE … symbiont tiny) instead of one shared density radius. Pure presentation (inv #2): the core
	# decided which species dominates each cell; this only maps id → pixels. Empty table (file-replay / older
	# cdylib) → the default plant visual (graceful).
	var vis_table := SpeciesVisualMap.build_table(_species_id_meta())
	if _organisms.has_method("set_dominant_species_ids"):
		var dom: PackedFloat32Array = snap.dominant_species_id if "dominant_species_id" in snap else PackedFloat32Array()
		_organisms.set_dominant_species_ids(dom, vis_table)
	_organisms.set_snapshot(snap, _cell)
	# S4: feed the organism layer the GSS6 Variant plane + the selected colony so its per-cell selected-pop
	# override can key on (species,variant). Read-only (inv #2): an inert per-cell ordinal + a render-scope id.
	if _organisms.has_method("set_variant_plane"):
		var var_plane: PackedFloat32Array = snap.dominant_variant_id if "dominant_variant_id" in snap else PackedFloat32Array()
		_organisms.set_variant_plane(var_plane)
	if _organisms.has_method("set_selected_colony"):
		_organisms.set_selected_colony(_selected_colony_id)
	# S4: resolve any fired-but-unminted brush strokes against THIS (post-edit) snapshot → the colony registry
	# learns the freshly-minted child Variant id appearing in the disc (ordered passes; inv #3). Then feed the
	# colony layer the registry + selection BEFORE the snapshot (which rebuilds districts using both).
	_resolve_pending_colonies(snap)
	# ADR-029 S2/S4: feed the colony layer the two off-hash identity planes (dominant_species_id +
	# dominant_variant_id) + the species visual table + the registry + selection; it derives the district polygons
	# (incl. the brushed child as a nested hole-cut district). Guarded so an older (pre-GSS6) snapshot lacking
	# dominant_variant_id never crashes (colonies.gd treats it as all-0).
	if _colonies != null:
		if _colonies.has_method("set_colony_registry"):
			_colonies.set_colony_registry(_colony_registry)
		if _colonies.has_method("set_selected_colony"):
			_colonies.set_selected_colony(_selected_colony_id)
		if _colonies.has_method("set_snapshot"):
			_colonies.set_snapshot(snap, _cell, vis_table)
	_update_scope_layers()
	if _iso_ground != null:
		_iso_ground.set_snapshot(snap, _overlay_mode)  # iso draws ground + data overlay as diamonds
	_update_overlay(snap)
	_refresh_hud()
	_eval_mission()
	_sync_controls()


## Feed the per-cell data texture (R=density, G=allele_freq, B=fitness) to the data-layer shader and select
## the active channel via the `layer` uniform. The colormap + alpha live in data_layer.gdshader (GPU). Under
## --iso the shader sprite stays hidden — the iso ground node renders the overlay into the diamonds instead.
func _update_overlay(snap) -> void:
	if _iso != null:
		return  # iso_ground (fed in _show) owns the overlay in isometric mode
	if _overlay_mode == 0:
		_overlay.visible = false
		return
	_overlay.visible = true
	_overlay.texture = ImageTexture.create_from_image(snap.to_data_image())
	_overlay.scale = Vector2(_cell, _cell)
	var mat := _overlay.material as ShaderMaterial
	if mat != null:
		# layer 0..2 sample the population texture; 3..5 the soil texture (R1.0); 6..8 the pool texture (GSS3);
		# 9..11 the chem texture (GSS4, ADR-013 F5: toxin/kin/alarm).
		mat.set_shader_parameter("layer", _overlay_mode - 1)
		mat.set_shader_parameter("soil_tex", ImageTexture.create_from_image(snap.to_soil_image()))
		mat.set_shader_parameter("pool_tex", ImageTexture.create_from_image(snap.to_pool_image()))
		mat.set_shader_parameter("chem_tex", ImageTexture.create_from_image(snap.to_chem_image()))


func _refresh_hud() -> void:
	_refresh_vitals()  # title-bar chips + Vitals scoreboard + sparkline
	if _view_mode == VIEW_SPECIMEN:
		# Specimen view: caption in the title status; hide the data legend.
		if _title_status != null:
			# `[lb]V back]` escapes the literal "[" for the BBCode title-bar RichTextLabel (Item b made it bbcode).
			var edits := _specimen_list().size() - 1
			_title_status.text = ("specimen view — baseline + %d edited genome(s)   [lb]V back]" % maxi(0, edits)
				if edits >= 0 else "specimen view — no specimens.json   [lb]V back]")
		if _legend != null:
			_legend.set_active(false)
		return
	if _view_mode == VIEW_RELATIONS:
		# Relations view: caption in the title status; the inferno data legend is irrelevant (the heatmap carries
		# its own diverging sign/magnitude legend strip).
		if _title_status != null:
			_title_status.text = "relations view — S×S inter-species joule flows   [lb]V back]"
		if _legend != null:
			_legend.set_active(false)
		return
	if _legend != null:
		_legend.set_active(_overlay_mode != 0)
		if _overlay_mode != 0 and _legend_label != null:
			var nm: String = OVERLAY_NAMES[_overlay_mode]
			_legend_label.text = OVERLAY_LEGENDS.get(nm, "%s   low → high" % nm)


## Name the current zoom scope from the magnification (HUD only).
func _scope_label() -> String:
	var z := _cam.zoom.x
	if z < 1.8:
		return "field"
	elif z < 4.2:
		return "patch"
	return "cells"


func _set_zoom(z: float) -> void:
	var zc := clampf(z, ZOOM_MIN, ZOOM_MAX)
	_cam.zoom = Vector2(zc, zc)
	_refresh_hud()
	_sync_scope_buttons()
	_update_scope_layers()


## ADR-029 S3: the per-colony LOD POP ladder — NOT a binary layer swap. BOTH layers are now visible at once; each
## decides its own rung from ITS on-screen FOOTPRINT (§4.1: _cell * cam.zoom.x * size_scale(species)). A district
## below its pop threshold draws as a polygon (colonies.gd) with ZERO per-cell sprites (organisms.gd skips it);
## a district above the threshold fades its polygon to a thin ghost+outline while its cells POP to per-cell sprites
## — both sides crossfading as a pure function of footprint over the SAME 6-8px band. Because size_scale is in the
## footprint, PLANT districts (2.2) pop FIRST while microbe districts stay polygons. We just thread cam.zoom.x into
## both layers; the per-frame-free crossfade lives inside each layer's _draw (inv #3: redraw only on this zoom/scope
## change, never _process). Pure presentation visibility (inv #2): no biology, no determinism exposure.
func _update_scope_layers() -> void:
	if _organisms == null or _colonies == null or _cam == null:
		return
	var z := _cam.zoom.x
	_colonies.visible = true
	_organisms.visible = true
	if _colonies.has_method("set_zoom"):
		_colonies.set_zoom(z)
	if _organisms.has_method("set_zoom"):
		_organisms.set_zoom(z)


## Jump to a named zoom scope preset (keys 1/2/3).
func _set_scope(i: int) -> void:
	_set_zoom(float(SCOPES[i]["zoom"]))


## Pan the camera; step is in world pixels, scaled inversely with zoom so it feels constant on screen.
func _pan(dir: Vector2) -> void:
	_cam.position += dir * (90.0 / _cam.zoom.x)
	_refresh_hud()


func _advance() -> void:
	if _paused or _view_mode != 0 or _snaps.size() < 2:
		return
	_idx = (_idx + 1) % _snaps.size()
	_show(_idx)


# ──────────────────────────── input (windowed) ────────────────────────────

func _unhandled_input(event: InputEvent) -> void:
	# While the pre-run menu is up it is a modal gate (ADR-012 E4): swallow every sim hotkey — its dim backdrop
	# already blocks the mouse, but keyboard would otherwise leak through (ESC=quit, SPACE/V/D/B/S mutate the
	# hidden sim). The menu's own controls handle the keys they need.
	if _menu != null:
		return
	if event is InputEventMouseButton:
		# Brush mode: wheel = brush radius, left-click = paint a region edit. Else wheel = zoom, click = inspect.
		if _brush_on:
			if event.button_index == MOUSE_BUTTON_WHEEL_UP and event.pressed:
				_set_brush_radius(_brush_radius + 1)
			elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN and event.pressed:
				_set_brush_radius(_brush_radius - 1)
			elif event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
				_update_brush_cell()
				_brush_painting = true  # begin a drag-paint stroke (POSITION MATTERS along the drag)
				_apply_active_tool()
			elif event.button_index == MOUSE_BUTTON_LEFT and not event.pressed:
				_brush_painting = false  # end the stroke
			return
		if event.button_index == MOUSE_BUTTON_WHEEL_UP and event.pressed:
			_set_zoom(_cam.zoom.x * 1.15)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN and event.pressed:
			_set_zoom(_cam.zoom.x / 1.15)
		elif event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_dragging = true
				_drag_moved = 0.0
			else:
				if _dragging and _drag_moved < 6.0:  # a click, not a drag → inspect
					_on_click()
				_dragging = false
		return

	if event is InputEventMouseMotion:
		if _brush_on:
			var prev_cell := _brush_cell
			_update_brush_cell()  # follow the cursor with the brush preview
			# Drag-to-paint (SP-3.6): while the button is held, fire the active tool at each NEWLY-hovered cell.
			if _brush_painting and (event.button_mask & MOUSE_BUTTON_MASK_LEFT) and _brush_cell != prev_cell:
				_apply_active_tool()
			if _tooltip != null:
				_tooltip.visible = false
			return
		if _dragging and (event.button_mask & MOUSE_BUTTON_MASK_LEFT):
			_cam.position -= event.relative / _cam.zoom.x  # drag the map under the cursor
			_drag_moved += event.relative.length()
			if _tooltip != null:
				_tooltip.visible = false
		else:
			_update_tooltip()
		return

	if not (event is InputEventKey and event.pressed and not event.echo):
		return
	match event.keycode:
		KEY_SPACE:
			_paused = not _paused
			_update_play_button()
			_refresh_hud()
		KEY_V:
			_set_view_mode((_view_mode + 1) % VIEW_COUNT)
		KEY_TAB:
			# Cycle the focused specimen (specimen view only); guard empty list (no div-by-zero).
			if _view_mode == 1 and not _specimen_list().is_empty():
				_focus = (_focus + 1) % _specimen_list().size()
				if _specimen_picker != null:
					_specimen_picker.select(_focus)
				_on_specimen_selected(_focus)
		KEY_D:
			if _view_mode == 0:
				_overlay_mode = (_overlay_mode + 1) % OVERLAY_NAMES.size()
				if _layer_picker != null:
					_layer_picker.selected = _overlay_mode
				_show(_idx)
		KEY_S:
			if _view_mode == 0 and _organisms != null:  # toggle trait-driven plant sprites vs plain dots
				_organisms.set_sprites_on(not _organisms._sprites_on)
		KEY_B:
			if _view_mode == 0 and _live != null:  # toggle the selective region-edit brush (live only)
				_set_brush_mode(not _brush_on)
		KEY_BRACKETLEFT:
			if _brush_on:
				_set_brush_radius(_brush_radius - 1)
		KEY_BRACKETRIGHT:
			if _brush_on:
				_set_brush_radius(_brush_radius + 1)
		KEY_PERIOD:
			_paused = true
			_show((_idx + 1) % _snaps.size())
		KEY_COMMA:
			_paused = true
			_show((_idx - 1 + _snaps.size()) % _snaps.size())
		KEY_1:
			_set_scope(0)
		KEY_2:
			_set_scope(1)
		KEY_3:
			_set_scope(2)
		KEY_LEFT:
			_pan(Vector2.LEFT)
		KEY_RIGHT:
			_pan(Vector2.RIGHT)
		KEY_UP:
			_pan(Vector2.UP)
		KEY_DOWN:
			_pan(Vector2.DOWN)
		KEY_ESCAPE:
			get_tree().quit()


# ──────────────────────────── snapshot loading / discovery (read-only) ────────────────────────────

## Load every snap_*.bin in a run dir, ordered by generation.
func _load_run(dir_path: String) -> Array:
	var out: Array = []
	var d := DirAccess.open(dir_path)
	if d == null:
		printerr("cannot open run dir: %s" % dir_path)
		return out
	for name in d.get_files():
		if name.begins_with("snap_") and name.ends_with(".bin"):
			var snap := SnapshotReader.load_from(dir_path.path_join(name))
			if snap != null:
				out.append(snap)
	out.sort_custom(func(a, b): return a.generation < b.generation)
	print("loaded %d snapshots from %s" % [out.size(), dir_path])
	return out


## Load specimens.json (baseline + edited trait vectors) from a run dir, if present. Read-only — the trait
## values were computed by the core; the renderer only reads them (invariant #2). {} if absent/malformed.
func _load_specimens(dir_path: String) -> Dictionary:
	var path := dir_path.path_join("specimens.json")
	if not FileAccess.file_exists(path):
		return {}
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return {}
	var data: Variant = JSON.parse_string(f.get_as_text())
	if typeof(data) != TYPE_DICTIONARY:
		printerr("specimens.json: not a JSON object")
		return {}
	var edits: Array = data.get("edits", [])
	print("loaded specimens: baseline + %d edits from %s" % [edits.size(), dir_path])
	return data


## Newest data/runs/<id>/ (by modified time) that contains at least one snap_*.bin. "" if none.
func _newest_run_dir() -> String:
	var runs := "res://../data/runs"
	var d := DirAccess.open(runs)
	if d == null:
		return ""
	var best := ""
	var best_mtime := -1
	for sub in d.get_directories():
		var p := runs.path_join(sub)
		var has_snap := false
		var sd := DirAccess.open(p)
		if sd != null:
			for f in sd.get_files():
				if f.begins_with("snap_") and f.ends_with(".bin"):
					has_snap = true
					break
		if has_snap:
			var m := FileAccess.get_modified_time(p)
			if m > best_mtime:
				best_mtime = m
				best = p
	return best


## Index of the snapshot whose generation == want; -1/unknown → the last one.
func _pick_index(want: int) -> int:
	if want >= 0:
		for i in _snaps.size():
			if int(_snaps[i].generation) == want:
				return i
	return _snaps.size() - 1


# ──────────────────────────── screenshot (verification) ────────────────────────────

func _take_shot(out_path: String) -> void:
	# Two frames so the TileMap, overlay texture and organism _draw() have all flushed to the viewport.
	await RenderingServer.frame_post_draw
	await RenderingServer.frame_post_draw
	var tex := get_viewport().get_texture()
	var img: Image = tex.get_image() if tex != null else null
	if img == null:
		printerr("shot FAILED: no viewport image (headless has no GPU; run windowed for --shot)")
		get_tree().quit(1)
		return
	var err := img.save_png(out_path)
	if err != OK:
		printerr("shot FAILED: save_png(%s) err %d" % [out_path, err])
		get_tree().quit(1)
		return
	print("shot OK — %s (%dx%d), gen=%d" % [out_path, img.get_width(), img.get_height(), _snaps[_idx].generation])
	get_tree().quit()


# ──────────────────────────── helpers ────────────────────────────

## Read a `--flag value` pair from the user command line (args after `--`). Returns `fallback` if absent.
func _arg_value(flag: String, fallback: String = "") -> String:
	var args := OS.get_cmdline_user_args()
	var idx := args.find(flag)
	if idx != -1 and idx + 1 < args.size():
		return args[idx + 1]
	return fallback


## True if a valueless flag (e.g. `--check`) is present on the user command line.
func _has_flag(flag: String) -> bool:
	return OS.get_cmdline_user_args().has(flag)


func _hash01(x: int, y: int, k: int) -> float:
	var h := (x * 73856093) ^ (y * 19349663) ^ ((k + 1) * 83492791)
	h = (h ^ (h >> 13)) * 1274126177
	h = h ^ (h >> 16)
	return float(h & 0xffff) / 65535.0
