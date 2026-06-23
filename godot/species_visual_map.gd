extends RefCounted
## Per-species VISUAL table for the ecosystem map (the species-sized/coloured field). Maps a species KEY
## (with a ROLE fallback) to a real-cell-scale SIZE multiplier + a base COLOR, so the map can draw each cell's
## dominant species at a biologically legible relative size — plant/mold LARGE, E.coli/Bacillus small rods,
## Bdellovibrio a tiny speck, Carsonella/Syn3 tinier — instead of every species at one density-derived radius.
##
## INVARIANT #2 (STOP THE LINE if violated): PURE PRESENTATION. The core EXPORTS a per-cell dominant species id
## (GSS5); the renderer maps that id → key/role (from observe_species) → a visual here. NO biology / no
## genotype→phenotype is computed in GDScript — this is a lookup table from an already-decided species to pixels,
## exactly the role glyph_factory.gd plays for the specimen view. It mirrors glyph_factory's MORPH_BY_KEY keys so
## the field view and the specimen view agree on which species is which.
##
## INVARIANT #3: a single by-key lookup (or a role fallback) — no HashMap iteration in any order-sensitive path,
## no RNG. Unknown key/role → a graceful DEFAULT visual (never a crash); the size scale is a relative multiplier
## the per-cell draw applies to its base radius.

# Relative SIZE multipliers on a real-organism scale (× the cell base radius). The arc spans ~the real
# magnitude gap: a plant canopy / mold colony is metres-to-millimetres of visible mass, an E. coli rod ~2 µm,
# a Bdellovibrio attack cell ~0.3 µm, a reduced-genome symbiont (Carsonella/Syn3) smaller still. We compress
# that ~order-of-magnitude story into a legible 0.30…2.2 multiplier so the smallest specks still read.
const SIZE_PLANT := 2.2       # plant canopy — the largest visible mass
const SIZE_MOLD := 1.9        # filamentous mold colony — large
const SIZE_ROD := 0.9         # E. coli / Bacillus / Pseudomonas — a small rod
const SIZE_COCCUS := 0.75     # staph cocci cluster — small
const SIZE_PLEOMORPH := 0.6   # wall-less mycoplasma — soft, small
const SIZE_VIBRIOID := 0.5    # Bdellovibrio — a tiny predatory speck
const SIZE_SYMBIONT := 0.34   # Carsonella / Syn3 — the tiniest (reduced-genome) speck
const SIZE_DEFAULT := 1.0     # unknown species → a neutral mid radius (graceful)

# Base COLORs (HSV-derived RGB) chosen DISJOINT per morphotype so adjacent dominant-species cells read apart.
# These are the field-view base hues; the per-cell allele/fitness still tint within (see organisms.gd).
const COLOR_PLANT := Color(0.36, 0.62, 0.24)      # leaf green
const COLOR_MOLD := Color(0.62, 0.50, 0.30)       # ochre mycelium
const COLOR_ROD := Color(0.30, 0.66, 0.74)        # cyan-teal rod
const COLOR_COCCUS := Color(0.88, 0.78, 0.36)     # golden (staphyloxanthin)
const COLOR_PLEOMORPH := Color(0.74, 0.66, 0.82)  # soft violet
const COLOR_VIBRIOID := Color(0.74, 0.40, 0.78)   # magenta predator
const COLOR_SYMBIONT := Color(0.80, 0.72, 0.56)   # pale tan speck
const COLOR_DEFAULT := Color(0.50, 0.62, 0.40)    # neutral green-grey

## Species KEY → morphotype string. Mirrors glyph_factory.MORPH_BY_KEY so the field + specimen views agree on
## which species draws as what. Read by a single lookup (never iterated order-sensitively — inv #3).
const MORPH_BY_KEY := {
	"default": "plant",
	"ecoli-core": "rod",
	"bdellovibrio": "vibrioid",
	"staph": "cocci",
	"cutibacterium": "rod",
	"pseudomonas": "rod",
	"bacillus": "rod",
	"aspergillus-niger": "mold",
	"penicillium": "mold",
	"mycoplasma": "pleomorph",
	"carsonella": "symbiont",
	"syn3": "symbiont",
}


## The morphotype for a species: the key table first (mirrors glyph_factory.morph_for), then a role fallback so
## an un-tabled key still draws SOMETHING (graceful-degrade — the same discipline as the codex/glyph factory).
static func morph_for(key: String, role: String) -> String:
	if MORPH_BY_KEY.has(key):
		return MORPH_BY_KEY[key]
	match role.to_lower():
		"autotroph":
			return "plant"
		"symbiont", "obligatesymbiont":  # the core TrophicRole debug string is "ObligateSymbiont"
			return "symbiont"
		"predator":
			return "vibrioid"
		_:
			return "rod"


## Size multiplier (× the cell base radius) for a species key/role. Unknown → SIZE_DEFAULT (never a crash).
static func size_for(key: String, role: String) -> float:
	match morph_for(key, role):
		"plant": return SIZE_PLANT
		"mold": return SIZE_MOLD
		"rod": return SIZE_ROD
		"cocci": return SIZE_COCCUS
		"pleomorph": return SIZE_PLEOMORPH
		"vibrioid": return SIZE_VIBRIOID
		"symbiont": return SIZE_SYMBIONT
		_: return SIZE_DEFAULT


## Base color for a species key/role. Unknown → COLOR_DEFAULT (never a crash).
static func color_for(key: String, role: String) -> Color:
	match morph_for(key, role):
		"plant": return COLOR_PLANT
		"mold": return COLOR_MOLD
		"rod": return COLOR_ROD
		"cocci": return COLOR_COCCUS
		"pleomorph": return COLOR_PLEOMORPH
		"vibrioid": return COLOR_VIBRIOID
		"symbiont": return COLOR_SYMBIONT
		_: return COLOR_DEFAULT


## Whether the morphotype draws as a large plant canopy (vs a microbe blob/rod). Lets organisms.gd keep the
## existing plant L-sprite for autotrophs while sizing microbes down — a single presentation routing flag.
static func is_plant(key: String, role: String) -> bool:
	return morph_for(key, role) == "plant"


## Build the per-species-id → {size, color, is_plant} visual lookup the field draw indexes by dominant id.
## `id_to_meta` is {species_id:int -> {key, role}} (assembled in main.gd from observe_species()); this resolves
## each to its visual once so the hot per-cell loop is a plain array/dict read. Graceful: a species id absent
## from the map (or an empty map, e.g. file-replay) is simply not present → the draw falls back to the default.
static func build_table(id_to_meta: Dictionary) -> Dictionary:
	var table: Dictionary = {}
	for sid in id_to_meta:
		var meta: Dictionary = id_to_meta[sid]
		var key := str(meta.get("key", "default"))
		var role := str(meta.get("role", ""))
		table[int(sid)] = {
			"size": size_for(key, role),
			"color": color_for(key, role),
			"is_plant": is_plant(key, role),
		}
	return table
