extends RefCounted
## Codex loader (SP-4): the renderer-only encyclopedia layer. Loads the static res://data/codex/codex.json and
## builds ordered lookup indices keyed on ids the core ALREADY exports (species `key`, locus `go`/`so`,
## TrophicRole id, FlowMatrix from/to roles). It ANNOTATES biology the core computed — it never derives any.
##
## INVARIANT #2 (STOP THE LINE if violated): pure presentation/content. No genotype→phenotype, no flow
## computation, no SimRng. Every method is a static string/dict lookup keyed on a core-exported id.
##
## INVARIANT #3 (UI hygiene): iterate the ORDERED arrays from the file to build the indices (never a Dictionary's
## key order). The lookup dicts below are built once, in declared array order; reads are O(1) by id.
##
## Graceful degrade: a missing entry returns {} so a species/gene/role can ship before its codex copy exists —
## the caller falls back to bare exported ids, never an error, never a blank. Mirrors the loader discipline
## main.gd already uses for res://data/species (FileAccess + JSON.parse_string of inert bytes).
##
## Loaded via `preload(...)`-then-`.new()` (NOT class_name) — the headless --check trap: a bare global is
## unresolved without an editor import pass; preload needs no cache.

const CODEX_PATH := "res://data/codex/codex.json"

var format_version: int = 0
var _ok := false
# Ordered indices (built once from the ordered arrays). Keys are the core-exported ids.
var _species: Dictionary = {}  # key -> species entry dict
var _genes_by_go: Dictionary = {}  # int(go) -> gene entry dict
var _genes_by_symbol: Dictionary = {}  # symbol -> gene entry dict
var _roles: Dictionary = {}  # id -> role entry dict
var _flows: Dictionary = {}  # "from|to" -> flow entry dict
# The ordered key lists (for any UI that wants to walk entries in declared order, inv #3).
var _species_order: Array = []
var _gene_order: Array = []
var _role_order: Array = []


func _init() -> void:
	load_codex()


## (Re)load the codex from res://. Safe to call repeatedly; rebuilds the indices from the ordered arrays.
## Returns true when the file parsed and at least the four arrays were present (graceful otherwise).
func load_codex() -> bool:
	_ok = false
	_species = {}
	_genes_by_go = {}
	_genes_by_symbol = {}
	_roles = {}
	_flows = {}
	_species_order = []
	_gene_order = []
	_role_order = []
	if not FileAccess.file_exists(CODEX_PATH):
		push_warning("codex.gd: %s not found (staging?) — INSPECT/tooltips degrade to bare ids" % CODEX_PATH)
		return false
	var f := FileAccess.open(CODEX_PATH, FileAccess.READ)
	if f == null:
		push_warning("codex.gd: could not open %s" % CODEX_PATH)
		return false
	var text := f.get_as_text()
	f.close()
	var parsed: Variant = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		push_warning("codex.gd: %s did not parse to a JSON object" % CODEX_PATH)
		return false
	var doc: Dictionary = parsed
	format_version = int(doc.get("format_version", 0))
	# Iterate each ORDERED array (inv #3) and key its entries. Missing arrays are tolerated (empty index).
	for sp in doc.get("species", []):
		if typeof(sp) != TYPE_DICTIONARY:
			continue
		var key := str((sp as Dictionary).get("key", ""))
		if key == "":
			continue
		_species[key] = sp
		_species_order.append(key)
	for g in doc.get("genes", []):
		if typeof(g) != TYPE_DICTIONARY:
			continue
		var gd: Dictionary = g
		var sym := str(gd.get("symbol", ""))
		if gd.has("go"):
			_genes_by_go[int(gd["go"])] = gd
		if sym != "":
			_genes_by_symbol[sym] = gd
			_gene_order.append(sym)
	for r in doc.get("roles", []):
		if typeof(r) != TYPE_DICTIONARY:
			continue
		var rid := str((r as Dictionary).get("id", ""))
		if rid == "":
			continue
		_roles[rid] = r
		_role_order.append(rid)
	for fl in doc.get("flows", []):
		if typeof(fl) != TYPE_DICTIONARY:
			continue
		var fd: Dictionary = fl
		var fk := "%s|%s" % [str(fd.get("from_role", "")), str(fd.get("to_role", ""))]
		_flows[fk] = fd
	_ok = true
	return true


## Whether the codex parsed successfully (at least the file was a valid JSON object).
func is_loaded() -> bool:
	return _ok


## Species entry for a core-exported species `key` (e.g. "ecoli-core"), or {} if absent.
func species_for(key: String) -> Dictionary:
	return _species.get(key, {})


## Gene entry by GO ref (the int the core exports in a locus's go_refs), or {} if absent.
func gene_for_go(go: int) -> Dictionary:
	return _genes_by_go.get(go, {})


## Gene entry by symbol (the locus `name`), or {} if absent.
func gene_for_symbol(sym: String) -> Dictionary:
	return _genes_by_symbol.get(sym, {})


## Role entry by id (must match gp::role_from_str: autotroph/heterotroph/mixotroph/decomposer/predator/symbiont),
## or {} if absent.
func role_for(id: String) -> Dictionary:
	return _roles.get(id, {})


## Flow entry for a FlowMatrix edge (from_role → to_role), or {} if absent.
func flow_for(from_role: String, to_role: String) -> Dictionary:
	return _flows.get("%s|%s" % [from_role, to_role], {})


## The first gene entry whose `trait` field matches `trait_key` (a Trait::snake_name) for a species, or {}.
## Used by the trait-readout gloss join ("RespirationMode ← pflB (pyruvate formate-lyase)").
func gene_for_trait(trait_key: String, species_key: String) -> Dictionary:
	for sym in _gene_order:
		var g: Dictionary = _genes_by_symbol[sym]
		if str(g.get("trait", "")) == trait_key and str(g.get("species_key", "")) == species_key:
			return g
	# Fall back to a trait-only match (so a future species reusing a gene still glosses).
	for sym in _gene_order:
		var g: Dictionary = _genes_by_symbol[sym]
		if str(g.get("trait", "")) == trait_key:
			return g
	return {}


## Ordered species keys (declared order, inv #3) — for any browsable list.
func species_keys() -> Array:
	return _species_order.duplicate()
