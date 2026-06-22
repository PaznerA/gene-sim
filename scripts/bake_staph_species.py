#!/usr/bin/env python3
"""Bake data/species/staph.json — the real Staphylococcus epidermidis ATCC 12228 CONTAMINANT genome as a
gene-sim SpeciesSpec (ADR-019 S0, Mode A — the skin-flora biofilm contaminant the human operator carries in).
Mirrors scripts/bake_mycoplasma_species.py / bake_bacillus_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Staphylococcus species file: it reads ONE pinned public source and emits a byte-identical staph.json on
every run.

WHY STAPHYLOCOCCUS EPIDERMIDIS IS A KEYSTONE CONTAMINANT (contamination-immigration-draft §4 Mode A):
  S. epidermidis defeats the barrier of the HUMAN OPERATOR — it is the dominant normal flora of human skin,
  so it is the most easily operator-introduced contaminant (every touch reseeds it) and the #1 cause of
  indwelling-device biofilm infections [PMC2777538]. It is a skin commensal heterotroph that holds the niche
  by surface attachment → `niche.trophic_role = "heterotroph"` (the data-driven gp::role_from_override ->
  Heterotroph), the same flow class as the other heterotrophic commensals.

GENOME PROVENANCE (verified, primary-sourced):
  - 2,499,279 bp (ATCC 12228, the original Zhang et al. 2003 reference; the draft cites ~2,570,371 bp /
    2,462 CDS for the species-level figure). The pinned RefSeq annotation (NC_004461.1 / ASM764v1) carries
    2,366 CDS in cds_from_genomic. NB: ATCC 12228 is the canonical ICA-NEGATIVE reference strain (it lacks
    the icaADBC polysaccharide-intercellular-adhesin operon) — so this roster anchors biofilm/attachment on
    the SURFACE-ADHESIN apparatus it DOES carry (Aap, the Sdr serine-aspartate-repeat adhesins, the giant
    Ebh) rather than the absent ica operon. This is a faithful, evidence-correct curation of THIS strain.
  - Reference assembly (pinned): NCBI RefSeq GCF_000007645.1 (ASM764v1), public domain.

WHY A CURATED ROSTER (not genome-complete): like bdellovibrio.json / mycoplasma.json / bacillus.json, the
immigration MECHANIC reads role + trait levers, NOT specific genes (contamination-immigration-draft §5.1).
So a hand-curated set is sufficient and faithful: (a) the central-metabolism backbone (glycolysis gap/eno/pyk
+ TCA icd/sdh/suc/fumC + ATP synthase + the pdhA/pflB/pta fermentation route — the skin-commensal's facultative
metabolism); (b) the SURFACE-ADHESIN / attachment apparatus (aap accumulation-associated protein, the sdrG/
sdrF/sdrH Sdr adhesins, the giant cell-wall-anchored Ebh, the gehC lipase — the host-grip / colonization
machinery of the operator-introduced commensal). Each curated locus is a REAL ATCC 12228 CDS (pure ACGT),
selected by its `[gene=...]` annotation (robust across re-annotations; first occurrence wins).

The roster ships with empty go_refs (the contaminant is inert DATA on disk until an immigration config
references it — hash-neutral per ADR-019 S0; no sim-core TraitMap binds it in this slice, exactly as the
non-anchor bdellovibrio loci ship with empty go_refs). The niche declares `trophic_role` + the entity_count.

Run (needs network):  python3 scripts/bake_staph_species.py
Then commit data/species/staph.json + godot/data/species/staph.json (renderer mirror); the harness gate test
`shipped_staph_species_loads` enforces it builds + round-trips + declares the heterotroph role.
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
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/007/645/"
    "GCF_000007645.1_ASM764v1/GCF_000007645.1_ASM764v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000007645.1 ASM764v1"
SO_GENE = 704  # SO:0000704 "gene"

# ── CURATED CONTAMINANT-ANCHOR ROSTER (by RefSeq [gene=...] symbol) ──────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (gene_symbol, display name, [go_refs]). All ship with
# empty go_refs in S0 (no sim-core TraitMap binds a contaminant yet — hash-neutral). Curated from the live ATCC
# 12228 RefSeq annotation: the facultative central-metabolism backbone + the SURFACE-ADHESIN / attachment
# apparatus (Aap / the Sdr adhesins / the giant Ebh / the gehC lipase — the operator-introduced commensal's
# host-grip / device-biofilm colonization machinery; this strain is ica-negative, so attachment anchors here).
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── central metabolism (glycolysis + TCA + ATP + fermentation — the facultative commensal's energy) ──
    ("gap", "gap", []),       # glyceraldehyde-3-phosphate dehydrogenase (glycolysis)
    ("eno", "eno", []),       # enolase (glycolysis)
    ("pyk", "pyk", []),       # pyruvate kinase (glycolysis, ATP-yielding step)
    ("icd", "icd", []),       # isocitrate dehydrogenase (TCA)
    ("sdhA", "sdhA", []),     # succinate dehydrogenase flavoprotein (TCA / aerobic respiration)
    ("sucC", "sucC", []),     # succinyl-CoA synthetase beta (TCA)
    ("fumC", "fumC", []),     # fumarase (TCA)
    ("atpA", "atpA", []),     # F0F1 ATP synthase subunit alpha
    ("atpD", "atpD", []),     # F0F1 ATP synthase subunit beta
    ("pdhA", "pdhA", []),     # pyruvate dehydrogenase E1 alpha (pyruvate -> acetyl-CoA)
    ("pflB", "pflB", []),     # pyruvate formate-lyase (anaerobic fermentation route)
    ("pta", "pta", []),       # phosphate acetyltransferase (acetate overflow)
    # ── surface adhesins / attachment (the operator-introduced commensal's host-grip machinery) ──
    ("aap", "aap", []),       # accumulation-associated protein (biofilm accumulation, ica-independent)
    ("sdrG", "sdrG", []),     # Sdr serine-aspartate-repeat adhesin G (fibrinogen-binding)
    ("sdrF", "sdrF", []),     # Sdr adhesin F (collagen / device-surface binding)
    ("sdrH", "sdrH", []),     # Sdr adhesin H
    ("ebh", "ebh", []),       # giant cell-wall-anchored adhesin Ebh
    ("gehC", "gehC", []),     # glycerol-ester hydrolase / lipase (skin-lipid / sebum utilization)
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "staph.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "staph.json"


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
        "key": "staph",
        "name": "Staphylococcus epidermidis ATCC 12228",
        "niche": {
            "entity_count": 120,  # contaminants seed SPARSE — immigration pressure, not a founding population
            "description": (
                f"Curated contaminant-anchor roster x CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: "
                f"the facultative central-metabolism backbone (glycolysis gap/eno/pyk + TCA icd/sdhA/sucC/fumC + "
                f"ATP synthase + the pdhA/pflB/pta fermentation route) + the surface-adhesin / attachment "
                f"apparatus (aap, the sdrG/sdrF/sdrH Sdr adhesins, the giant Ebh, the gehC lipase — the host-grip "
                f"machinery). S. epidermidis ATCC 12228: ~2.5 Mb / ~2,400 CDS; dominant normal skin flora -> the "
                f"most easily operator-introduced contaminant, #1 indwelling-device biofilm. (ATCC 12228 is the "
                f"canonical ica-negative reference strain, so attachment anchors on its Aap/Sdr/Ebh apparatus.) "
                f"The immigration kernel reads role + trait levers, not specific genes (ADR-019 S0, Mode A)."
            ),
            "temp_optimum": 0.6,  # skin commensal ~30-37 C — normalized into the sim's [0,1] temperature band
            "parent_key": None,
            "trophic_role": "heterotroph",  # the DATA-driven role override (gp::role_from_override -> Heterotroph)
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
