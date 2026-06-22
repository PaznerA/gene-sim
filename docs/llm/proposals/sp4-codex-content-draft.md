# SP-4 — Codex content + UI surface plan (DRAFT)

> **Status:** content draft (parallel-safe; no code, no gate). Feeds the SP-4 codex/inspect slice.
> **Scope:** the player-facing *encyclopedia layer* for the three shipped species, the 5 E. coli anchor
> genes, the 5 trophic roles, and the 4 trophic flows — plus the content schema future species slot into.
> **Invariant #2 (STOP THE LINE):** every word here is *renderer-only display copy + a data file*. No
> genotype→phenotype logic moves into `godot/` or GDScript. The codex is a static lookup keyed on ids the
> **core already exports** (`name`, `so_term`, `go_refs`, species `key`, `trophic_role`). It computes no
> biology — it *annotates* biology the core computed.

The content is grounded in the real data model: species live in `data/species/*.json` (keys `default`,
`ecoli-core`, `bdellovibrio`); the anchor genes are the GO-tagged loci in `ecoli.json`
(`gltA`/GO:0004108, `ptsG`/GO:0008982, `pflB`/GO:0008861, `pta`/GO:0008959, `ldhA`/GO:0008720); roles are
`gp::TrophicRole {Autotroph, Heterotroph, Mixotroph, Decomposer, Predator}`; the SO type on every locus is
`SO:0000704` ("gene"). The three axes the brief asks for map cleanly onto the data model:

| Axis | What it means here | Data hook |
|------|--------------------|-----------|
| **taxonomy** | classification / relationships | species `key`, `niche.parent_key`, NCBITaxon (Stage 5) |
| **ontology** | the molecular-function meaning | `tags.so_term` (SO) + `tags.go_refs` (GO MF) |
| **phenology** | life-cycle / timing behaviour | the growth-phase / biphasic / Lotka–Volterra dynamics the sim emits |

---

## 1. CODEX ENTRIES

Each entry below is the *authored content*. The fields (`headline`, `taxonomy`, `ontology`, `phenology`,
`famous_fact`, `sim_role`, `sources`) are exactly the schema in §3 — so this section IS the seed content
file, just in readable form.

### 1.1 SPECIES

---

#### 🌱 Abstract Plant — the primary producer  (`key: default`)

- **Headline.** The faucet *and* the first toll-gate of the whole ecosystem: the only organism that mints
  new joules into the world, by tapping the abiotic `light` plane. Everything else is re-packaged sunlight.
- **Taxonomy.** Deliberately *abstract* — a stand-in for any photoautotroph (terrestrial plants, algae). In
  the sim it is the `Autotroph` role, the default species (`data/species/default.json`), 9 decoupled
  morphology traits. No real NCBITaxon node yet; it is the schematic "producer."
- **Ontology.** Its loci are tagged `SO:0000704` ("gene") with abstract GO function refs (e.g. GO:0009579
  *thylakoid*-flavoured foliage locus, GO:0003006 reproductive locus). The load-bearing lever is `LeafSize`,
  which drives the `Acquisition` budget channel — a bigger leaf is a bigger solar panel.
- **Phenology.** The renewable clock. Its light-driven Net Primary Production is minted every tick and is the
  slow background pulse every downstream boom/bust is a phase-shifted echo of.
- **Famous fact.** A producer never hands all its captured energy up the chain — it burns some on its own
  respiration; only the leftover (**Net Primary Production**) reaches consumers. And photosynthesis is the
  single most consequential reaction for complex life: it both feeds every food web *and* oxygenated the
  atmosphere (the **Great Oxidation Event**) — the reason the player's E. coli and Bdellovibrio can respire
  at all.
- **Sim role.** `Autotroph`. Income: the `light` channel only (PoolStock channel 0, the solar-influx proxy).
- **Sources.** [Primary production (Wikipedia)](https://en.wikipedia.org/wiki/Primary_production) ·
  [Autotroph (Wikipedia)](https://en.wikipedia.org/wiki/Autotroph) ·
  [Khan Academy — energy flow & primary productivity](https://www.khanacademy.org/science/biology/ecology/intro-to-ecosystems/a/energy-flow-primary-productivity)

---

#### 🦠 *Escherichia coli* K-12 — the decomposer  (`key: ecoli-core`)

- **Headline.** Biology's lab rat, re-cast as the recycler. A metabolic generalist that mineralizes the
  detritus end of the food web back into free nutrient — exactly its job in real soil.
- **Taxonomy.** *Escherichia coli* str. K-12 substr. MG1655 — the field's reference *E. coli*. The K-12
  lineage was isolated in 1922 from a convalescent diphtheria patient at Stanford and has been cultured for a
  century, the single most-studied free-living organism on Earth. Its genome (Blattner *et al.*, *Science*
  1997; GenBank **U00096**) was one of the first finished bacterial genomes. NCBITaxon:511145.
- **Ontology.** Roster of 136 GO-MF-curated loci (BiGG `e_coli_core` × CDS `GCF_000005845.2` ASM584v2,
  public-domain NCBI). The 5 anchor genes (see §1.2) tag the four enzyme-class verbs of central carbon flux:
  transport-and-phosphorylate (`ptsG`), condense-into-TCA (`gltA`), ferment-to-acetyl-CoA (`pflB`),
  overflow-to-acetate (`pta`), reduce-to-lactate-for-redox (`ldhA`). The decomposer's per-org mineralization
  is gene-driven off `pta`/AcetateOverflow — a CRISPRi knockdown literally throttles the recycling.
- **Phenology.** A facultative anaerobe whose "life cycle" is metabolic mode-switching. In a batch culture it
  runs the classic growth curve — **lag → exponential (~20 min doubling) → stationary → death**. The deeper
  cycle is aerobic vs. anaerobic carbon routing: with oxygen it burns carbon through the TCA cycle (`gltA`),
  but **above a critical growth rate (~0.27 /h)** it flips into *overflow metabolism*, spilling acetate via
  Pta-AckA (`pta`) even *with* oxygen; remove oxygen and it switches to mixed-acid fermentation
  (`pflB`→formate, `ldhA`→lactate). This is exactly why a single-species run slides toward stationary/decline
  on a finite pool — and why the trophic loop (the decomposer regenerating nutrient) is what lets the system
  reach a living equilibrium instead of collapse.
- **Famous fact.** The most-sequenced genome in history was still a work in progress: the 1997 *Science*
  deposit carried **243 sequencing errors**, corrected only in the curated U00096.3.
- **Sim role.** `Decomposer` (`niche.trophic_role: "decomposer"`). Income: detritus → free nutrient
  (mineralization), minus the respired efficiency tax. Eligible **prey** for the predator.
- **Sources.** [EcoCyc organism summary](https://www.biocyc.org/ECOLI/organism-summary) ·
  [Blattner *et al.* 1997, *Science* 277:1453](https://pubmed.ncbi.nlm.nih.gov/9025293/) ·
  [Bacterial growth curve (LibreTexts)](https://bio.libretexts.org/Courses/City_College_of_San_Francisco/Introduction_to_Microbiology_(Liu_et_al.)/10:_Microbial_Growth/10.02:_Growth_Curve) ·
  [Overflow metabolism, Basan *et al.* (PMC)](https://pmc.ncbi.nlm.nih.gov/articles/PMC2849250/)

---

#### 🦠 *Bdellovibrio bacteriovorus* — the predator  (`key: bdellovibrio`)

- **Headline.** A comma-shaped bacterium that hunts and eats *other* bacteria — from the *inside*. The sim's
  apex predator: it closes the chain plant → E. coli → Bdellovibrio.
- **Taxonomy.** Genus *Bdellovibrio* (Greek *bdella*, "leech" + Latin *vibrio*, "comma-shaped");
  *bacteriovorus* = "bacteria-devouring." Phylum **Bdellovibrionota** (historically a deltaproteobacterium).
  Reference strain **HD100** (NCBITaxon:264462) — the first predatory bacterium ever fully sequenced. One of
  the smallest free-living bacteria known: ~0.3–0.5 µm × 0.5–1.4 µm, small enough to fit several inside one
  E. coli. Discovered by accident in 1962 by Stolp & Petzold, screening soil for *phages* — they found
  something clearing the bacterial lawn that was **not** a virus.
- **Ontology.** A genome packed with *paralogous degradative-enzyme families* — proteases, nucleases,
  lipases, and peptidoglycan hydrolases — the molecular toolkit for prey entry, killing, and uptake. The sim
  tags 14 anchor loci (curated × CDS `GCF_000196175.1` ASM19617v1, public-domain NCBI): the TCA backbone
  `gltA` (GO:0004108) and the host-interaction / lytic-attack machinery, with `PredationCapacity` anchored on
  **GO:0008745** (lysozyme / peptidoglycan-muralytic activity) — the `hit`/lytic attack lever. The predation
  kernel reads `role` + this attack-rate lever, not specific genes; a `hit`-locus CRISPRi knockdown
  (`PredationCapacity`→0) throttles the attack — the **oversight lever**.
- **Phenology.** A "Jekyll-and-Hyde" **biphasic life cycle**, the whole loop ~3–4 h. **Attack phase:** a
  free-swimming, *non-replicating* hunter — the fastest-swimming bacterium relative to its size (up to
  ~160 µm/s, >100 body-lengths/second) on one sheathed polar flagellum. **Entry:** it gnaws into the prey's
  **periplasm**, reseals the hole, and rounds the host into a sealed larder — the **bdelloplast**. **Growth
  phase:** inside, it stops swimming, secretes hydrolases, and grows as a single **filament** (not binary
  fission), then septates all at once into **3–6 progeny from one E. coli** (up to ~90 from a larger prey).
  **Dormancy:** wild Bdellovibrio is an *obligate* predator — no prey, no reproduction — but it *persists*
  rather than dying, surviving prey-poor periods in a low-metabolism state. This real biology is the direct
  warrant for the sim's **predator-dormancy / starvation-survival** mechanic, which turns a single
  Lotka–Volterra crash into recoverable, oscillating coexistence.
- **Famous fact.** The textbook **"living antibiotic."** Because it *eats* prey rather than poisoning one
  molecular target, prey can't easily evolve resistance — as one researcher put it, you could stuff every
  known resistance gene into a single cell and Bdellovibrio "would just eat it anyway." It kills
  multidrug-resistant *Klebsiella*, *Acinetobacter*, *Salmonella*; it spares Gram-positive bacteria and human
  cells — making it self-limiting (run out of Gram-negative prey and the predator population crashes on its
  own).
- **Sim role.** `Predator` (`niche.trophic_role: "predator"`). Income: predation only — it taps **no** abiotic
  channel. **Host range** (sim `is_prey`): preys on `Heterotroph` + `Decomposer` (E. coli), spares
  `Autotroph` (plant cells), `Mixotroph`, and `Predator` (no hyper-predation).
- **Sources.** [Rendulic *et al.* 2004, *Science* 303:689 "A Predator Unmasked"](https://pubmed.ncbi.nlm.nih.gov/14752164/) ·
  [Bdellovibrio (Wikipedia)](https://en.wikipedia.org/wiki/Bdellovibrio) ·
  ["Living antibiotic" review, Herencias *et al.* 2021](https://www.tandfonline.com/doi/full/10.1080/1040841X.2021.1908956) ·
  [Biphasic GEM model, PLOS Comp Biol 2020](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1007646)

---

### 1.2 ANCHOR GENES  (the E. coli central-carbon-metabolism story)

> All five cluster around the **pyruvate node** — the hub where glycolysis hands off carbon. Glucose enters
> (`ptsG`), breaks to pyruvate, and the cell must *choose*: burn it fully (`gltA`/TCA), dump it as acetate for
> fast ATP (`pta`), or ferment it to formate (`pflB`) or lactate (`ldhA`) when oxygen is gone. Knocking any one
> down reroutes flux down the others — which is what makes them satisfying, true-to-life trait dials.
> Every gene tags **SO:0000704** ("gene"); the GO ref carries its specific molecular function.

---

#### `gltA` — citrate synthase · the TCA gate  (GO:0004108 · sim Trait::GrowthRate)

- **Ontology / GO:0004108** "citrate (Si)-synthase activity" (EC 2.3.3.1): *acetyl-CoA + H₂O + oxaloacetate →
  citrate + CoA + H⁺*. The first committed, rate-setting step of the **TCA (Krebs) cycle**. *(Note: this exact
  GO id is now obsolete upstream — citrate-synthase activity was re-homed — but it stays the codebase's pinned
  anchor literal; the codex should display the human label, not just the number.)*
- **Famous fact.** During aerobic growth on glucose, **>62 % of all acetyl-CoA flux** passes through citrate
  synthase — the single largest native acetyl-CoA drain. E. coli's is an unusual "type II" hexamer
  (trimer-of-dimers) found only in Gram-negatives, switched **off** by NADH so an energy-rich cell throttles
  its own TCA entry.
- **Why it's a good knockdown.** Turn `gltA` down and you choke full oxidation; carbon backs up as acetyl-CoA
  and spills into overflow (acetate). The master "burn-for-energy vs. waste" valve — and why the sim anchors
  **GrowthRate** (the only selection-driving trait) here.
- **Sources.** [EcoCyc CITSYN-MONOMER](https://biocyc.org/gene?orgid=ECOLI&id=CITSYN-MONOMER) ·
  [QuickGO GO:0004108](https://www.ebi.ac.uk/QuickGO/term/GO:0004108)

#### `ptsG` — glucose PTS transporter · the front door  (GO:0008982 · sim Trait::GlucoseUptake)

- **Ontology / GO:0008982** "protein-Nᴾᴵ-phosphohistidine–sugar phosphotransferase activity": the
  PTS-permease function. Encoded by Enzyme IICBᴳˡᶜ, it imports glucose **and phosphorylates it in one move**
  — a phosphate relay (PEP→EI→HPr→EIIA→EIICB) traps glucose as glucose-6-P on arrival, spending no extra ATP.
- **Famous fact.** This same system is the hub of **carbon-catabolite repression** — how the cell senses
  glucose and decides which sugar to eat first. The solute is *chemically modified during transport* (group
  translocation), unlike ordinary active transport.
- **Why it's a good knockdown.** The literal intake valve. Throttle `ptsG` and you starve the whole
  downstream network of carbon — a clean "how much can this cell eat" dial, upstream of every other anchor.
  In the sim it also anchors the decomposer's **detritus affinity**.
- **Sources.** [QuickGO GO:0008982](https://www.ebi.ac.uk/QuickGO/term/GO:0008982) ·
  [EcoliWiki ptsG](https://ecoliwiki.org/colipedia/index.php/ptsG:Gene_Product(s))

#### `pflB` — pyruvate formate-lyase · the anaerobic fork  (GO:0008861 · sim Trait::RespirationMode)

- **Ontology / GO:0008861** "formate C-acetyltransferase activity" (EC 2.3.1.54): *pyruvate + CoA →
  acetyl-CoA + formate*. A **glycyl-radical enzyme** — a radical parked on Gly-734 cleaves the C–C bond
  without producing reducing equivalents (the electrons leave as formate).
- **Famous fact.** That radical is so reactive **oxygen destroys it** — O₂ exposure actually cleaves the
  protein chain — so PFL is *the* anaerobic route to acetyl-CoA, active only when oxygen is gone (and
  rescuable by the GrcA protein after O₂ damage).
- **Why it's a good knockdown.** The fermentation gateway to acetyl-CoA. Knock it down and an anaerobic cell
  loses its main way to make acetyl-CoA from pyruvate, forcing carbon and electrons toward lactate (`ldhA`) —
  the "which fermentation path" switch. Anchors the sim's aerobic↔fermentative **RespirationMode** lever.
- **Sources.** [QuickGO GO:0008861](https://www.ebi.ac.uk/QuickGO/term/GO:0008861) ·
  [PFL glycyl-radical chemistry (PMC)](https://pmc.ncbi.nlm.nih.gov/articles/PMC2774668/)

#### `pta` — phosphate acetyltransferase · the overflow tap  (GO:0008959 · sim Trait::AcetateOverflow)

- **Ontology / GO:0008959** "phosphate acetyltransferase activity" (EC 2.3.1.8): *acetyl-CoA + phosphate ⇌
  acetyl-phosphate + CoA*. The first committed step of the Pta-AckA pathway that converts acetyl-CoA to
  acetate while harvesting an ATP.
- **Famous fact.** The engine of **overflow metabolism** (the bacterial Crabtree effect): fed sugar faster
  than the TCA cycle can burn it (above ~0.27 /h), E. coli stops fully oxidizing carbon and just **dumps the
  excess as acetate** for quick ATP — the microbial equivalent of cutting corners when you're rich in food.
  Acetyl-phosphate also acetylates other proteins as a global signal.
- **Why it's a good knockdown.** The wasteful-but-fast valve. Knock `pta` down and you block acetate overflow,
  pushing carbon back into efficient oxidation — the cleanest "efficient vs. fast-and-wasteful" trade-off in
  the cell. In the sim it gene-drives the decomposer's **per-org mineralization rate** (the "shed carbon back
  to the environment" valve), so a `pta` CRISPRi knockdown literally throttles recycling.
- **Sources.** [QuickGO GO:0008959](https://www.ebi.ac.uk/QuickGO/term/GO:0008959) ·
  [Overflow metabolism (PMC)](https://pmc.ncbi.nlm.nih.gov/articles/PMC2849250/)

#### `ldhA` — D-lactate dehydrogenase · the redox safety valve  (GO:0008720 · sim Trait::FermentationCapacity)

- **Ontology / GO:0008720** "D-lactate dehydrogenase (NAD⁺) activity": *(R)-lactate + NAD⁺ ⇌ pyruvate + NADH
  + H⁺*. Fermentative — reduces pyruvate to D-lactate using NADH.
- **Famous fact.** Its real job isn't the lactate — it's the **NAD⁺**. Anaerobic glycolysis keeps consuming
  NAD⁺; unless NADH is re-oxidized, glycolysis grinds to a halt, and `ldhA` regenerates NAD⁺ to keep ATP
  flowing. A telling detail: LdhA has a **high Kₘ (low affinity)** for pyruvate, so it only fires when
  pyruvate *accumulates* — which is exactly why a `pta` knockout (pyruvate piles up) starts spilling lactate.
- **Why it's a good knockdown.** The redox-balance escape hatch. Knock it down and an anaerobic cell loses a
  main way to dump excess electrons — a vivid "how does this cell stay redox-balanced without oxygen" lever
  that pairs naturally against `pflB` at the same pyruvate fork. Anchors the sim's **FermentationCapacity**.
- **Sources.** [QuickGO GO:0008720](https://www.ebi.ac.uk/QuickGO/term/GO:0008720) ·
  [ldhA regulation/Km (Frontiers Microbiol 2020)](https://www.frontiersin.org/journals/microbiology/articles/10.3389/fmicb.2020.00233/full)

---

### 1.3 TROPHIC ROLES  (`gp::TrophicRole` — how a species earns its joules)

> A faithful encoding of the standard microbial-ecology classification. **Categorical data, declared
> per-species — a role is what an organism *is*, never a dial that drifts with gene edits.**

- **Autotroph** ("self-feeding"). Fixes inorganic carbon using an external energy source — photoautotrophs
  from sunlight, chemo-/lithoautotrophs from oxidizing inorganic compounds (ammonia, sulphur, iron). The base
  of every chain. *Sim:* taps the `light` channel only. *(the plant)*
- **Heterotroph** ("other-feeding"). Cannot fix carbon; must eat organic carbon others made — every animal,
  fungus, and most bacteria. *Sim:* draws organic-carbon taps (free_nutrient/detritus). Eligible prey.
- **Mixotroph.** The fascinating in-betweener — blends or switches between autotrophy and heterotrophy.
  Phagotrophic algae form a *continuum* of nutritional modes; a mixotrophic chrysophyte can photosynthesize
  **and** eat bacteria at once. "Obligate" = needs both to survive; "facultative" = one is a supplement. A
  living rebuttal to tidy textbook categories.
- **Decomposer** (saprotroph). A chemoheterotroph using **extracellular** digestion — it secretes enzymes
  into the environment, then absorbs the freed nutrients. Nature's recycler; why an ecosystem isn't buried in
  its own corpses. *Sim:* E. coli, mineralizing detritus → free nutrient. Eligible prey.
- **Predator.** In this sim a *strict* predator — earns **only** by consuming other organisms' joules, tapping
  no abiotic channel at all (that structural purity is exactly why it's a dedicated role and not just "a hungry
  heterotroph": a `Heterotroph + affinity` would double-dip the abiotic taps). *Sim:* Bdellovibrio, eating the
  E. coli decomposer. Not itself prey.

The chain the sim builds is complete and real: **plant (Autotroph) → detritus → E. coli (Decomposer) →
Bdellovibrio (Predator).** Kill the predator and you get a textbook **trophic cascade** — prey boom →
mineralization boom → plant boom — emergent, not scripted.

*Sources.* [Trophic levels (Labster theory)](https://theory.labster.com/troph/) ·
[Mixotroph (Wikipedia)](https://en.wikipedia.org/wiki/Mixotroph) ·
[Heterotroph (Wikipedia)](https://en.wikipedia.org/wiki/Heterotroph) ·
[Food-web components (LibreTexts)](https://bio.libretexts.org/Courses/Gettysburg_College/01:_Ecology_for_All/19:_Food_Webs/19.01:_Introduction_to_and_Components_of_Food_Webs)

---

### 1.4 TROPHIC FLOWS  (the 4 joule transfers — the RELATIONS-view content)

> The sim enforces **strict joule conservation** — energy flows one way and degrades to heat at each step,
> exactly as real ecosystems obey thermodynamics. Each flow below is one edge in the FlowMatrix the
> Relations view renders.

- **F1 · Solar light influx** (primary production / photosynthesis). The *only* joules **minted** into the
  world each tick. Photosynthesis abstracted: light is the renewable, externally-supplied energy that powers
  Net Primary Production. NPP is what's left after the producer's own respiration — and it is the **entire
  ecosystem's energy budget**; every downstream flow is a transformation of these minted joules, never a
  creation of new ones.
  *Source:* [Energy flow & primary productivity (Khan Academy)](https://www.khanacademy.org/science/biology/ecology/intro-to-ecosystems/a/energy-flow-primary-productivity)
- **F2 · Decomposer mineralization** (detritus → free nutrient; the soil nutrient cycle). Dead organic matter
  is converted into plant-available inorganic nutrient, the rest respired as the efficiency tax — the literal
  soil N/C cycle. **Mineralization** turns *organic* nitrogen into *inorganic* mineral forms plants take up.
  The numbers teach: each ton of soil organic matter microbes decompose can release ~220 lb of nitrogen,
  ~33 lb of phosphorus, and ~33 lb of sulphur in plant-available form. In the sim this **closes the loop** —
  the plant's own dead matter feeds the microbe, which regenerates the nutrient the plant needs.
  *Source:* [Soil microbes & nutrient recycling (Ohio State)](https://cfaes.osu.edu/fact-sheet/understanding-soil-microbes-and-nutrient-recycling)
- **F3 · Predation** (Bdellovibrio eats E. coli; real org-eats-org). The first true organism-eats-organism
  flow: a predator consumes a co-located prey's joules on a frozen snapshot — it gains energy, pays an
  efficiency tax, and the carcass returns to detritus. Drives the classic **Lotka–Volterra** dynamics —
  coupled populations that *oscillate* with no external forcing. Real Bdellovibrio predation has been modelled
  with Lotka–Volterra + Holling type II/III responses; chemostat studies show predator washout, stable
  coexistence, *or* sustained oscillation. Remove the predator and watch the cascade ripple to the plant.
  *Sources:* [Lotka–Volterra (Wikipedia)](https://en.wikipedia.org/wiki/Lotka%E2%80%93Volterra_equations) ·
  [Biphasic GEM model, PLOS Comp Biol 2020](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1007646)
- **F4 · Chemical / allelopathy** (the toxin field; microbial chemical warfare). The endogenous chemical-signal
  field: organisms spend joules to emit into three planes — **toxin** (allelopathy), **kin** (a presence
  marker for kin-cooperation), and **alarm** (distress) — which diffuse, decay, and are sensed by neighbours
  (toxin suppresses uptake + a lethal drain; kin boosts survival; alarm biases dispersal). This is real
  **allelopathy**: secondary metabolites (phenolics, terpenoids, flavonoids, alkaloids) released by plants,
  algae, bacteria, or fungi to inhibit competitors. In microbes it's outright warfare — secreted antibiotics,
  enzymes, Type VI secretion systems (e.g. *Streptomyces* linearmycins lysing *Bacillus subtilis*). The subtle
  twist the sim captures via the **kin-sparing toxin**: real communities *modulate, degrade, or amplify*
  allelochemicals — allelopathy is a community-level negotiation, not one-way poisoning.
  *Sources:* [Allelopathy (Wikipedia)](https://en.wikipedia.org/wiki/Allelopathy) ·
  [Microbial chemical warfare (PMC)](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC3997803/)

---

## 2. UI SURFACE PLAN

> **Invariant #2.** All three surfaces are *renderer-only*: they read ids the core already exports (`name`,
> `so_term`, `go_refs`, species `key`, `trophic_role`, the FlowMatrix edges) and look the authored copy up in
> a **static codex data file** shipped to `res://`. No genotype→phenotype, no flow computation, ever moves
> into GDScript. The codex annotates; it never derives.

The content reaches the player through the three existing inspect/specimen/relations seams — no new view mode
required, just enrichment.

### 2.1 INSPECT panel (`🔍 INSPECT`, `godot/main.gd::_fill_detail`)

Today, clicking a cell pins a panel that already lists the species genome ontology as bare rows:
`"• <name>  <so_term>  <go_refs>"` (main.gd ~L2599–2607, reading `loci` from the genome / `_live.loci()`).

**Enrichment (display-only):**
- For each locus row, look up `go_refs[0]` in the codex's **gene** table → append the human label and a
  one-line gloss: `• gltA  SO:0000704 gene  GO:0004108 citrate synthase — the TCA gate`.
- Add a **species header** keyed on the active species `key`: emoji + display name + a one-line headline +
  the trophic-role badge (resolved from `niche.trophic_role`). E.g. `🦠 E. coli K-12 — the decomposer
  · Decomposer`.
- A "**Codex ▸**" affordance on the header opens the full CODEX panel (§2.3) at that species' entry.
- Unknown `key`/GO (future species before content lands) degrades to the *current* bare-id row — never an
  error, never a blank (mirrors the `0.0`/fallback discipline the core already uses).

### 2.2 Tooltips (hover; `godot/main.gd::_tooltip` / `_tooltip_label`)

The cursor-following tooltip currently shows per-cell snapshot stats. Add a **lazy one-liner** layer:
- Hovering a **gene** row (in INSPECT or a future gene chip) → the gene's `famous_fact` truncated to one
  line: *"gltA · >62 % of acetyl-CoA flows through citrate synthase — the TCA gate."*
- Hovering a **trait** readout (the SPECIMEN view's `TRAIT_KEYS` / `MICROBE_TRAIT_KEYS` rows) → the
  trait↔gene mapping: *"GrowthRate ← gltA (citrate synthase)."*
- Hovering a **role badge** → the one-line role definition from §1.3.
- Tooltips read the same codex table; they hold no logic, only string lookups keyed on the hovered id.

### 2.3 CODEX panel (browsable encyclopedia; new `PanelChrome`-wrapped Control)

A new docked, minimizable panel (reusing `panel.gd` chrome exactly like VITALS/SPECIMEN/RELATIONS — a pill on
the rail, draggable, `set_active`/`set_title`). Opened by a top-bar `📖 CODEX` button or the INSPECT header
affordance. Layout:
- **Left rail:** category tabs — *Species · Genes · Roles · Flows* — each a flat list of entries from the
  codex data file (iterated in a **stable declared order**, never a Dictionary key order — UI hygiene mirror
  of inv #3).
- **Right pane:** the selected entry rendered with its three labelled axes (**Taxonomy / Ontology /
  Phenology**), the `famous_fact` highlighted, the `sim_role` line, and source links (rich-text URLs,
  view-only).
- **Cross-links:** a species entry lists its anchor genes (click → Genes tab); a gene entry links its species
  + the trait it anchors; a flow entry links the two roles it connects → ties the CODEX to the **Relations
  FlowMatrix** (clicking a heatmap edge can deep-link to the matching flow entry).
- **Wiring:** parallels `_specimen_panel` / `_relations_panel` — built once at boot, toggled by the view/key
  buttons, fed purely from the static `res://data/codex/*.json`. No `_live` call, no snapshot read needed
  (it's reference content, not per-tick state).

**Relations-view tie-in.** The Relations heatmap (`relations_heatmap.gd`, the S×S FlowMatrix) already renders
the 4 flows as edges; the CODEX flow entries are the long-form companion — hover an edge → tooltip one-liner
(§2.2), click → CODEX flow entry (§2.3).

---

## 3. EXTENSIBILITY — the content schema

The codex is a **data file**, not code — so future species drop in by adding rows, never by touching GDScript
(inv #2) or Rust. One JSON file (`data/codex/codex.json`, copied to `res://data/codex/`), four ordered arrays
keyed on ids the core already emits.

```jsonc
{
  "format_version": 1,
  "species": [
    {
      "key": "ecoli-core",              // MUST match data/species/*.json `key`
      "emoji": "🦠",
      "display_name": "Escherichia coli K-12",
      "headline": "Biology's lab rat, re-cast as the recycler.",
      "taxonomy": "E. coli str. K-12 substr. MG1655 … NCBITaxon:511145.",
      "ontology": "136 GO-MF loci; 5 anchor genes span the four carbon-flux verbs …",
      "phenology": "Facultative anaerobe; lag→log(~20min)→stationary→death; overflow above ~0.27/h.",
      "famous_fact": "The 1997 Science deposit carried 243 sequencing errors.",
      "sim_role": "Decomposer — mineralizes detritus → free nutrient. Eligible prey.",
      "anchor_genes": ["gltA", "ptsG", "pflB", "pta", "ldhA"],  // → gene entries
      "ncbi_taxon": 511145,             // Stage-5 taxonomy graph hook (optional)
      "sources": ["https://www.biocyc.org/ECOLI/organism-summary", "…"]
    }
  ],
  "genes": [
    {
      "symbol": "gltA",
      "go": 4108,                       // MUST match a tags.go_refs entry in the species genome
      "so": 704,                        // SO feature type (gene)
      "go_label": "citrate (Si)-synthase activity",
      "one_line": "the TCA gate",
      "species_key": "ecoli-core",
      "trait": "growth_rate",           // matches Trait::snake_name (the SPECIMEN readout key)
      "ontology": "EC 2.3.3.1; acetyl-CoA + oxaloacetate → citrate.",
      "famous_fact": ">62% of acetyl-CoA flux passes through citrate synthase.",
      "knockdown": "Chokes full oxidation → carbon spills to acetate.",
      "sources": ["https://www.ebi.ac.uk/QuickGO/term/GO:0004108"]
    }
  ],
  "roles": [
    { "id": "decomposer", "title": "Decomposer (saprotroph)",
      "one_line": "Extracellular digestion of dead matter → free nutrient.",
      "body": "…", "is_prey": true, "sources": ["…"] }
    // id MUST match gp::TrophicRole via role_from_str (autotroph/heterotroph/mixotroph/decomposer/predator)
  ],
  "flows": [
    { "id": "predation", "title": "Predation", "from_role": "predator", "to_role": "decomposer",
      "phenology": "Lotka–Volterra oscillation, no external forcing.",
      "body": "…", "sources": ["…"] }
    // from_role/to_role tie a FlowMatrix edge to its codex entry
  ]
}
```

**The slot-in contract for a new species** (e.g. the contamination/symbiont set): add one `species` row keyed
on its `data/species/<x>.json` `key`, add `genes` rows for each GO-tagged anchor locus, reuse existing `roles`
ids (or add one if a genuinely new role lands). The INSPECT/tooltip/CODEX surfaces light up automatically —
they iterate the arrays and join on `key`/`go`/`role-id`. **Missing entry → graceful degrade to bare ids**
(never an error), so a species can ship *before* its codex copy is written and gain it later with zero code
change. Iteration is over the ordered arrays (never Dictionary keys) — the inv-#3 hygiene the UI already keeps.

**Link to the contamination epic.** The contamination/symbiont roster — **Mycoplasma** (one of the smallest
genomes / smallest free-living cells), **Bacillus subtilis** (the Gram-positive sporulation model — and
Bdellovibrio's *immune* prey, a built-in teaching contrast), and the endosymbionts **Carsonella** (~160 kb,
the most reduced cellular genome class) and **Hodgkinia** — are **codex gold**: their genomes are famous
precisely *because* of an extreme number (smallest, most-reduced, sporulating, Gram-positive-and-uneatable).
Each already has a memorable `famous_fact` and a clean taxonomy/ontology/phenology story, so they slot into
this schema with no new surface work. (See the contamination epic for the species-spec/JSON side; this codex
schema is its display companion — one `species` row + `genes` rows per organism.)

---

## 4. PASTE-READY ENRICHMENT SNIPPETS

### 4.1 For `docs/llm/TAXONOMY.md` (append after §4 — the ontology graph section)

```markdown
## 6. Codex (player-facing annotation layer) 🔭 (Stage 4/SP-4 — renderer-only)

The **codex** is a static display-content layer (`data/codex/codex.json`) that annotates — never derives —
biology the core computed. It is keyed entirely on ids the core already exports, so it carries **no logic**
and lives outside the sim path (inv #2: renderer-only):

| Codex table | Joins on | Core source of truth |
|-------------|----------|----------------------|
| `species[]` | `key` | `data/species/*.json` `key` |
| `genes[]`   | `go` (+ `so`) | a locus's `tags.go_refs` / `tags.so_term` |
| `roles[]`   | `id` | `gp::TrophicRole` (`role_from_str`) |
| `flows[]`   | `from_role`/`to_role` | the FlowMatrix edges |

Each entry carries the three annotation axes — **taxonomy** (classification), **ontology** (the SO/GO
molecular-function meaning), **phenology** (life-cycle/timing) — plus a `famous_fact` and `sources[]`.
A missing entry degrades to the bare exported ids (never an error), so a species can ship before its codex
copy exists. Iteration is over the ordered arrays, never a Dictionary's keys (inv #3 UI hygiene).
```

### 4.2 For `docs/llm/GLOSSARY.md` (append under "Ontologies / data")

```markdown
- **Codex** — the player-facing encyclopedia layer: a static `data/codex/codex.json` of authored
  **taxonomy / ontology / phenology** copy for each species, anchor gene, trophic role, and trophic flow.
  Renderer-only (inv #2): it *annotates* core-exported ids (species `key`, locus `go`/`so`, `TrophicRole`,
  FlowMatrix edges), never computes biology. Surfaces: the INSPECT panel, hover tooltips, and a browsable
  CODEX panel. Missing entry → graceful degrade to bare ids.
- **Anchor gene** — a GO-tagged locus the genotype→phenotype map binds a `Trait` to (E. coli: `gltA`/GO:0004108
  →GrowthRate, `ptsG`/GO:0008982→GlucoseUptake, `pflB`/GO:0008861→RespirationMode, `pta`/GO:0008959→
  AcetateOverflow, `ldhA`/GO:0008720→FermentationCapacity). The CRISPR levers; each is a codex `genes[]` entry.
- **Trophic role** — *what an organism is* (categorical, declared per-species, never drifts with edits):
  `Autotroph · Heterotroph · Mixotroph · Decomposer · Predator` (`gp::TrophicRole`). Sets which joule tap a
  species draws from. The plant→detritus→E. coli→Bdellovibrio chain is Autotroph→Decomposer→Predator.
- **Trophic cascade** — a top-down ripple: remove the predator → prey boom → mineralization boom → plant boom.
  Emergent in the sim (not scripted), the payoff of the conserved joule economy.
- **Living antibiotic** — Bdellovibrio's claim to fame: it *eats* Gram-negative prey rather than poisoning a
  molecular target, so prey can't easily evolve resistance. Game: the `Predator` role + `PredationCapacity`
  attack lever (GO:0008745); spares Gram-positives (the future *Bacillus* teaching contrast).
- **Mineralization** — decomposers converting *organic* nutrient (detritus) into plant-available *inorganic*
  (free_nutrient). Game: the F4 decomposer flow, gene-driven off `pta`/AcetateOverflow.
- **Overflow metabolism** — E. coli dumping carbon as acetate for fast ATP when fed faster than the TCA cycle
  can burn it (above ~0.27 /h); the bacterial Crabtree effect. Game: the `pta`/AcetateOverflow lever.
- **Bdelloplast** — the rounded, sealed prey cell Bdellovibrio remodels and lives inside during its growth
  phase. Game: flavour for the predator's biphasic phenology (attack ↔ growth/dormancy).
```

---

## 5. ROADMAP ENTRY — SP-4

**SP-4 (Codex / annotation layer, renderer-only):** ship `data/codex/codex.json` — authored
taxonomy/ontology/phenology copy with `famous_fact`s and sources for the three shipped species (abstract
plant, E. coli decomposer, Bdellovibrio predator), the five E. coli anchor genes
(`gltA`/`ptsG`/`pflB`/`pta`/`ldhA`), the five trophic roles, and the four trophic flows — then surface it
through the three existing read-only seams: enrich the **INSPECT** panel's ontology rows with gene
labels/glosses + a species/role header, add **hover tooltips** (gene famous-facts, trait↔gene mapping, role
definitions), and add a browsable **CODEX** panel (`panel.gd` chrome, Species/Genes/Roles/Flows tabs,
cross-linked to the Relations FlowMatrix). All strictly invariant-#2 clean: the codex *joins on ids the core
already exports* (`key`, `go`/`so`, `TrophicRole`, FlowMatrix edges) and computes no biology — a missing entry
degrades to bare ids, so future species (the contamination/symbiont set: Mycoplasma, Bacillus, Carsonella,
Hodgkinia — whose extreme genomes are codex gold) slot in by adding rows, zero code change.
```
