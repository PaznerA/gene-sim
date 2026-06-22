#!/usr/bin/env python3
"""Bake data/species/cutibacterium.json — the real Cutibacterium acnes KPA171202 CONTAMINANT genome as a
gene-sim SpeciesSpec (ADR-019 S0, Mode A — the slow lipophilic anaerobe that survives antiseptic prep).
Mirrors scripts/bake_mycoplasma_species.py / bake_bacillus_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Cutibacterium species file: it reads ONE pinned public source and emits a byte-identical cutibacterium.json
on every run.

WHY CUTIBACTERIUM ACNES IS A KEYSTONE CONTAMINANT (contamination-immigration-draft §4 Mode A):
  C. acnes (formerly Propionibacterium acnes) defeats the ANTISEPTIC-PREP + AEROBIC-DETECTION barrier — it is
  an aerotolerant ANAEROBE that grows slowly (cultures take >5 days, so it is missed by standard aerobic
  contamination checks) and lives in sebaceous follicles where sebum dampens disinfectant penetration, so it
  survives skin-antiseptic prep [PMC9250478; PMC10891977]. It is a slow lipophilic decomposer that mineralizes
  sebum/lipid detritus → `niche.trophic_role = "decomposer"` (the data-driven gp::role_from_override ->
  Decomposer), the same detritus -> free_nutrient flow class E. coli and B. subtilis already occupy.

GENOME PROVENANCE (verified, primary-sourced):
  - 2,560,265 bp / 2,333 ORFs, ~60 % GC [Brüggemann et al. 2004, Science 1100330, the KPA171202 reference].
    The modern RefSeq annotation (NC_006085.1 / ASM834v1) carries 2,420 CDS in cds_from_genomic.
  - Reference assembly (pinned): NCBI RefSeq GCF_000008345.1 (ASM834v1), public domain.

WHY A CURATED ROSTER (not genome-complete): like bdellovibrio.json / mycoplasma.json / bacillus.json, the
immigration MECHANIC reads role + trait levers, NOT specific genes (contamination-immigration-draft §5.1).
So a hand-curated set is sufficient and faithful: (a) the central-metabolism + PROPIONATE-fermentation
backbone (glycolysis gap/eno/pyk + pdhA + the sucC/sucD/fumC route toward propionate — the anaerobe's
signature fermentation that names the genus *Propioni*bacterium); (b) the SEBUM-DEGRADING / HOST-INTERACTION
apparatus (the patatin-like phospholipase + GDSL lipase that liberate free fatty acids from sebum
triglycerides — the lipophilic identity; the camp2/camp4 CAMP pore-forming toxins; an exo-alpha-sialidase and
a glycerophosphodiester phosphodiesterase — the follicle-colonization machinery). Each curated locus is a
REAL KPA171202 CDS (pure ACGT). Because this older annotation gene-names only its metabolism, virulence loci
are selected by RefSeq LOCUS_TAG (a hybrid gene-symbol-OR-locus_tag selector), keeping the bake reproducible.

The roster ships with empty go_refs (the contaminant is inert DATA on disk until an immigration config
references it — hash-neutral per ADR-019 S0; no sim-core TraitMap binds it in this slice, exactly as the
non-anchor bdellovibrio loci ship with empty go_refs). The niche declares `trophic_role` + the entity_count.

Run (needs network):  python3 scripts/bake_cutibacterium_species.py
Then commit data/species/cutibacterium.json + godot/data/species/cutibacterium.json (renderer mirror); the
harness gate test `shipped_cutibacterium_species_loads` enforces it builds + round-trips + the decomposer role.
"""

from __future__ import annotations

import gzip
import io
import json
import re
import sys
import urllib.request
from pathlib import Path

# ── PINNED SOURCE (inv #7 — change only deliberately; a different pin re-bakes a different file) ──────────────
NCBI_CDS_URL = (
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/008/345/"
    "GCF_000008345.1_ASM834v1/GCF_000008345.1_ASM834v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000008345.1 ASM834v1"
SO_GENE = 704  # SO:0000704 "gene"

# ── CURATED CONTAMINANT-ANCHOR ROSTER ────────────────────────────────────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (selector, display name, [go_refs]) where `selector`
# is matched FIRST against `[gene=...]`, then against `[locus_tag=...]` (a hybrid selector — this old KPA171202
# annotation gene-names only its metabolism, so the virulence loci are pinned by RefSeq locus_tag). All ship
# with empty go_refs in S0 (no sim-core TraitMap binds a contaminant yet — hash-neutral).
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── central metabolism + propionate fermentation (the anaerobe's energy; *Propioni*bacterium signature) ──
    ("gap", "gap", []),               # glyceraldehyde-3-phosphate dehydrogenase (glycolysis)
    ("eno", "eno", []),               # enolase (glycolysis)
    ("pyk", "pyk", []),               # pyruvate kinase (glycolysis, ATP-yielding step)
    ("pgi", "pgi", []),               # glucose-6-phosphate isomerase (glycolysis)
    ("tpiA", "tpiA", []),             # triose-phosphate isomerase (glycolysis)
    ("pdhA", "pdhA", []),             # pyruvate dehydrogenase E1 (pyruvate -> acetyl-CoA)
    ("aceE", "aceE", []),             # pyruvate dehydrogenase E1 component
    ("sucC", "sucC", []),             # succinyl-CoA synthetase beta (toward propionate)
    ("sucD", "sucD", []),             # succinyl-CoA synthetase alpha
    ("fumC", "fumC", []),             # fumarase (toward propionate via succinate)
    ("atpA", "atpA", []),             # F0F1 ATP synthase subunit alpha
    ("atpD", "atpD", []),             # F0F1 ATP synthase subunit beta
    ("glpK", "glpK", []),             # glycerol kinase (glycerol from sebum lipolysis -> glycolysis)
    # ── sebum-degrading lipases + host-interaction apparatus (selected by locus_tag) ──
    ("PPA_RS00165", "lip_patatin", []),   # patatin-like phospholipase — sebum triglyceride lipase (FFA release)
    ("PPA_RS14310", "lip_gdsl", []),      # GDSL-type esterase/lipase (lipid degradation)
    ("PPA_RS03515", "camp2", []),         # CAMP factor pore-forming toxin 2 (host-cell interaction)
    ("PPA_RS06210", "camp4", []),         # CAMP factor pore-forming toxin 4
    ("PPA_RS03500", "sialidase", []),     # exo-alpha-sialidase (host-glycan / follicle colonization)
    ("PPA_RS06175", "glpQ", []),          # glycerophosphodiester phosphodiesterase (lipid-derived glycerol)
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "cutibacterium.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "cutibacterium.json"


def fetch(url: str) -> bytes:
    print(f"» fetch {url}", file=sys.stderr)
    req = urllib.request.Request(url, headers={"User-Agent": "gene-sim-bake/1.0"})
    with urllib.request.urlopen(req, timeout=180) as r:  # noqa: S310 (pinned trusted host)
        return r.read()


def ncbi_cds() -> tuple[dict[str, str], dict[str, str]]:
    """({gene_symbol: seq}, {locus_tag: seq}) from the NCBI cds_from_genomic FASTA (first occurrence wins)."""
    raw = gzip.GzipFile(fileobj=io.BytesIO(fetch(NCBI_CDS_URL))).read().decode("ascii")
    by_gene: dict[str, str] = {}
    by_tag: dict[str, str] = {}
    gene, tag, seq = None, None, []
    for line in raw.splitlines():
        if line.startswith(">"):
            s = "".join(seq)
            if gene is not None and gene not in by_gene:
                by_gene[gene] = s
            if tag is not None and tag not in by_tag:
                by_tag[tag] = s
            seq = []
            gm = re.search(r"\[gene=([^\]]*)\]", line)
            tm = re.search(r"\[locus_tag=([^\]]*)\]", line)
            gene = gm.group(1).strip() if gm else None
            tag = tm.group(1).strip() if tm else None
        else:
            seq.append(line.strip())
    s = "".join(seq)
    if gene is not None and gene not in by_gene:
        by_gene[gene] = s
    if tag is not None and tag not in by_tag:
        by_tag[tag] = s
    print(f"» NCBI CDS: {len(by_gene)} gene-named, {len(by_tag)} locus_tag sequences", file=sys.stderr)
    return by_gene, by_tag


def main() -> int:
    by_gene, by_tag = ncbi_cds()

    loci = []
    missing = []
    for sel, name, go_refs in ROSTER:
        seq = (by_gene.get(sel) or by_tag.get(sel) or "").upper()
        if not seq:
            missing.append(sel)
            continue
        bad = next((i for i, c in enumerate(seq) if c not in "ACGT"), None)
        if bad is not None:
            print(f"  ! {sel} ({name}) has non-ACGT base at {bad}; skipping", file=sys.stderr)
            missing.append(sel)
            continue
        loci.append({
            "id": len(loci),
            "name": name,
            "sequence": seq,
            "parameters": [
                {"id": 0, "value": {"kind": "numeric", "value": 1.0, "min": 0.0, "max": 1.0}},
            ],
            "tags": {"so_term": SO_GENE, "go_refs": go_refs},
        })

    if missing:
        print(f"» WARNING: {len(missing)} roster loci had no usable CDS: {', '.join(missing)}", file=sys.stderr)
    if not loci:
        print("» FATAL: no curated locus resolved to a real CDS — check the pinned assembly", file=sys.stderr)
        return 1

    spec = {
        "format_version": 1,
        "key": "cutibacterium",
        "name": "Cutibacterium acnes KPA171202",
        "niche": {
            "entity_count": 120,  # contaminants seed SPARSE — immigration pressure, not a founding population
            "description": (
                f"Curated contaminant-anchor roster x CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: "
                f"the central-metabolism + propionate-fermentation backbone (glycolysis gap/eno/pyk/pgi/tpiA + "
                f"pdhA/aceE + the sucC/sucD/fumC route toward propionate + ATP synthase + glpK) + the sebum-"
                f"degrading lipases (patatin-like phospholipase, GDSL lipase) and host-interaction apparatus "
                f"(camp2/camp4 CAMP toxins, an exo-alpha-sialidase, a glycerophosphodiester phosphodiesterase). "
                f"C. acnes KPA171202: 2,560,265 bp / 2,333 ORFs, ~60% GC; aerotolerant anaerobe, slow (>5 d so "
                f"missed by aerobic checks), sebum dampens disinfectants -> survives antiseptic prep. The "
                f"immigration kernel reads role + trait levers, not specific genes (ADR-019 S0, Mode A)."
            ),
            "temp_optimum": 0.6,  # skin/follicle anaerobe ~37 C — normalized into the sim's [0,1] band
            "parent_key": None,
            "trophic_role": "decomposer",  # the DATA-driven role override (gp::role_from_override -> Decomposer)
        },
        "genome": {"version": 2, "loci": loci},
    }

    text = json.dumps(spec, indent=2) + "\n"
    for out in (OUT, OUT_GODOT):
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(text)
        print(f"» wrote {out} — {len(loci)} loci", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
