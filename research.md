# Reuse-First Tech Stack & Architecture for a CRISPR Genetic-Engineering Simulation Game (2D PoC)

## TL;DR
- **Do not reinvent population genetics or CRISPR scoring.** Wrap SLiM (GPL-3 forward pop-gen simulator, now at v5.2) as a subprocess-driven evolution oracle, and embed the CRISPR science layer from crisprVerse/crisprScore (Bioconductor) algorithms, Cas-OFFinder (off-targets, OpenCL/GPU), and CRISPOR/CHOPCHOP scoring models — all FOSS and actively maintained.
- **Split the architecture in two: a headless, deterministic Rust/Bevy ECS simulation core (performance + volume priority) wrapped in a Gymnasium/PettingZoo-style API for parallel LLM-driven runs, plus a thin Godot 4 rendering/UI layer reading sim state in bulk.** Godot is MIT-licensed, but its scripting (and even its FFI/GDExtension boundary) is too slow for the high-volume sim loop — keep the sim engine-agnostic and headless-first.
- **Seed the parametric genome + ontology from real data**: Sequence Ontology + Gene Ontology (OBO files), NCBI Taxonomy, reference genomes via Ensembl REST/FTP, with L-Py (L-systems) for visible plant morphology. The LLM (Claude Code) generates new ontology/modifier nodes on the fly against a schema-validated extension boundary.

## Key Findings

### This is mostly an integration project, not a greenfield build
For every one of the five subsystems the user described, mature, actively-developed FOSS already exists. The novel engineering is the *glue*: a deterministic, headless sim core that bridges real bioinformatics libraries (Python/R/C++/OpenCL) to a high-volume agent simulation and a thin 2D renderer. The single most important architectural decision is to **keep the simulation core completely separate from Godot**, because Godot's GDScript and even its GDExtension binding are not built for the simulation-volume-first priority the user stated.

### Licensing is the critical constraint and it shapes the whole architecture
The strongest evolution-game prior art (Thrive) and the strongest pop-gen engine (SLiM) are both **GPL-3**, while the best ECS engine (Bevy), the AI-harness (Gymnasium/PettingZoo), and Godot itself are permissive (MIT/Apache). To preserve maximum licensing freedom, GPL components (SLiM especially) should be invoked as **separate command-line subprocesses** (aggregation), not linked into the game binary — this is exactly how the stdpopsim library and Sam Champer's `slim_from_python` driver use SLiM.

---

## Details

### 1. Existing synbio / evolution / ALife games and simulators

**Thrive (Revolutionary Games)** — The closest direct precedent: an open-source evolution game built *on Godot with C#*. Per Revolutionary Games Studio's official itch.io page: "The game is built in the open source Godot engine with the C# programming language. The source code is GPLv3 licensed and our assets are Attribution-ShareAlike 3.0 Unported licensed." It entered Steam Early Access on Nov 26, 2021; the microbe stage is complete and the multicellular stage is in prototype, with regular devbuilds. Its "auto-evo" population-dynamics system, patch/biome model, and species editor are conceptually exactly what this project needs at the ecosystem-scope level. **Reusable as a design reference and as proof that Godot can host this genre**; GPL-3 means any *code* reuse forces the whole game to GPL-3. Contributing back is possible.

**SLiM** — see Section 3 (the single most important reuse target).

**Lenia (Bert Wang-Chak Chan)** — Continuous cellular automata; a 2D ALife system with 400+ "species," self-organization, self-repair, and locomotion. Mass-conserving **Flow-Lenia** (Plantec et al., accepted in *Artificial Life* journal, 2025) embeds update-rule parameters *within the simulation's own dynamics*, enabling multi-species simulations and emergent evolutionary dynamics — directly relevant to the "emergent systems/behavior" and "parametric genome embedded in the sim" requirements. Python/Matlab/JS implementations on GitHub (Chakazul/Lenia). A good source of *mechanics inspiration* and a possible alternative substrate for emergence, but it is not genetically-explicit CRISPR science.

**Hero.Coli (CRI Paris)** — The first synthetic-biology crafting game; a 2D top-down adventure where you collect/combine BioBrick DNA fragments. Built in Unity, open-source (GitHub under CyberCRI), CC-BY licensed. Explicitly inspired by Foldit/EteRNA's citizen-science loop. In Maxime Bertaux's PhD thesis (HAL tel-02524484, 2019), the game was validated by comparing pre- and post-tests of players (n=89), finding "an average of 32 percentage point increase between pretest and posttest correct answer rate per question." **Most relevant as a UX/pedagogy precedent** for "stealth learning" of real molecular biology in a game. Not a sim engine.

**Foldit / EteRNA (Eterna)** — Crowdsourced protein-folding / RNA-design "games with a purpose" (CMU/Stanford/UW). Real scientific back-ends with lab validation loops. The canonical model for "player/AI proposes a biological design → it's scored by a real model → best designs are validated." EteRNA is browser-based; both are precedents for tying a game to a real computational scoring oracle.

**Other ALife (Avida, Tierra, Framsticks, The Bibites, Species: ALRE)** — Avida (digital evolution, used in real research, C++) and Framsticks (3D creature evolution, free for non-commercial) are mature but research/edu-oriented; The Bibites and Species: ALRE are closed-source hobby/commercial games. These inform genotype→phenotype design but are not integration targets.

**Plague Inc / Bio Inc / SimEarth / Spore / Cities: Skylines** — All closed-source; usable only as **UI/UX pattern references**: Plague Inc's escalating-spread + evolve-traits loop, Cities: Skylines' info-view data overlays (per-layer heatmaps, toggled map modes), and SimEarth's multi-scale zoom. These define the *interface vision*, not reusable code.

### 2. The real CRISPR science layer (runnable, FOSS)

This is the strongest reuse story — there is no need to invent any CRISPR math:

- **crisprVerse / crisprScore (Bioconductor, R; open source).** The best-organized reuse target for the science layer. `crisprScore` provides on-target scores **RuleSet1, RuleSet3, Azimuth (Rule Set 2), DeepHF, DeepSpCas9, DeepCpf1, enPAM+GB, CRISPRscan, CRISPRater**, and off-target **CFD and MIT** scores, plus a Lindel-derived frameshift-probability score. `crisprBase` models nucleases and base editors as data objects. Supports SpCas9, AsCas12a, enAsCas12a, RfxCas13d (CasRx). Note: the Python-2-era algorithms (Azimuth, DeepCpf1, DeepSpCas9) now require building conda environments; RuleSet3/DeepHF are the maintained replacements.
- **Cas-OFFinder (snugel/cas-offinder).** Ultrafast, **OpenCL → runs on GPU**, not limited by mismatch count, supports arbitrary PAMs and DNA/RNA bulges. Per Bae, Park & Kim (*Bioinformatics* 30(10):1473, 2014): "The speed of Cas-OFFinder based on GPU (3.0 s) was 20× faster than that of CPU (60.0 s) when 1000 target sites were analyzed" — benchmarked on an Intel i7 3770K CPU vs an AMD Radeon HD 7870 GPU. This is the off-target search engine to embed (as a CLI subprocess) for the "compute realistic off-target effects" mechanic.
- **CRISPOR (Haeussler lab / Tefor; academic open source) and CHOPCHOP (UiB).** Both are reference guide-design tools with documented scoring methods and command-line/local-install paths. CHOPCHOP runs from the CLI with `--scoringMethod DOENCH_2016`, supports many Cas systems, uses Bowtie + a genome 2bit/genePred index, and is scriptable for cluster batch runs. CRISPOR provides detailed position-specific off-target scoring. (Caution: a marketing page at "biology.digital" fabricates an enterprise CRISPOR product and a "$8.3B by 2028" market figure — ignore it; the real CRISPOR is the academic Haeussler/Tefor tool.)
- **Crisflash, OffScan** — fast open-source off-target generators (Crisflash is C, >1 order of magnitude faster than comparable tools; OffScan uses FM-index). Alternatives/supplements to Cas-OFFinder.
- **PAM / Cas-variant data (for the "pick the scissors" mechanic).** Well-documented in the literature and directly encodable as a small data table:
  - SpCas9: PAM **5′-NGG-3′**; blunt cut ~3 bp upstream of PAM.
  - SaCas9: PAM **5′-NNGRRT-3′** (SaKKH variant **5′-NNNRRT-3′**).
  - Cas12a (Cpf1): **T-rich PAM** (5′-TTTV), staggered cut, works well in GC-rich regions, enables multiplexing from a single CRISPR array.
  - SpCas9 PAM-relaxed variants: **xCas9 / Cas9-NG (5′-NG-3′), SpG, SpRY, SpVQR (NGA), SpEQR (NGAG), SpVRER (NGCG)**.
  - **Base editors**: CBE/ABE editing windows roughly positions 4–8 upstream of PAM for canonical BE3; SaCas9-based editors expand windows (e.g., BE3-SaKKH ~1–16 nt); narrowed (YEE) variants reach 1–2 nt. Base editors require no double-strand break.
  - **Prime editors**: use a Cas9 nickase + reverse transcriptase + pegRNA; Cas12a-based circular-RNA prime editors (CPEs) exploit T-rich PAMs.
  A 2024 *BMC Genomics* catalog ("A catalog of gene editing sites and genetic variations in editing sites in model organisms," doi:10.1186/s12864-024-11073-9) characterized six representative Cas proteins (Cas9, Cas12a, Cas12b, Cas12i, Cas12j, Cas12l) across ten model organisms (yeast, flatworms, flies, zebrafish, mice, humans, rice, maize, Arabidopsis, tomato) and reported "more than 34 editing sites per kilobase on average," with 91.69–99.83% of genes having at least one unique editing site in an exon and 95.4–99.73% in a promoter. Useful for tuning how "targetable" the in-game genome is.
- **Sequence handling: rust-bio (MIT) and Biopython.** `rust-bio` is a fast, MIT-licensed Rust library with FASTA/FASTQ/BED I/O, FM-index, pattern matching and alignment — **ideal for embedding PAM-finding and sequence ops directly in a Rust sim core** without GPL entanglement. Biopython is the Python equivalent for the scripting/glue layer.
- **Gene/genome data sources.** Ensembl REST API (Apache-2.0 server, JSON/FASTA, rest.ensembl.org) plus Ensembl/UCSC FTP for offline reference genomes in FASTA; NCBI E-utilities for Entrez. For an offline-first game, download reference genomes once (e.g., *Arabidopsis* ~116 Mb, or *Drosophila*) and ship/cache them rather than hitting APIs at runtime.

### 3. The population / evolution simulation core (performance-critical, headless)

**SLiM is the standout reuse target and should be the evolutionary core** rather than reinventing population-genetics math. Verified facts (from the MesserLab/SLiM GitHub repo, messerlab.org, and Haller & Messer peer-reviewed papers):

- **License: GPL-3.0-or-later** ("Copyright (c) 2016-2025 Benjamin C. Haller"; conda-forge lists "GPL-3.0-or-later"). Strong copyleft → **invoke the `slim` CLI binary as a subprocess** (aggregation), do not link its code, to preserve the game's licensing freedom. This is the documented pattern used by stdpopsim and the `slim_from_python` driver.
- **Headless + deterministic**: ships a `slim` command-line tool (separate from the SLiMgui GUI); runs fully headless with `slim -seed N -d param=value script.slim`. The SLiM 4.2.2 release notes state it "preserves backward reproducibility (the same model with the same seed will produce the same results) in all cases." Batch parallelism via shell loops / cluster queues (e.g., `for TRIAL in {1..4}; do slim -d trialNumber=${TRIAL} model.slim; done`).
- **Tree-sequence recording** (Haller, Galloway, Kelleher, Messer & Ralph, *Molecular Ecology Resources* 19(2):552–566, 2019): records full genealogy to a `.trees` file, loadable in Python via tskit/pyslim/msprime. **Recapitation** lets you skip neutral burn-in; the paper reports that "using recapitation to construct a neutral burn-in period provided a speedup of more than five orders of magnitude (Example 4), and using the tree sequence to obtain true local ancestry information provided a speedup of more than six orders of magnitude (Example 3)." Neutral mutations can be overlaid afterward with `msprime.sim_mutations`.
- **Eidos scripting language** (R-like) controls every aspect of a model; supports Wright-Fisher and non-Wright-Fisher (individual-based, eco-evolutionary) models, mutation, recombination, migration, drift, selection on fecundity/survival, continuous space, and multi-species interaction. There is **no in-process Python API that drives the engine**; `pyslim` is a tskit I/O bridge, and the standard way to "drive" SLiM from Python is to spawn it as a subprocess (as stdpopsim does).
- **Latest version: SLiM 5 (5.2 current; 5.0 released 2025).** SLiM 5 adds multi-chromosome / full-genome simulation (up to 256 chromosomes per species, sex chromosomes XY/ZW/UV, organelle DNA), per the peer-reviewed "SLiM 5" paper (Haller, Ralph & Messer, *MBE*, 2026; bioRxiv preprint Aug 2025). Multi-species eco-evolutionary modeling arrived in SLiM 4 (2022) — directly relevant to "one field/forest/pond ecosystem" with multiple interacting species.
- **Performance/threading**: default single-threaded per run (scale by running many seeded processes in parallel — ideal for the "hundreds of parallel deterministic runs" requirement); optional OpenMP multithreading since v4.1 via a PARALLEL build (`-maxThreads`). The original Haller & Messer benchmark (10 Mb chromosome × 10⁵ generations × N=10⁴) was ~10 hr/core — a conservative upper bound; tree recording makes realistic models far faster.

**Decision: SLiM is the right population-genetics core IF the game's "genome" maps to chromosomes/loci/mutations** (it does, per the user's parametric-genome design). The CRISPR edit becomes, in SLiM terms, an introduced mutation (or a gene-drive element — SLiM has extensive published gene-drive models) at a chosen locus, after which SLiM evolves the population forward. **However**, SLiM's per-run model (one process per simulation) and Eidos-only control surface mean it is best used as a **batch oracle for the genetics**, not as the real-time interactive tick loop. For the real-time, high-entity-count ecosystem visualization and agent behavior, use a separate ECS (below) and treat SLiM as the rigorous genetics backend invoked per generation/epoch.

**Agent-based / ECS alternatives for the real-time core:**
- **Bevy ECS (Rust, MIT/Apache-2.0).** Archetypal ECS, cache-friendly, parallel system scheduling, runs **headless** trivially (use `bevy_ecs` without the render plugins). This is the recommended high-volume real-time sim core: permissively licensed, fast, embeddable, and the same Rust crate can be shared between a headless batch binary and a rendering build.
- **Mesa (Python, Apache-2.0)** — easy, great for prototyping and data collection, but slow; **Agents.jl (Julia, MIT)** is much faster than Mesa (the Agents.jl team documents large speedups over Mesa) but adds a Julia dependency; FLAME GPU / Repast / MASON are heavier. For a performance-first build, **Bevy beats Mesa decisively**; Mesa is only worth it for a throwaway Python prototype.
- **GPU/JAX option**: ABMax (JAX) and Flow-Lenia (JAX) show that `vmap` can run many ABM instances in parallel on a GPU — relevant if the "hundreds of parallel runs" requirement becomes the dominant cost.

### 4. Godot for this use case (and why to keep it thin)

- **Godot 4 headless mode works**: `--display-driver headless` runs without rendering on any desktop OS; dedicated-server export templates exist; Dockerized headless Godot 4 servers are a known pattern. So Godot *can* run headless.
- **But Godot scripting is the wrong place for the high-volume sim loop.** GDScript is interpreted and slow for tight loops; a documented godot-cpp issue (#1063) found GDExtension (C++) was actually *slower* than GDScript for a per-pixel image loop (30 ms vs 20 ms) due to FFI call overhead in hot loops — i.e., crossing the Godot FFI boundary per element is costly. **godot-rust (gdext)** (MPL-2.0) gives type-safe, performant Rust in Godot and lets Rust and GDScript mix, and is the right tool *if* you keep heavy computation inside Rust and only cross the boundary in batches.
- **Conclusion**: Use Godot 4 purely as the **2D rendering + data-layer UI layer** (and only build it after the headless core works). Implement the simulation core as a standalone Rust crate (Bevy ECS) that runs headless for AI/batch, and have the Godot layer read sim state in bulk (shared memory, a tick snapshot buffer, or IPC) rather than driving per-entity logic across the FFI boundary. This satisfies "engine-agnostic sim core" and "headless-first."
- **2D data-layer UI**: implement Cities-Skylines-style overlays with Godot TileMap + custom shaders sampling a per-cell data texture (one channel per data layer: population density, allele frequency, fitness, edit penetrance), with viewport zoom for scope changes. This is standard Godot 4 2D shader work.

### 5. Architecture for LLM/AI-playable games (parallel runs)

- **Adopt the Gymnasium (single-agent) / PettingZoo (multi-agent) API standard** (Farama Foundation, MIT). These are the de facto RL-environment interfaces: `reset()`, `step(action)`, observation/action spaces, strict versioning for reproducibility. PettingZoo's Agent-Environment-Cycle (AEC) model handles multi-agent turns. Known limitation (stated in the PettingZoo paper): environments with significantly more than ~10,000 agents have "meaningful performance issues" because spaces/names are specified at creation — relevant if agents are individual organisms (keep agents at the "operator/species" level, not per-organism).
- **Expose the headless sim core behind this API** so Claude Code agents drive it programmatically: an `EditAction` (Cas variant + target locus + guide) and observations (population stats, allele frequencies, fitness, phenotype layers). The same interface supports hundreds of parallel deterministic instances.
- **Determinism & replay**: seed everything (the Rust core's RNG and SLiM's `-seed`); record action+seed logs so any emergent run can be replayed exactly. SLiM's per-version reproducibility guarantee and Bevy's deterministic scheduling (fixed timestep + explicitly ordered systems) make this achievable.
- **Precedents**: PettingZoo wraps Atari, multi-agent (MAgent-style) and SISL pursuit environments; stdpopsim wraps SLiM for batch genetics. The "game-as-RL-environment" pattern is well established. Note you need the *API shape*, not RL training — adopt the interface, skip the training stack to avoid heavy dependencies.

### 6. Parametric genome + ontology

- **Seed the ontology from real, downloadable data (all OBO/OWL/JSON, open):**
  - **Sequence Ontology (SO)** — feature types (gene, exon, CDS, promoter, etc.); OBO source on GitHub (The-Sequence-Ontology). The natural schema for "what kind of genomic feature is being edited."
  - **Gene Ontology (GO)** — molecular function / biological process / cellular component; `go-basic.obo` recommended. Parse in Python with `obonet`, in R with `ontologyIndex`.
  - **NCBI Taxonomy** — for the taxonomy/clade nodes; extensible as new in-game lineages emerge.
- **Genotype→phenotype mapping**: represent the genome as parametric modular data (loci with typed parameters + ontology tags). Map to traits via either (a) a gene-regulatory-network / weighted-sum model, or (b) NEAT/CPPN-style indirect encoding for richer emergent morphology. For **visible plant/tree morphology in 2D**, use **L-Py** (OpenAlea's Python L-system framework, CeCILL/GPL-compatible) or a lighter permissively-licensed L-system lib (e.g., `pvigier/lsystem`, or the `lindenmayer` JS library) — L-system production-rule parameters become downstream phenotype outputs of the genome, so an edit visibly changes branching/leaf structure.
- **LLM-generated modifiers on the fly**: because the genome and ontology are data (typed parameters + ontology node references), Claude Code can generate new ontology nodes (subclasses of existing SO/GO terms) and new modifier functions (parameter transforms) at runtime against a fixed JSON schema. Validate generated nodes against the schema and the ontology graph before admitting them to the sim — this is the safe extension boundary.
- **Kill-switch / activation-gene biosafety**: model on **real daisy-chain / daisyfield gene-drive containment** (Esvelt/Church, MIT; DARPA Safe Genes). Mechanics that map directly to sim parameters: a payload that spreads only while linked daisy elements remain; daisy elements diluted ~50% per generation (Mendelian) until the drive self-exhausts; anti-CRISPR proteins as "off-switches"; reversal drives. SLiM has published gene-drive models to learn from. This grounds the kill-switch concept in real science rather than flavour.

---

## Recommendations

**Recommended reuse-first MVP stack, by layer:**

| Layer | Reuse | License | Integration |
|---|---|---|---|
| (a) CRISPR science | rust-bio (PAM finding, seq ops) in-core; Cas-OFFinder (off-targets, GPU) + crisprScore/CHOPCHOP (on-target scores) as CLI subprocess oracles; PAM/Cas-variant data table hand-encoded from literature | rust-bio MIT; Cas-OFFinder/CHOPCHOP open; crisprScore (Bioconductor) | Embed rust-bio; shell out to the rest |
| (b) Evolution/pop-gen core | SLiM (batch genetics oracle) + Bevy ECS (real-time ecosystem) | SLiM GPL-3 (subprocess only); Bevy MIT/Apache | SLiM via CLI subprocess + `.trees`/tskit; Bevy as the core crate |
| (c) 2D render + data UI | Godot 4 (thin), TileMap + data-texture shaders | MIT | Reads sim snapshots in bulk; godot-rust (MPL-2.0) only if needed |
| (d) Headless/AI harness | Gymnasium / PettingZoo API around the Rust core | MIT | `reset/step`, seeded, replayable |
| (e) Genome + ontology | Sequence Ontology + GO (OBO) + NCBI Taxonomy; L-Py for plant morphology | SO/GO open; L-Py CeCILL/GPL | Parse OBO with obonet; LLM extends, schema-validated |

**Staged plan (matches "tens of hours, Claude Code multi-agent"):**
1. **Stage 0 — headless core first.** Build the Rust/Bevy ECS sim crate: parametric genome data model, a tick loop, deterministic seeding, and a CLI that runs N seeded instances headless and dumps stats. No graphics. Benchmark entity counts now.
2. **Stage 1 — CRISPR mechanic.** Embed rust-bio for PAM finding; encode the Cas-variant/PAM/editing-window table; wire Cas-OFFinder (subprocess) for off-target counts and a CFD/Doench score for on-target efficiency. An edit = a parameter mutation at a locus.
3. **Stage 2 — genetics realism.** Add SLiM as a subprocess oracle: translate the in-game edit into an Eidos model, run forward, read back allele frequencies/fitness via `.trees`/tskit. Validate determinism (seed in → identical out).
4. **Stage 3 — AI harness.** Wrap the core in a Gymnasium/PettingZoo env; confirm hundreds of parallel deterministic runs; add action/seed replay logs.
5. **Stage 4 — Godot UI last.** Build the 2D Plague-Inc/Cities-Skylines view reading sim snapshots; TileMap + data-layer shaders + zoom scopes. L-Py (or a port) for visible plant morphology.
6. **Stage 5 — ontology + LLM modifiers.** Load SO/GO/NCBI-Taxonomy; expose a schema-validated extension API so Claude Code generates new ontology nodes/modifiers; model daisy-chain kill-switch containment.

**Benchmarks/thresholds that change the plan:**
- If Bevy headless can't hit your target entity count at target tick rate → move the hot path to GPU (JAX/ABMax-style `vmap`) or coarsen organisms into population cohorts (let SLiM carry the genetics, ECS carry only spatial/visible agents).
- If SLiM subprocess latency dominates parallel-run throughput → precompute/cache edit→outcome tables, or run SLiM only at epoch boundaries rather than per tick.
- If PettingZoo's ~10k-agent ceiling is hit → keep RL agents at operator/species granularity, not per-organism.
- If GPL-3 contamination is unacceptable for a planned commercial/closed release → keep SLiM strictly as an optional external tool and fall back to a permissively-licensed pop-gen implementation (e.g., msprime/tskit, which are MIT, or a clean reimplementation of the needed math) for the shipped core.

## Caveats
- **Reproducibility across SLiM versions is not guaranteed**: same seed reproduces only within the same SLiM version. Pin the SLiM version in the build.
- **GPL-3 is genuinely constraining.** Thrive code and SLiM code are both GPL-3; reusing Thrive *code* would force the whole game to GPL-3. The subprocess pattern for SLiM is the standard mitigation but warrants a legal check before any commercial release.
- **Some CRISPR ML scores are dependency-heavy** (Python-2-era Azimuth/DeepCpf1 need conda environments; some DeepHF/DeepCpf1/enPAM+GB are unavailable on Windows). Prefer RuleSet3/DeepHF and CFD for portability; treat exotic scores as optional.
- **The "biology.digital" CRISPOR page is unreliable marketing** (fabricated enterprise product and market-size figures). Use the genuine academic CRISPOR (Haeussler/Tefor) and its published methods.
- **Performance numbers are model-dependent.** The SLiM ~10-hour figure is the original conservative benchmark; Cas-OFFinder's 20× GPU speedup is for a specific 1000-target human-genome test on 2014-era hardware (i7 3770K vs Radeon HD 7870). Re-benchmark on your own hardware/model.
- **Validate during prototyping** (search-budget-limited areas): Godot data-overlay shader specifics, SLiM/tskit determinism edge cases, and the exact NEAT/CPPN library choice for genotype→phenotype indirect encoding. These should be confirmed empirically in Stages 0–4.