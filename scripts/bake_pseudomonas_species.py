#!/usr/bin/env python3
"""Bake data/species/pseudomonas.json — the real Pseudomonas aeruginosa PAO1 CONTAMINANT genome as a gene-sim
SpeciesSpec (ADR-019 S0, Mode A — the biofilm metabolic-generalist that grows in distilled water). Mirrors
scripts/bake_mycoplasma_species.py / bake_bacillus_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Pseudomonas species file: it reads ONE pinned public source and emits a byte-identical pseudomonas.json on
every run.

WHY PSEUDOMONAS AERUGINOSA PAO1 IS A KEYSTONE CONTAMINANT (contamination-immigration-draft §4 Mode A):
  P. aeruginosa defeats the NUTRIENT-STARVATION + DISINFECTANT barrier — it grows in distilled water, on the
  metabolic trace nutrients of nearly any surface, and at the time of sequencing carried the largest set of
  REGULATORY genes of any sequenced genome (the metabolic generalist) [Stover et al. 2000, Nature 35023079].
  It armours itself in an EPS BIOFILM (the Pel/Psl/alginate exopolysaccharides) that resists disinfectants
  and antibiotics [PMC11504098]. It is the metabolic generalist / oligotroph that holds taps a vegetative
  cull cannot reach → `niche.trophic_role = "mixotroph"` per the draft's biofilm-oligotroph generalist class
  (the data-driven gp::role_from_override → Mixotroph). NB: ConsortiumConfig::default_mode_a references the
  `pseudomonas` key directly.

GENOME PROVENANCE (verified, primary-sourced):
  - 6,264,404 bp, 5,570 predicted ORFs [Stover et al. 2000, Nature 406:959 (35023079), the PAO1 reference].
    The modern RefSeq annotation (NC_002516.2 / ASM676v1) carries 5,572 CDS in cds_from_genomic.
  - Reference assembly (pinned): NCBI RefSeq GCF_000006765.1 (ASM676v1), public domain.

WHY A CURATED ROSTER (not genome-complete): like bdellovibrio.json / mycoplasma.json / bacillus.json, the
immigration MECHANIC reads role + trait levers, NOT specific genes (contamination-immigration-draft §5.1).
So a hand-curated set of P. aeruginosa's defining apparatus is sufficient and faithful: (a) the central
CARBON-METABOLISM backbone (gltA/acnB/icd/sdhA + atp + glycolytic enzymes — the generalist's broad central
metabolism that lets it grow on almost anything); (b) the BIOFILM EPS machinery (the pel/psl
exopolysaccharide operons + algD/algU alginate — the disinfectant-resistant matrix); (c) the
QUORUM-SENSING + EFFLUX/DEFENCE apparatus (lasR/rhlR/pqsA quorum regulators, the mexB/oprM multidrug-efflux
pump, katA/sodB oxidative defence — the cull-resistance levers). Each curated locus is a REAL PAO1 CDS
(pure ACGT), selected by its `[gene=...]` annotation (robust across re-annotations).

The roster ships with empty go_refs (the contaminant is inert DATA on disk until an immigration config
references it — hash-neutral per ADR-019 S0; no sim-core TraitMap binds it in this slice, exactly as the
non-anchor bdellovibrio loci ship with empty go_refs). The niche declares `trophic_role` + the entity_count.

Run (needs network):  python3 scripts/bake_pseudomonas_species.py
Then commit data/species/pseudomonas.json + godot/data/species/pseudomonas.json (renderer mirror); the harness
gate test `shipped_pseudomonas_species_loads` enforces it builds + round-trips + declares the mixotroph role.
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
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/006/765/"
    "GCF_000006765.1_ASM676v1/GCF_000006765.1_ASM676v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000006765.1 ASM676v1"
SO_GENE = 704  # SO:0000704 "gene"

# ── CURATED CONTAMINANT-ANCHOR ROSTER (by RefSeq [gene=...] symbol) ──────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (gene_symbol, display name, [go_refs]). All ship with
# empty go_refs in S0 (no sim-core TraitMap binds a contaminant yet — hash-neutral). Curated from the live PAO1
# RefSeq annotation: the central-metabolism backbone (the generalist's broad carbon catabolism) + the Pel/Psl/
# alginate BIOFILM EPS machinery (the disinfectant-resistant matrix) + the quorum-sensing / multidrug-efflux /
# oxidative-defence apparatus (the cull-resistance levers).
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── central carbon metabolism (the metabolic generalist's broad catabolic backbone) ──
    ("gltA", "gltA", []),     # citrate synthase (TCA entry — the growth anchor)
    ("acnB", "acnB", []),     # aconitase B (TCA)
    ("icd", "icd", []),       # isocitrate dehydrogenase (TCA)
    ("sdhA", "sdhA", []),     # succinate dehydrogenase flavoprotein (TCA / aerobic respiration)
    ("atpA", "atpA", []),     # F0F1 ATP synthase subunit alpha
    ("atpD", "atpD", []),     # F0F1 ATP synthase subunit beta
    # ── biofilm EPS matrix (the disinfectant-resistant armour) ──
    ("pelA", "pelA", []),     # Pel exopolysaccharide biosynthesis (glycoside hydrolase / deacetylase)
    ("pelF", "pelF", []),     # Pel exopolysaccharide glycosyltransferase
    ("pslA", "pslA", []),     # Psl exopolysaccharide biosynthesis
    ("pslD", "pslD", []),     # Psl exopolysaccharide export
    ("algD", "algD", []),     # GDP-mannose 6-dehydrogenase (alginate biosynthesis)
    ("algU", "algU", []),     # alginate biosynthesis sigma factor AlgU/AlgT (mucoid switch)
    # ── quorum sensing + efflux + oxidative defence (the cull-resistance levers) ──
    ("lasR", "lasR", []),     # LasR quorum-sensing master regulator
    ("rhlR", "rhlR", []),     # RhlR quorum-sensing regulator
    ("pqsA", "pqsA", []),     # PQS (Pseudomonas quinolone signal) biosynthesis anthranilate-CoA ligase
    ("mexB", "mexB", []),     # MexB RND multidrug-efflux transporter (the disinfectant/antibiotic pump)
    ("oprM", "oprM", []),     # OprM outer-membrane efflux channel
    ("katA", "katA", []),     # catalase (oxidative-stress / disinfectant defence)
    ("sodB", "sodB", []),     # iron superoxide dismutase (oxidative defence)
    ("phzM", "phzM", []),     # pyocyanin biosynthesis O-methyltransferase (redox-active phenazine virulence)
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "pseudomonas.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "pseudomonas.json"


def fetch(url: str) -> bytes:
    print(f"» fetch {url}", file=sys.stderr)
    req = urllib.request.Request(url, headers={"User-Agent": "gene-sim-bake/1.0"})
    with urllib.request.urlopen(req, timeout=180) as r:  # noqa: S310 (pinned trusted host)
        return r.read()


def ncbi_cds() -> dict[str, str]:
    """{gene_symbol: CDS sequence} from the NCBI cds_from_genomic FASTA (first occurrence wins)."""
    raw = gzip.GzipFile(fileobj=io.BytesIO(fetch(NCBI_CDS_URL))).read().decode("ascii")
    by_gene: dict[str, str] = {}
    gene, seq = None, []
    for line in raw.splitlines():
        if line.startswith(">"):
            if gene is not None and gene not in by_gene:
                by_gene[gene] = "".join(seq)
            seq = []
            gene = None
            m = re.search(r"\[gene=([^\]]*)\]", line)
            if m:
                gene = m.group(1).strip()
        else:
            seq.append(line.strip())
    if gene is not None and gene not in by_gene:
        by_gene[gene] = "".join(seq)
    print(f"» NCBI CDS: {len(by_gene)} gene-named sequences", file=sys.stderr)
    return by_gene


def main() -> int:
    cds = ncbi_cds()

    loci = []
    missing = []
    for sym, name, go_refs in ROSTER:
        seq = cds.get(sym, "").upper()
        if not seq:
            missing.append(sym)
            continue
        bad = next((i for i, c in enumerate(seq) if c not in "ACGT"), None)
        if bad is not None:
            print(f"  ! {sym} ({name}) has non-ACGT base at {bad}; skipping", file=sys.stderr)
            missing.append(sym)
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
        "key": "pseudomonas",
        "name": "Pseudomonas aeruginosa PAO1",
        "niche": {
            "entity_count": 120,  # contaminants seed SPARSE — immigration pressure, not a founding population
            "description": (
                f"Curated contaminant-anchor roster x CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: "
                f"the central-metabolism backbone (gltA/acnB/icd/sdhA + ATP synthase — the generalist's broad "
                f"carbon catabolism) + the Pel/Psl/alginate biofilm EPS machinery (the disinfectant-resistant "
                f"matrix) + the quorum-sensing (lasR/rhlR/pqsA) and multidrug-efflux (mexB/oprM) + oxidative-"
                f"defence (katA/sodB) apparatus (the cull-resistance levers). P. aeruginosa PAO1: 6,264,404 bp / "
                f"5,570 ORFs; grows in distilled water, largest regulatory-gene set of any genome then sequenced, "
                f"EPS biofilm resists disinfectants -> defeats the nutrient-starvation + disinfectant barrier. The "
                f"immigration kernel reads role + trait levers, not specific genes (ADR-019 S0, Mode A contaminant)."
            ),
            "temp_optimum": 0.58,  # mesophile, broad range to ~42 C — normalized into the sim's [0,1] band
            "parent_key": None,
            "trophic_role": "mixotroph",  # the DATA-driven role override (gp::role_from_override -> Mixotroph)
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
