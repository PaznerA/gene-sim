extends RefCounted
## Evidence-driven, KEY-LED, trait-driven glyph factory (SP-4): one dispatcher mapping a species (key + role +
## trait scalars) to a built specimen glyph. It REPLACES the binary `if _is_microbe_key(...)` branch in
## main.gd::_render_specimens — every glyph it returns honours the existing Node2D + build(Dictionary) +
## bounds()->Rect2 contract, so the row/label/focus/framing machinery is untouched.
##
## INVARIANT #2 (STOP THE LINE if violated): PURE PRESENTATION. trait scalars + role + key → pixels, no biology.
## The genome→phenotype map ran in the Rust core; this maps already-expressed scalars to visual params (the
## per-morphotype param builders below) — exactly the role lsystem.gd/microbe.gd/mold.gd play.
##
## INVARIANT #3: all geometry is precomputed in the glyph's build() (so --check catches a malformed polygon);
## deterministic per-index _hash01 jitter inside each glyph, no global RNG.
##
## DISPATCH IS KEY-LED, ROLE-FALLBACK (verified against the real species JSONs): role ALONE is insufficient —
## FIVE species are role `decomposer` (ecoli-core, bacillus, cutibacterium, aspergillus-niger, penicillium) yet
## split into rod / spore-former / mold, and heterotroph covers both staph (cocci) and mycoplasma (wall-less).
## So a `MORPH_BY_KEY` table decides the morphotype per baked key; an unknown key falls back to its role so a
## future baked species draws SOMETHING with zero code change (the same graceful-degrade discipline as the codex).

const Lsystem := preload("res://lsystem.gd")
const Microbe := preload("res://microbe.gd")
const Mold := preload("res://mold.gd")

# Morphotypes.
const PLANT := "plant"
const ROD := "rod"
const VIBRIOID := "vibrioid"
const COCCI := "cocci"
const MOLD := "mold"
const PLEOMORPH := "pleomorph"  # wall-less mycoplasma
const SYMBIONT := "symbiont"  # tiny coccoid speck

## The 12 baked species → morphotype (the decisive table; role is fallback only). Ordered as a const dict; we
## read it by key (a single lookup, no iteration of its key order in sim logic — inv #3 is about ordered
## ITERATION, and we never iterate this for anything order-sensitive).
const MORPH_BY_KEY := {
	"default": PLANT,
	"ecoli-core": ROD,
	"bdellovibrio": VIBRIOID,
	"staph": COCCI,
	"cutibacterium": ROD,  # short, non-motile rod
	"pseudomonas": ROD,  # rod + polar flagella + biofilm halo
	"bacillus": ROD,  # spore-former (endospore drawn when SporulationCapacity > 0)
	"aspergillus-niger": MOLD,
	"penicillium": MOLD,
	"mycoplasma": PLEOMORPH,  # wall-less
	"carsonella": SYMBIONT,  # speck
	"syn3": SYMBIONT,  # speck
}


## Build a glyph Node2D for one specimen. `traits` is the snake_case trait dict; `spec` is the full specimen
## entry ({label,traits,key,...}); `idx` is the row index (the deterministic jitter seed). Returns a Node2D
## whose build() has already been called — ready to add as a child.
static func make(key: String, role: String, traits: Dictionary, spec: Dictionary, idx: int) -> Node2D:
	var morph := morph_for(key, role)
	match morph:
		ROD:
			var g := Microbe.new()
			g.build(_rod_params(key, traits, idx))
			return g
		VIBRIOID:
			var g := Microbe.new()
			g.build(_vibrioid_params(traits, idx))
			return g
		COCCI:
			var g := Microbe.new()
			g.build(_cocci_params(traits, idx))
			return g
		PLEOMORPH:
			var g := Microbe.new()
			g.build(_pleomorph_params(traits, idx))
			return g
		SYMBIONT:
			var g := Microbe.new()
			g.build(_symbiont_params(traits, spec, idx))
			return g
		MOLD:
			var g := Mold.new()
			g.build(_mold_params(key, traits, idx))
			return g
		_:  # PLANT (default)
			var g := Lsystem.new()
			g.build(plant_params(traits, idx))
			return g


## The morphotype for a species: the key table first, then role fallback (graceful for an un-tabled key).
static func morph_for(key: String, role: String) -> String:
	if MORPH_BY_KEY.has(key):
		return MORPH_BY_KEY[key]
	match role.to_lower():
		"autotroph":
			return PLANT
		"symbiont":
			return SYMBIONT
		"predator":
			return VIBRIOID
		_:
			return ROD


## The per-morphotype emoji (chrome glyph) for a species — keyed first, then role fallback. Mirrors morph_for.
static func emoji_for(key: String, role: String) -> String:
	match morph_for(key, role):
		ROD, VIBRIOID, COCCI:
			return "🦠"
		MOLD:
			return "🍄"
		PLEOMORPH:
			return "🫧"
		SYMBIONT:
			return "🔬"
		_:
			return "🌱"


# ──────────────────────────── per-morphotype param builders (presentation only, inv #2) ────────────────────────────

## L-system plant params (was main.gd::_plant_params_from_traits — moved here verbatim, hash-irrelevant).
static func plant_params(t: Dictionary, seed_val: int) -> Dictionary:
	var growth := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
	var stature := clampf(float(t.get("stature", 0.5)), 0.0, 1.0)
	var branchy := clampf(float(t.get("branchiness", 0.5)), 0.0, 1.0)
	var leaf := clampf(float(t.get("leaf_size", 0.5)), 0.0, 1.0)
	var hue := clampf(float(t.get("leaf_hue", 0.5)), 0.0, 1.0)
	var refl := clampf(float(t.get("reflectance", 0.5)), 0.0, 1.0)
	var fec := clampf(float(t.get("fecundity", 0.5)), 0.0, 1.0)
	var drought := clampf(float(t.get("drought_tolerance", 0.5)), 0.0, 1.0)
	var ksl := clampf(float(t.get("kill_switch_linkage", 0.0)), 0.0, 1.0)
	var leaf_hsv := Color.from_hsv(0.18 + hue * 0.30, 0.55 + drought * 0.25, 0.55 + refl * 0.35)
	return {
		"iterations": 3 + int(round(branchy * 3.0)),
		"angle_deg": 14.0 + branchy * 34.0,
		"segment_len": 5.0 + stature * 10.0,
		"len_falloff": 0.78 + drought * 0.16,
		"thickness": 2.5 + growth * 4.0,
		"leaf_size": 1.5 + leaf * 7.0,
		"leaf_aspect": 0.42 + drought * 0.30,
		"jitter_deg": 2.0 + ksl * 11.0,
		"seed": seed_val,
		"flower_count": int(round(fec * 5.0)),
		"petal_count": 4 + int(round(fec * 4.0)),
		"branch_base": Color(0.34, 0.23, 0.12).lerp(Color(0.45, 0.34, 0.18), growth),
		"branch_tip": Color(0.30, 0.50, 0.20).lerp(Color(0.64, 0.60, 0.22), drought),
		"leaf_color": leaf_hsv,
		"flower_color": Color(0.95, 0.45, 0.55).lerp(Color(0.98, 0.85, 0.35), hue),
		"leaf_sheen": refl,
		"kill_marker": ksl,
	}


## Rod params (was main.gd::_microbe_params_from_traits, now per-key). E. coli = peritrichous rod; cutibacterium
## = short non-motile rod; pseudomonas = rod + polar flagella + biofilm halo; bacillus = rod + endospore.
static func _rod_params(key: String, t: Dictionary, seed_val: int) -> Dictionary:
	var growth := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
	var glucose := clampf(float(t.get("glucose_uptake", 0.5)), 0.0, 1.0)
	var respiration := clampf(float(t.get("respiration_mode", 0.5)), 0.0, 1.0)
	var acetate := clampf(float(t.get("acetate_overflow", 0.0)), 0.0, 1.0)
	var ferment := clampf(float(t.get("fermentation_capacity", 0.0)), 0.0, 1.0)
	var spore := clampf(float(t.get("sporulation_capacity", 0.0)), 0.0, 1.0)
	var aerobic := Color(0.42, 0.78, 0.80)
	var ferment_tint := Color(0.86, 0.66, 0.34)
	var body := aerobic.lerp(ferment_tint, respiration).lerp(Color(0.90, 0.42, 0.30), acetate * 0.6)
	var p := {
		"shape": "rod",
		"length": 56.0 + growth * 86.0,
		"width": 24.0 + glucose * 26.0,
		"septum_pinch": clampf((growth - 0.7) / 0.3, 0.0, 1.0),
		"respiration": respiration,
		"flagella_layout": "peritrichous",
		"flagella_count": 2 + int(round(glucose * 4.0)),
		"flagella_len": 44.0 + glucose * 46.0,
		"granule_count": int(round(ferment * 12.0)),
		"halo_count": int(round(acetate * 14.0)),
		"seed": seed_val,
		"body_color": body,
		"outline_color": Color(0.92, 0.97, 0.99, 0.9),
		"granule_color": Color(0.97, 0.88, 0.46, 0.9),
		"halo_color": Color(0.93, 0.58, 0.30, 0.6),
	}
	match key:
		"cutibacterium":
			p["length"] = 40.0 + growth * 36.0  # short rod
			p["flagella_layout"] = "none"  # non-motile
			p["body_color"] = body.darkened(0.18)  # dim cytoplasm
		"pseudomonas":
			p["flagella_layout"] = "polar"  # 1-2 polar flagella
			p["flagella_count"] = 1 + int(round(glucose))
			p["biofilm"] = 0.55 + 0.45 * glucose  # translucent biofilm-matrix halo
		"bacillus":
			# Spore-former: draw the endospore when SporulationCapacity is non-zero (spore-CAPABLE, graceful).
			p["endospore"] = spore if spore > 0.0 else 0.6  # gate on the trait; default visible so it reads spore-capable
			p["flagella_layout"] = "peritrichous"
	return p


## Vibrioid params (Bdellovibrio): a COMMA — a bent capsule + one long thick sheathed polar flagellum. The
## ~160 µm/s attack morphology; PredationCapacity drives the flagellum vigour (length).
static func _vibrioid_params(t: Dictionary, seed_val: int) -> Dictionary:
	var growth := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
	var predation := clampf(float(t.get("predation_capacity", 0.5)), 0.0, 1.0)
	var respiration := clampf(float(t.get("respiration_mode", 0.3)), 0.0, 1.0)
	return {
		"shape": "vibrioid",
		"length": 50.0 + growth * 40.0,
		"width": 18.0 + growth * 8.0,
		"curvature": 0.5 + 0.5 * predation,  # bend the spine into a comma
		"respiration": respiration,
		"flagella_layout": "polar",
		"flagella_count": 1,  # forced single sheathed polar flagellum
		"flagella_len": 60.0 + predation * 70.0,  # PredationCapacity → flagellum vigour (the attack speed)
		"seed": seed_val,
		"body_color": Color(0.55, 0.45, 0.82).lerp(Color(0.85, 0.35, 0.45), predation * 0.5),
		"outline_color": Color(0.92, 0.90, 0.99, 0.9),
	}


## Cocci params (staph): a grape-cluster of daughter spheres, no flagella (non-motile).
static func _cocci_params(t: Dictionary, seed_val: int) -> Dictionary:
	var growth := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
	return {
		"shape": "coccus",
		"length": 30.0 + growth * 16.0,  # (unused for spacing of cluster; sets radius scale)
		"width": 30.0 + growth * 16.0,
		"flagella_layout": "none",
		"flagella_count": 0,
		"seed": seed_val,
		"body_color": Color(0.86, 0.78, 0.42).lerp(Color(0.95, 0.86, 0.40), growth),  # golden (staphyloxanthin)
		"outline_color": Color(0.99, 0.96, 0.85, 0.9),
	}


## Pleomorph params (mycoplasma, wall-less): a small irregular soft blob, no crisp ring, no flagella.
static func _pleomorph_params(t: Dictionary, seed_val: int) -> Dictionary:
	var growth := clampf(float(t.get("growth_rate", 0.4)), 0.0, 1.0)
	return {
		"shape": "wall_less",
		"length": 26.0 + growth * 16.0,
		"width": 26.0 + growth * 16.0,
		"flagella_layout": "none",
		"flagella_count": 0,
		"scale": 0.85,  # small — the wall-less smallest-genome story
		"seed": seed_val,
		"body_color": Color(0.74, 0.66, 0.82, 0.85),
		"outline_color": Color(0.85, 0.80, 0.92, 0.5),
	}


## Symbiont params (carsonella/syn3): the SMALLEST glyph — a tiny coccoid speck at reduced scale with a faint
## host-containment ring + a SymbiosisCapacity-driven host-coupling tether. Size scales DOWN with loci count
## (read from the widened export via `spec.loci_count` when present) — the small size IS the genome-reduction story.
static func _symbiont_params(t: Dictionary, spec: Dictionary, seed_val: int) -> Dictionary:
	var symbiosis := clampf(float(t.get("symbiosis_capacity", 0.5)), 0.0, 1.0)
	var growth := clampf(float(t.get("growth_rate", 0.4)), 0.0, 1.0)
	# Genome-reduction: scale DOWN with loci count. Carsonella ~16 loci in the baked set → small. Clamp generous.
	var loci_count := int(spec.get("loci_count", 16))
	var reduce := clampf(float(loci_count) / 200.0, 0.05, 1.0)  # fewer loci → smaller
	return {
		"shape": "coccus",
		"length": 16.0 + growth * 8.0,
		"width": 16.0 + growth * 8.0,
		"flagella_layout": "none",
		"flagella_count": 0,
		"scale": 0.45 + 0.4 * reduce,  # deliberately small speck
		"host_ring": 0.7,  # lives inside a bacteriocyte
		"host_tether": symbiosis,  # SymbiosisCapacity host-coupling tether
		"seed": seed_val,
		"body_color": Color(0.78, 0.70, 0.55),
		"outline_color": Color(0.90, 0.86, 0.70, 0.7),
	}


## Mold params (aspergillus-niger / penicillium): hyphal mycelium + conidiophore; the head form follows the key,
## the conidia-chain density follows SporulationCapacity (the brlA→abaA→wetA cascade).
static func _mold_params(key: String, t: Dictionary, seed_val: int) -> Dictionary:
	var growth := clampf(float(t.get("growth_rate", 0.5)), 0.0, 1.0)
	var spore := clampf(float(t.get("sporulation_capacity", 0.6)), 0.0, 1.0)
	# If SporulationCapacity isn't exported (the species defaults to the plant map), keep a sensible default so a
	# mold still reads as a mold — graceful, never bare.
	var density := spore if spore > 0.0 else 0.6
	var head := "penicillium" if key == "penicillium" else "aspergillus"
	var conidia_color := Color(0.30, 0.45, 0.55, 0.95) if head == "penicillium" else Color(0.16, 0.14, 0.16, 0.95)
	return {
		"head": head,
		"hyphae_count": 3 + int(round(growth * 4.0)),  # growth_rate → mycelium extent
		"hyphae_len": 44.0 + growth * 40.0,
		"conidia_density": density,
		"conidia_color": conidia_color,
		"vesicle_color": Color(0.40, 0.36, 0.34),
		"stalk_color": Color(0.62, 0.58, 0.40),
		"seed": seed_val,
	}
