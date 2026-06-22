# Contamination & Immigration — the CLEAN-ROOM epic (DRAFT)

> **DRAFT — research synthesis + implementation proposal. Design only, no code; awaiting human keystone
> sign-off (the containment-knob policy + the contaminant data bakes touch the pinned config and a re-pin
> decision).** Builds directly on the accepted **ADR-013 ecology substrate** (the conserved fixed-point
> joule ledger) and the **SP-3 intervention panel** (journaled, RNG-free, conserved region Actions). This
> is the SP-3-deferred **"seed / inoculate"** tool (`sp3-intervention-panel-impl.js:38` — "seed … a deferred
> 2nd wave"), promoted into its own epic. Proposed number: **ADR-019** (next free accepted; the ledger has
> 001–014, 017, 018).

---

## 1. THE FRAME — contamination is the default state of reality

The premise of the epic, and the one-line teaching beat: **a sterile world is the expensive, artificial,
constantly-decaying exception; contamination is the baseline.** You do not "add" contaminants to a clean
culture — you spend energy holding back a universe that is already full of spores, skin flora, and mold,
and the moment your guard drops, reality floods back in. This is the clean-room metaphor as *emergent
gameplay*: the player is an operator running a culture inside a containment envelope, and the drama is the
permanent, evidence-grounded tension between immigration pressure and the resident community's hold on the
niche.

Why this is true biology, not flavor (verified):

- **Air is full of propagules.** Every human inhales *several hundred* *Aspergillus fumigatus* conidia per
  day, normally cleared by the immune system [Nierman et al. 2005, Nature 438:1151; Wikipedia/A. fumigatus
  cross-checked]. *Penicillium* is the single most common mold genus in indoor **and** outdoor air samples
  [van den Berg et al. 2008, Nat Biotechnol 26:1161]. An undisturbed compost pile emits ~8–11×10³ cfu/m²/s
  of *A. fumigatus* conidia at 1 m/s wind.
- **Sterility barriers leak by design.** *Mycoplasma genitalium* has **no cell wall** (class Mollicutes),
  so it deforms through the 0.22 µm filters used to sterilize media **and** is intrinsically resistant to
  cell-wall antibiotics (penicillin/β-lactams). It reaches 10⁸ organisms/mL *without clouding the medium* —
  cryptic, chronic contamination; **15–35 % of continuous mammalian cell lines are estimated to be
  mycoplasma-contaminated** [PMC10668599, cell-culture mycoplasma review].
- **The cleanest rooms on Earth are not sterile.** Of 130 floor isolates over 6 months in NASA JPL's
  spacecraft-assembly cleanroom (Mars 2020 / Perseverance), **97 % were spore-formers** — *Bacillus*,
  *Geobacillus*, *Sphingomonas*, *Acinetobacter*; an *Aspergillus fumigatus* was pulled off an ISS HEPA
  filter [PMC8643001; Sci Rep s41598-019-50782-0].
- **The default failure mode of an unsterile plate is takeover by mold.** A single landed spore germinates,
  hyphae outgrow even Gram-negative bacteria (*A. niger* grows faster than *E. coli* on EMB and suppresses
  it by acidifying the medium [bsmiab.org/jabet 178-1634619354]), sporulates, and visibly takes the plate.
  "Lab weeds."

So the epic's frame is the verified default: **lower your guard and the consortium that flies in wins by
default unless your residents already hold the niche.** That is exactly the dynamic ADR-013's conserved
joule economy already produces — a poorly-adapted immigrant *starves* (its J integral hits zero → death →
detritus), a well-adapted one *invades* (out-harvests the resident until the resident's J integral hits
zero → competitive exclusion). We do not script "establish/displace/die"; we let the existing ledger decide
it. Contamination becomes the first system where the player feels the chemostat as an *adversary*.

---

## 2. THE MECHANIC — deterministic, journaled IMMIGRATION / INOCULATION events

### 2.1 What an immigration event is

A single new journaled region Action — the SP-3-deferred **seed/inoculate** tool — drops a **consortium**
(an ordered menu set of contaminant `SpeciesSpec`s) onto the substrate. It is the exact sibling of SP-3's
`RegionPcrAmplify`: RNG-free (or single documented stream draw), region-scoped (a brush disc reusing
`RegionSpec`, `harness/src/lib.rs:175`), integer, **conserved via a NAMED influx ledger tap**, and journaled
into `actions.ndjson` so a contaminated run replays bit-identically.

Proposed Action (externally-tagged serde-additive, existing `actions.ndjson` unchanged — the SP-3 precedent):

```
/// Inoculate a contaminant SpeciesSpec at a region. Spawns `count` organisms of `species_key` inside
/// the region disc; their starting J is minted from a NAMED influx tap (`immigration`), conserved.
/// RNG-free placement (deterministic cell fill in (cell_index, slot) order; OrgIds from NextOrgId).
RegionInoculate { species_key: String, region: RegionSpec, count: u32, endow_j: i64 }
```

This reuses the *entire* SP-3 spine:

- **Spawn path** = SP-3's faithful-PCR-clone path, generalized: instead of cloning a *resident's* exact
  genome, it instantiates a *baked* `SpeciesSpec` (built via `SpeciesSpec::build`, `genome/src/spec.rs:163`)
  and spawns `count` organisms in the region, OrgIds from `NextOrgId`, J minted from the influx tap.
- **Conservation** = a NAMED ledger tap (`immigration`), identical in kind to SP-3's "PCR/intervention
  influx" tap. `ledger_closes` must hold every tick through every inoculation (the immigrants' J is
  accounted as influx, never conjured).
- **Determinism** = RNG-free, region-scoped, ordered (sort by `(cell_index, SpeciesId, OrgId)` — the
  ADR-013 binding ordering contract), journaled. Replay reproduces the exact contaminated run.
- **Establish / displace / die-out is NOT coded here.** It *emerges* from ADR-013's metabolism →
  trophic_transfer → reproduce_or_die pipeline. The immigrant either harvests enough J to fund offspring
  (establishes), out-harvests the resident until the resident starves (displaces), or fails to cover
  maintenance and its sub-population integrates to zero (dies out). All three are already-reachable ADR-013
  outcomes; this epic only supplies the *arrivals*.

### 2.2 Why this is HASH-NEUTRAL (no re-pin) for the immigration system itself

By the exact SP-3 argument (`sp3-intervention-panel-impl.js:26`): **the new Action is inert until invoked.**
The pinned single-species-plant config issues no `RegionInoculate`, the `immigration` ledger tap is a new
field that is neutral at zero, and the contaminant `SpeciesSpec` JSON files sit unused on disk (serde-default
`niche.trophic_role`, `genome/src/spec.rs:53`, makes them byte-neutral for the roster). Therefore the pinned
literal **`0x47a0_3c8f_6701_f240` stays unchanged** for the event system + the consortium config + the
containment knob (with the knob defaulted OFF; see §3). The data bakes (§5) are also hash-neutral until a
config references them. **If the implementer finds the literal would move, STOP and report — that is a
re-pin and a separate sign-off.** (The only re-pin in the whole epic is the *optional* spore/dormancy
sub-state of §5.4, called out explicitly.)

### 2.3 The keystone-granularity check (inv #6)

Immigration is an **operator/species-level** event: the player (or a deterministic schedule) seeds a
*species* at a *region* — never a per-organism RL action. The contaminant's behavior once it lands is pure
ADR-013 biology (metabolism/trophic/reproduce), not an agent. This sits exactly at the granularity ceiling,
identical to SP-3's region tools.

---

## 3. THE CONTAINMENT / STERILITY KNOB — contamination pressure as one deterministic scalar

A single sandbox parameter, `ContainmentLevel`, sets **contamination pressure**: the frequency, size, and
diversity of immigration events. Dirtier (lower containment) → more pressure; the player counters with the
SP-3 cull/antibiotic/toxin tools **and** by keeping their own resident consortium holding the niche.

### 3.1 Grounding the ladder (verified)

The knob is an explicit, citable ladder, not an arbitrary slider — **ISO 14644-1:2015** air-cleanliness
classes (max particles/m³) [theccnetwork ISO 14644-1 guide; 14644.dk; ISO 53394], with the EU GMP grade and
FED-209E equivalences [gmpinsiders]:

| Knob level | Class | Equivalence | Contamination pressure (immigration schedule) |
|---|---|---|---|
| **Sealed** | ISO 5 | Class 100 / GMP Grade A | near-zero: events extremely rare, size 1, diversity 1 |
| **Clean** | ISO 7 | Class 10 000 / Grade C | sparse: low frequency, small propagules |
| **Lab** | ISO 8 | Class 100 000 / Grade D | frequent: the realistic "open bench" default |
| **Open** | ISO 9 / room air | unclassified | constant flood: the "lab weeds take the plate" mode |

Orthogonally, a **BSL-style hazard tag** (CDC/NIH BSL-1→4) can ride on each contaminant `SpeciesSpec` as
codex metadata (§5) — flavor + teaching, not a sim input.

### 3.2 Determinism — the knob SEEDS a schedule, never a wall clock

The critical rule (inv #3): **`ContainmentLevel` deterministically derives an event schedule off a
dedicated off-stream seed family** (`IMMG_STREAM_BASE`, an ASCII-tagged `derive_seed` family in the
ADR-013 OFF-STREAM SEED REGISTRY tradition — *zero* `SimRng` draws, so introduction is spawn-draw-order
neutral and independently attributable). The schedule is a pure function of `(master_seed,
ContainmentLevel, consortium_config)`: it expands at run start into a fixed, ordered list of `(generation,
species_key, region, count)` `RegionInoculate` events that are **journaled like any operator action**.
Replay reproduces the schedule bit-for-bit. No wall-clock, no thread-local RNG, no `HashMap` iteration —
the schedule list is a sorted `Vec`.

So the knob is a single deterministic input that *generates journaled events*; the events are the same
`RegionInoculate` the player can fire by hand. **Default `ContainmentLevel = Sealed` (or "Off")** → schedule
is empty → the pinned config issues no events → hash-neutral.

### 3.3 The counter-play loop (why it is a game, grounded)

The verified invasion-ecology result that makes this a *strategy* loop, not a slider: in microbial systems
**biotic interactions dominate propagule pressure.** Albright & Martiny (mBio 2020, PMC7593967) found
biotic interactions explained ~7× more variance in community function and ~40× more in composition than
propagule pressure; dose/frequency explained only 1.2–12 %; successful invaders were ~8 % of OTUs and
carried stress/biofilm/antimicrobial traits. Propagule *number* matters most against a *strong competitor*;
invasion *timing* matters most when the resident grows slowly — **priority effects** [Jones & Lennon 2017,
Ecology ecy.1852; Fukami 2015; Debray et al. 2021, Nat Rev Microbiol].

The clean design consequence, and it maps straight onto ADR-013: **the resident community's hold on the
niche (its standing J harvest) is the primary determinant of whether an immigrant establishes; the knob's
propagule pressure is secondary.** A player who keeps a dense, well-adapted resident consortium *holding the
cell pools* needs little containment; a player who lets residents thin out gets invaded even at moderate
pressure. This is colonization resistance, and it is exactly what the conserved-pool contention in ADR-013
already computes (the second demander by OrgId gets the *remainder* of a depleted pool). The defended
community is a **defined consortium** whose resistance is an emergent, keystone-dependent property, not a
per-organism stat — the verified SynCom / colonization-resistance frame: a 4-strain consortium restored
resistance to *C. difficile* in antibiotic-treated mice, with *Clostridium scindens* the keystone (its
7α-dehydroxylation of bile acids inhibits *C. difficile*) [Buffie et al. 2015, Nature 13828 / PMC4354891],
now an FDA-approved class of live biotherapeutics (VOWST/SER-109, REBYOTA/RBX2660 [Nature d41573-023-00081-1]).

Counter-play uses the **already-shipped SP-3 tools** — `RegionCull` (antibiotic), `RegionToxin`, plus the
resident's own defended niche-hold. The dramatic loop the biology forces (verified, §5.4): **a cull alone
fails against spore/biofilm contaminants — the dormant reservoir reseeds — unless the niche is also closed
by a holding consortium.** That tension is the heart of the mode.

---

## 4. THE TWO MODES — the verified biology split

The research is unambiguous that there are **two mechanistically distinct kinds of "new arrival,"** and they
must NOT share one rule. This split is the load-bearing design decision of the epic.

### Mode A — AIRBORNE CONTAMINANTS (free-living invaders)

Arrive **uninvited** from an environmental reservoir (air/dust/water/skin) onto any open cell, gated by the
`ContainmentLevel` knob, killable by a generic cull. Each is a baked `SpeciesSpec` (genome + trophic role).
These are the `RegionInoculate` default-spawn organisms. Six taxa, each defeating a *different* sterility
barrier (a natural set of distinct immigration vectors + distinct cull-resistances):

| Species | Defeats the barrier of… | Trophic role (existing seam) | Verified mechanism |
|---|---|---|---|
| ***Mycoplasma genitalium*** | filtration + cell-wall antibiotics | Heterotroph (host/serum-dependent parasite) | no cell wall → passes 0.22 µm, β-lactam-resistant; cryptic 10⁸/mL non-turbid [PMC10668599] |
| ***Bacillus subtilis*** | heat / desiccation / UV / radiation | Decomposer (generalist saprophyte) | **endospore** (CaDPA ~25 % core dry wt, multilayer coat) → dormant-but-viable years–millennia [PMC99004; PLoS ONE 0208425, 500-yr expt] |
| ***Pseudomonas aeruginosa*** | nutrient-starvation + disinfectant | Heterotroph/Mixotroph (oligotroph generalist + **biofilm**) | grows in distilled water; most regulatory genes of any genome then sequenced; EPS biofilm [Stover et al. 2000, Nature 35023079; PMC11504098] |
| ***Staphylococcus epidermidis*** | the human operator | Heterotroph (skin commensal + biofilm) | dominant normal skin flora → the most easily operator-introduced contaminant; #1 device biofilm [PMC2777538] |
| ***Cutibacterium acnes*** | antiseptic prep + aerobic detection | Decomposer (lipophilic anaerobe) | aerotolerant anaerobe; slow (>5 d); sebum dampens disinfectants → survives antiseptic prep [PMC9250478; PMC10891977] |
| ***Aspergillus / Penicillium*** (mold) | nearly everything | Decomposer/Saprotroph (osmotrophic, **fast hyphal**) | prolific airborne melanized conidia; outgrows + acidifies out bacteria; thermotolerant (*A. fumigatus* to 50 °C) [Nierman 2005; Pel 2007; van den Berg 2008] |

All six map cleanly onto the **existing `gp::TrophicRole` seam** `{Autotroph, Heterotroph, Mixotroph,
Decomposer, Predator}` (`sim-core/src/gp.rs:291`) — **no new role enum variant is required for Mode A.**
The dominant niche is **Decomposer** (consumes the detritus pool → free_nutrient, the same flow class E.
coli already occupies), with the parasites/skin-flora as Heterotroph and the oligotroph as Mixotroph.
Resolution rides the *existing data-driven override*: each contaminant JSON sets `niche.trophic_role`
(`genome/src/spec.rs:50`), resolved at the boundary by `gp::role_from_override` (`gp.rs:358`) — exactly how
`ecoli.json` and `bdellovibrio.json` already declare their roles.

### Mode B — SYMBIONTS / MINIMAL GENOMES (host-dependent life)

The **opposite** of an airborne contaminant, and the verified reason it cannot share the rule: these are
inherited *vertically inside host cells* (bacteriocytes), have shed the genes for free life, and **a
sterility/airflow knob does nothing to them.** They establish ONLY by being seeded into a compatible host
and a generic cull does NOT clear them (you would have to remove the host).

| Species | Genome (verified) | Why it CANNOT airborne-contaminate |
|---|---|---|
| ***Carsonella ruddii*** | **159,662 bp**, 182 ORFs, 16.5 % GC, 97.3 % coding [Nakabachi et al. 2006, Science 1134196] | lost cell-envelope, nucleotide, lipid, much DNA-repair machinery; "insufficient for most processes essential for bacterial life"; argued a sub-cellular entity *between cell and organelle* — no environmental phase, no free metabolism |
| ***Hodgkinia cicadicola*** | ancestral ~144 kb / ~150 genes; **splits** into co-dependent lineages (Magicicada complex ~1.1 Mb across ≥17 circular molecules) [Van Leuven/McCutcheon 2014, PNAS 4547289] | genome fragments into mutually-dependent lineages with *complementary* gene loss — the metabolic unit is the whole consortium; no single lineage is viable, none can be separated or culled without killing the host |
| ***JCVI-Syn3.0*** | **531,560 bp**, 473 genes (149 unknown function) [Hutchison/Gibson et al. 2016, Science aad6253] | smallest autonomously-replicating cell, but grows ONLY in rich defined lab medium — "does little else" beyond replicate/transcribe/translate/divide; dies in any minimal/natural environment → not a wild colonizer |

**Design rule:** Mode B is a *separate* host-dependence / minimal-life mode, gated behind its own design
phase. Its seeding is an **`RegionInoculate` into a compatible host entity**, NOT onto an open cell; the
`ContainmentLevel` knob and a region cull do **not** govern it. This needs a new interaction class that the
**existing 5-role enum does not express** — an *Obligate Symbiont* whose `FlowMatrix` edge is
host↔symbiont (the symbiont draws host J and returns an essential-nutrient flux), with a per-species
**cull-immune-at-the-environment-layer** flag. That is a genuine ADR-013-substrate extension (a new
TrophicRole variant + a host-coupling edge in the InteractionKernel) and therefore a **later phase with its
own sign-off** — explicitly NOT in the hash-neutral first wave. Hodgkinia's complementary lineage-splitting
("you cannot cull or separate one without killing the host") is flagged as a *future* unique mechanic, not
a v1 commitment.

The **bridge species** worth noting: *M. genitalium* is BOTH a Mode-A contaminant (as a host-dependent
parasite, 580,070 bp / ~470–485 genes [Fraser et al. 1995, Science 270:397; Glass et al. 2006, PNAS
16407165: 482 protein genes, 382 essential]) AND the genome template for the Mode-B minimal cell (Syn3.0
lineage). It can carry a single baked genome reused in both modes.

---

## 5. DATA PLAN — the contaminant `SpeciesSpec` bake

### 5.1 Bake plan (real NCBI genomes where feasible, exactly like `ecoli.json` / `bdellovibrio.json`)

Each contaminant ships as `data/species/<key>.json` built by the validated `SpeciesSpec::build` path
(`genome/src/spec.rs`), curated CDS from the public-domain NCBI reference assembly (the `bdellovibrio.json`
precedent: "Curated … roster × CDS GCF… (NCBI, public domain)"). The kernel reads **role + trait levers**,
not specific genes (the `bdellovibrio.json` note), so a *curated* roster of the mechanism loci suffices — we
do not need the whole genome inline. Citable genome facts (all primary-sourced) seed both the bake and the
SP-4 codex (§5.5):

| key | Reference assembly | Genome size / genes (verified) | `niche.trophic_role` |
|---|---|---|---|
| `mycoplasma` | *M. genitalium* G37 | 580,070 bp / ~470–485 genes | `heterotroph` |
| `bacillus` | *B. subtilis* 168, GCF_000009045.1 | 4,214,810 bp / ~4,100–4,300 CDS | `decomposer` |
| `pseudomonas` | *P. aeruginosa* PAO1 | 6,264,404 bp / 5,570 ORFs | `mixotroph` |
| `staph-epi` | *S. epidermidis* ATCC 12228 | 2,570,371 bp / 2,462 CDS, 32.08 % GC | `heterotroph` |
| `cutibacterium` | *C. acnes* KPA171202 | 2,560,265 bp / 2,333 ORFs, ~60 % GC | `decomposer` |
| `aspergillus-niger` | *A. niger* CBS 513.88 | 33.9 Mb / ~14,165 ORFs, 8 chr | `decomposer` |
| `penicillium` | *P. chrysogenum* Wis 54-1255 | 32.19 Mb / 12,943 genes | `decomposer` |
| *(Mode B, later)* `carsonella` | *Ca. C. ruddii* Pv | 159,662 bp / 182 ORFs | *new ObligateSymbiont role* |
| *(Mode B, later)* `syn3` | JCVI-Syn3.0 | 531,560 bp / 473 genes | *medium-dependent chassis* |

The **~10× genome-size spread** (0.58 → 6.3 Mb bacterial; 33 Mb fungal) is a ready axis for a future
genome-size-vs-niche-breadth trade-off, but v1 only needs the role + a few trait levers.

### 5.2 Trophic roles — reuse the existing seam, no new enum for Mode A

Mode A roles are all in the shipped `{Autotroph, Heterotroph, Mixotroph, Decomposer, Predator}` enum, set
per-species as DATA via `niche.trophic_role` + `gp::role_from_override` (the seam ADR-013 F4 already built
for `ecoli.json`/`bdellovibrio.json`). **Only Mode B needs a new role.**

### 5.3 Spore / biofilm / resistance traits → map onto EXISTING genome/trait seams

These differentiate the containment knob and the cull counter-play. They map onto the genome parameter +
ontology-tag levers the GP map already reads (`gp::express_strategy` keys off `OntologyTags`, `gp.rs:489`),
**not** new bespoke fields where avoidable:

- **Cull-resistance** = a per-species susceptibility scalar (a genome parameter / GO-tagged locus level,
  read like `bdellovibrio`'s `PredationCapacity` lever). High for spore/biofilm formers, low for
  vegetative cells. Differentiates `RegionCull` (antibiotic) per the verified profiles:
  *Mycoplasma* immune to a cell-wall-antibiotic cull (β-lactam-resistant; passes filters); *Pseudomonas*
  disinfectant + multi-antibiotic resistant; *Staph* biofilm-protected + constantly re-immigrating;
  *Cutibacterium* antiseptic-tolerant + slow.
- **Biofilm** = a defense/storage budget allocation in the existing `Strategy` budget simplex
  (`gp::BudgetChannel`) — biofilm formers spend budget on defense (resist cull) at the cost of growth,
  the structural trade-off ADR-013 F2 already encodes.
- **Oligotrophy / fast growth** = the existing uptake-affinity + growth budget channels (Pseudomonas
  oligotroph = high affinity at low pool J; mold = high growth = fast colony takeover).

These three reuse shipped seams → **hash-neutral** (inert until a config references a contaminant).

### 5.4 Spore / dormancy — the ONE optional re-pin

A true **dormant-spore sub-state** (a vegetative cull leaves a dormant reservoir that re-germinates →
"cull alone fails unless the niche is closed," the verified *Bacillus*/mold dynamic [PMC99004; the 97 %
spore-former cleanroom result PMC8643001]) is a *new per-organism state* and a new pass in the ADR-013
pipeline. That **moves the hash** and is therefore a **deliberate re-pin with its own sign-off**, NOT part
of the hash-neutral first wave. v1 approximates dormancy with the §5.3 cull-resistance scalar (a spore
former is simply hard to cull); the full germination sub-state is a roadmap follow-up.

### 5.5 SP-4 codex hooks (these genomes are famous — feed phenology/ontology/taxonomy)

Each contaminant is a vivid, evidence-anchored codex entry (the SP-4 codex draft consumes these): "you
inhale hundreds of *Aspergillus* spores every day"; "black mold" (*A. niger*, the citric-acid cell
factory); "the penicillin origin story" (*Penicillium*); "the silent invader that passes your filters"
(*Mycoplasma*); "the spore that survives 500 years / interplanetary transit" (*Bacillus*); "the minimal
cell — life with 473 genes, a third of unknown function" (Syn3.0); "the genome between cell and organelle"
(*Carsonella*); "the metabolism, not the cell, is the unit of selection" (*Hodgkinia*'s splitting). The
ISO/BSL containment ladder (§3.1) is itself a teaching panel; the FDA live-biotherapeutic anchors
(VOWST/REBYOTA) ground the "defended consortium" loop. Every fact in §5.1 carries a primary citation.

---

## 6. ADR DRAFT + SLICE PLAN

### 6.1 ADR-019 (DRAFT) — Contamination & Immigration: deterministic journaled inoculation + the containment knob

- **Status:** DRAFT, awaiting human sign-off. Builds on ADR-013 (joule ledger) + the SP-3 intervention
  panel (journaled region Actions); this is SP-3's deferred seed/inoculate 2nd wave.
- **Context:** ADR-013 already produces establish/displace/die from the conserved joule economy; the world
  has had no *arrivals* mechanism. Contamination is the verified default state of reality (§1).
- **Decision:** add ONE journaled, RNG-free, conserved region Action `RegionInoculate` (spawns a baked
  contaminant `SpeciesSpec` at a region, J from a NAMED `immigration` influx tap); a `ContainmentLevel`
  sandbox knob that deterministically derives a *journaled* event schedule off an off-stream
  `IMMG_STREAM_BASE` seed family (no wall-clock, no extra `SimRng` draws); a menu **consortium config**
  selecting which contaminant specs are in play; and a set of baked contaminant `SpeciesSpec`s (Mode A).
  Mode B (obligate symbionts / minimal genomes) is a separately-gated later phase requiring a new
  TrophicRole variant + host-coupling edge.
- **Determinism / hash:** the event system, consortium config, containment knob, and data bakes are
  **HASH-NEUTRAL** — the new Action is inert until invoked, the new tap is zero at rest, the knob defaults
  OFF (empty schedule), the JSONs sit unused (serde-default `trophic_role`). Pinned literal
  `0x47a0_3c8f_6701_f240` UNCHANGED. **The ONLY re-pins in the epic:** (a) the optional spore/dormancy
  sub-state (§5.4); (b) Mode B's new TrophicRole + host-edge. Each lands behind its own sign-off + ledgered
  re-pin per the ADR-011 procedure.
- **Invariants:** #2 — all biology stays in the core (contaminant genomes in `genome`/`sim-core`, GDScript
  only issues the Action + renders markers); #3 — RNG-free / single off-stream family, ordered, no
  `HashMap`; #6 — immigration is a species/region operator event, never per-organism; #1/#7 untouched.
- **Dependency:** the **SP-3 seed-tool plumbing must land first** (the `RegionInoculate` Action reuses the
  PCR-clone spawn path, the named influx tap, the region brush, and the timeline-marker UI). If SP-3 is not
  yet merged, this epic's S1 depends on it.

### 6.2 Slice plan (each leaves the gate green; hash-neutral unless flagged)

- **S0 — Contaminant data bakes (Mode A), hash-neutral.** Bake `mycoplasma`/`bacillus`/`pseudomonas`/
  `staph-epi`/`cutibacterium`/`aspergillus-niger`/`penicillium` `SpeciesSpec` JSONs (curated NCBI CDS +
  `niche.trophic_role`), each validating through `SpeciesSpec::build`. Touches: `data/species/*.json`,
  a build/round-trip test. Determinism: hash-neutral (unused on disk). Accept: each builds + round-trips;
  `role_from_override` resolves the declared role; pinned literal unchanged; gate green.
- **S1 — `RegionInoculate` Action + immigration influx tap, hash-neutral.** Add the Action (serde-additive),
  the deterministic region spawn (reuse SP-3 PCR-clone path, OrgIds from `NextOrgId`, J from the named
  `immigration` tap), journaled into replay. Touches: `crates/harness`, `crates/sim-core`. Determinism:
  hash-neutral (inert in pinned config). Accept: an inoculation conserves J + `ledger_closes` holds + is
  replay-reproducible; pinned config issues none → literal unchanged; gate green. **Depends on SP-3.**
- **S2 — `ContainmentLevel` knob + deterministic journaled schedule, hash-neutral.** The knob (ISO-ladder
  enum) expands at run start, off `IMMG_STREAM_BASE` (zero `SimRng` draws), into a sorted `Vec` of
  journaled `RegionInoculate` events; the consortium config selects the specs. Touches: `crates/harness`,
  `crates/sim-core`. Determinism: hash-neutral (default Sealed/Off → empty schedule). Accept: same
  seed+knob+config → identical schedule; replay reproduces it; default OFF → literal unchanged; gate green.
- **S3 — Renderer: contamination panel + immigration markers (read-only, inv #2), hash-neutral.** A
  containment-knob slider + consortium menu + a "seed" brush tool that issues `RegionInoculate`; timeline
  markers per immigration event (reuse SP-3's marker plumbing). GDScript issues the Action + renders; no
  biology. Touches: `godot/`, `crates/godot-sim` (read-only decode). Accept: markers render; no genome
  logic in GDScript; gate green (incl. headless `--check`).
- **S4 (RE-PIN, separate sign-off) — spore/dormancy sub-state.** The vegetative-cull-leaves-reservoir
  germination mechanic (§5.4). Touches: `crates/sim-core`. Determinism: 🔁 RE-PIN (new per-org state + pass).
  Accept: cull-then-regerminate emerges deterministically; ledger closes; re-pin ledgered; 🛑 sign-off.
- **S5 (RE-PIN, separate sign-off) — Mode B: obligate symbionts / minimal genomes.** New `ObligateSymbiont`
  TrophicRole variant + host-coupling `FlowMatrix` edge + cull-immune-at-environment flag + host-required
  `RegionInoculate` gating; bake `carsonella`/`syn3` (+ optional Hodgkinia splitting as a stretch). Touches:
  `crates/sim-core/src/gp.rs`, the InteractionKernel, `data/species`. Determinism: 🔁 RE-PIN. Accept: a
  symbiont establishes only into a compatible host, is cull-immune at the environment layer, host↔symbiont
  J flux measured in the FlowMatrix; ledger closes; re-pin ledgered; 🛑 sign-off.

### 6.3 What is hash-neutral vs a re-pin (summary)

- **HASH-NEUTRAL (no re-pin):** the immigration Action, the containment knob + deterministic schedule, the
  consortium config, the Mode-A data bakes, the renderer panel/markers. All inert in the pinned config;
  literal `0x47a0_3c8f_6701_f240` unchanged. (S0–S3.)
- **RE-PIN (own sign-off each):** the spore/dormancy sub-state (S4); Mode B's new TrophicRole + host edge
  (S5). (Both add per-org state / pipeline passes that move the hash.)

---

## Roadmap entry (paste into `docs/llm/TASKS.md`)

- [ ] 🧫 **Contamination & Immigration epic (ADR-019 DRAFT, needs sign-off)** — contamination as the default
  state of reality (the clean-room frame): one journaled, RNG-free, conserved `RegionInoculate` region Action
  (the SP-3-deferred seed/inoculate 2nd-wave tool) that drops a baked contaminant `SpeciesSpec` consortium
  onto the substrate with J minted from a named `immigration` influx tap, plus a `ContainmentLevel` sterility
  knob (ISO-14644 ladder) that deterministically derives a *journaled* immigration schedule off an off-stream
  `IMMG_STREAM_BASE` seed family (no wall-clock, no extra `SimRng` draws). Establish/displace/die-out emerges
  from the ADR-013 joule economy — nothing scripted; the player counters with the SP-3 cull/toxin tools + a
  resident consortium holding the niche (verified: biotic resistance dominates propagule pressure, mBio
  2020). **S0** bake the Mode-A contaminants (*Mycoplasma*/*Bacillus*/*Pseudomonas*/*Staph epi*/
  *Cutibacterium*/*Aspergillus*/*Penicillium*, real NCBI CDS, roles via `niche.trophic_role`) → **S1**
  `RegionInoculate` + influx tap (depends on the SP-3 seed-tool plumbing) → **S2** the containment-knob
  schedule → **S3** the renderer contamination panel + timeline markers (read-only, inv #2). **S0–S3 are
  HASH-NEUTRAL** (Action inert + tap zero + knob OFF + JSONs unused → literal `0x47a0_3c8f_6701_f240`
  unchanged). Two later RE-PIN phases, each its own 🛑 sign-off: **S4** the spore/dormancy reservoir
  (cull-alone-fails) and **S5** Mode B obligate symbionts / minimal genomes (*Carsonella* 159,662 bp /
  *Syn3.0* 473 genes) — a separate host-dependent mode needing a new `ObligateSymbiont` TrophicRole +
  host-coupling FlowMatrix edge (cannot airborne-contaminate; cull-immune at the environment layer). SP-4
  codex hooks: every genome above is famous + primary-sourced.
