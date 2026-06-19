# GLOSSARY — domain terms (biology ↔ game ↔ engineering)

> Keep both languages: how a term reads to a biologist and how it maps in the sim. Add a term the first
> time a slice introduces it.

## Biology / CRISPR
- **CRISPR** — a programmable gene-editing system: a Cas protein guided by an RNA to a matching DNA site.
- **Cas variant / "scissors"** — the nuclease the player picks. Each has a PAM rule, cut behaviour, and
  edit type. Game: a `CasVariant` data row (SpCas9, SaCas9, Cas12a, SpRY, base/prime editors).
- **PAM (Protospacer Adjacent Motif)** — a short DNA motif the Cas protein must find next to the target
  (e.g. SpCas9 `NGG`, Cas12a `TTTV`). Game: pattern matched in a locus sequence to validate an edit.
- **Guide / guide RNA / spacer** — the ~20 nt sequence directing the Cas to its target. Game: `GuideSequence`.
- **On-target efficiency** — how well a guide cuts its intended site. Game: `OnTargetScore` ∈ [0,1], gates the edit.
- **Off-target hits** — unintended sites the guide also matches. Game: `OffTargetScore` count; high count →
  failed/partial edit that perturbs *other* Parameters.
- **DSB / base edit / prime edit** — double-strand break vs. single-base change vs. search-and-replace edit
  (no DSB for base/prime). Game: `EditType`.
- **Locus** — a position/feature on the genome (gene, exon, promoter…). Game: `Locus` (sequence + Parameters + ontology tags).
- **Allele frequency** — fraction of a population carrying a variant. Game: a per-generation stat; invariant ∈ [0,1].
- **Gene drive / daisy-chain drive** — an inheritance-biasing element; daisy-chain variants self-limit by
  diluting ~50%/generation until exhausted. Game: the **kill-switch / biosafety** mechanic (Stage 5).
- **Genotype → phenotype** — how genome parameters become observable traits. Game: `GenotypePhenotypeMap`.

## Ontologies / data
- **SO (Sequence Ontology)** — controlled vocabulary of sequence feature *types* (the locus "kind").
- **GO (Gene Ontology)** — controlled vocabulary of gene *functions* (MF/BP/CC). `go-basic.obo` is the seed.
- **NCBI Taxonomy** — clade/lineage nodes; extended as in-game lineages emerge.
- **Ontology extension boundary** — the schema-validated gate where the LLM may add new SO/GO subclasses
  (Stage 5). The only place new "genes" enter the sim.

## Engineering / sim
- **Headless** — runs with no renderer/window (SPEC inv. #4). The core is headless-first.
- **ECS (Entity-Component-System)** — Bevy's data layout: organisms are **entities**, never RL agents (inv. #6).
- **Tick / generation** — one fixed step of the sim loop. The harness runs N generations per run.
- **Determinism gate** — same seed twice → identical hash (`tools/check_determinism.sh`). Hard, non-negotiable.
- **Seed → sub-seed derivation** — one master seed deterministically derives every RNG/`-seed` (inv. #3).
- **Snapshot** — a compact binary dump (bincode) the renderer reads in bulk; never per-entity across the boundary.
- **Oracle** — an external scientific tool run as a **subprocess** (SLiM for pop-gen, Crisflash for off-targets).
- **Slice** — the smallest vertical change that keeps the build green and advances the bar (SPEC §7.2).
- **Stop the line** — halt + surface to the human on any invariant violation; never work around it (SPEC §2.1).
- **Harness (gym-like)** — `reset()/step()/seed()` API around the core for parallel seeded runs (SPEC §2.2).
