#!/usr/bin/env python3
"""Bake data/species/penicillium.json — a curated representative-locus set of the real Penicillium rubens
Wisconsin 54-1255 CONTAMINANT genome (a EUKARYOTE) as a gene-sim SpeciesSpec (ADR-019 S0, Mode A — the most
common indoor/outdoor airborne mold, and the penicillin origin organism). Mirrors
scripts/bake_mycoplasma_species.py / bake_bacillus_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Penicillium species file: it reads ONE pinned public source and emits a byte-identical penicillium.json on
every run.

WHY PENICILLIUM IS A KEYSTONE CONTAMINANT (contamination-immigration-draft §4 Mode A):
  Penicillium is the single MOST COMMON mold genus in indoor AND outdoor air samples [van den Berg et al.
  2008, Nat Biotechnol 26:1161], so it is the archetypal airborne saprotroph contaminant — a landed spore
  germinates, hyphae spread, sporulates, takes the plate ('lab weeds'). It is also THE penicillin origin
  organism (Fleming's 1928 contaminant; Wisconsin 54-1255 is the industrial production lineage). As an
  osmotrophic saprotroph it externally digests and mineralizes detritus → `niche.trophic_role = "decomposer"`
  (the data-driven gp::role_from_override -> Decomposer), the same detritus -> free_nutrient flow class
  E. coli, B. subtilis, C. acnes and A. niger already occupy.

GENOME PROVENANCE (verified, primary-sourced):
  - 32.19 Mb / 12,943 genes [van den Berg et al. 2008, Nat Biotechnol 26:1161, the Wisconsin 54-1255
    reference]. The current RefSeq reference assembly (a re-sequenced Wisconsin 54-1255) carries ~11,613 CDS
    in cds_from_genomic.
  - Reference assembly (pinned): NCBI RefSeq GCF_028828025.1 (ASM2882802v1), public domain. (Penicillium
    chrysogenum Wisconsin 54-1255 was reclassified P. rubens; this is the current P. rubens reference.)

WHY A CURATED ANCHOR ROSTER (NOT genome-complete — this is a EUKARYOTE): the task is explicit that for a
~32 Mb / ~13k-gene eukaryote the bake is a CURATED ANCHOR ROSTER, not the whole genome — and the immigration
MECHANIC reads role + trait levers, NOT specific genes (contamination-immigration-draft §5.1). So a small,
faithful representative set is sufficient: (a) the CONIDIATION / AIRBORNE-SPORE cascade (the central
regulators brlA -> abaA -> wetA, the upstream fluG/flbA, a rodlet spore-wall hydrophobin) — the airborne-
propagule machinery; (b) the conidial PIGMENT polyketide synthase (pksP/alb1 — spore pigment, UV/desiccation
resistance); (c) the PENICILLIN-biosynthesis cluster (pcbC isopenicillin-N synthase + penDE isopenicillin-N
acyltransferase — the penicillin origin story + a chemical-warfare lever vs. bacterial competitors); (d) the
saprotroph housekeeping (gpdA, niaD nitrate assimilation). Each curated locus is a REAL Wisconsin-54-1255
spliced CDS (pure ACGT). The cascade is selected by `[gene=...]` symbol; the penicillin/hydrophobin loci by
RefSeq LOCUS_TAG (a hybrid gene-symbol-OR-locus_tag selector), keeping the bake reproducible.

The roster ships with empty go_refs (the contaminant is inert DATA on disk until an immigration config
references it — hash-neutral per ADR-019 S0; no sim-core TraitMap binds it in this slice). The niche declares
`trophic_role` + the entity_count.

Run (needs network):  python3 scripts/bake_penicillium_species.py
Then commit data/species/penicillium.json + the godot mirror; the harness gate test
`shipped_penicillium_species_loads` enforces it builds + round-trips + declares the decomposer role.
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
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/028/828/025/"
    "GCF_028828025.1_ASM2882802v1/GCF_028828025.1_ASM2882802v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_028828025.1 ASM2882802v1"
SO_GENE = 704  # SO:0000704 "gene"

# ── CURATED CONTAMINANT-ANCHOR ROSTER ────────────────────────────────────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (selector, display name, [go_refs]) where `selector`
# is matched FIRST against `[gene=...]`, then against `[locus_tag=...]` (the conidiation cascade is gene-named;
# the penicillin-biosynthesis + hydrophobin loci are pinned by RefSeq locus_tag). All ship with empty go_refs
# in S0 (hash-neutral). The set: the airborne-spore cascade + spore pigment + penicillin cluster + housekeeping.
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── conidiation / airborne-spore cascade (the airborne-propagule machinery) ──
    ("fluG", "fluG", []),               # FluG — conidiation initiation signal
    ("flbA", "flbA", []),               # FlbA — RGS regulator of conidiation
    ("brlA", "brlA", []),               # BrlA — central conidiophore-development master regulator
    ("abaA", "abaA", []),               # AbaA — phialide-stage developmental regulator (brlA->abaA->wetA)
    ("wetA", "wetA", []),               # WetA — conidial maturation / spore-wall regulator
    ("N7525_002274", "rodA_hyd", []),   # rodlet spore-wall hydrophobin (surface/airborne hydrophobicity)
    # ── conidial pigment (UV / desiccation-resistant spores) ──
    ("pksP", "pksP", []),               # PksP/alb1 polyketide synthase — conidial pigment biosynthesis
    # ── penicillin biosynthesis (the penicillin origin story + chemical warfare vs. bacteria) ──
    ("N7525_001231", "pcbC_ipns", []),  # isopenicillin N synthase (IPNS) — penicillin biosynthesis
    ("N7525_006406", "penDE_iat", []),  # isopenicillin N acyltransferase (IAT) — final penicillin step
    # ── saprotroph housekeeping ──
    ("gpdA", "gpdA", []),               # glyceraldehyde-3-phosphate dehydrogenase (glycolysis housekeeping)
    ("niaD", "niaD", []),               # nitrate reductase — nitrate assimilation (broad N niche)
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "penicillium.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "penicillium.json"


def fetch(url: str) -> bytes:
    print(f"» fetch {url}", file=sys.stderr)
    req = urllib.request.Request(url, headers={"User-Agent": "gene-sim-bake/1.0"})
    with urllib.request.urlopen(req, timeout=240) as r:  # noqa: S310 (pinned trusted host)
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
        "key": "penicillium",
        "name": "Penicillium rubens Wisconsin 54-1255",
        "niche": {
            "entity_count": 120,  # contaminants seed SPARSE — immigration pressure, not a founding population
            "description": (
                f"Curated EUKARYOTE contaminant-anchor roster x spliced CDS {ASSEMBLY} (NCBI, public domain). "
                f"{len(loci)} representative loci (NOT genome-complete — a ~32 Mb / ~13k-gene mold): the "
                f"conidiation / airborne-spore cascade (fluG/flbA, brlA->abaA->wetA, a rodlet spore-wall "
                f"hydrophobin) + the pksP conidial-pigment PKS + the penicillin-biosynthesis cluster (pcbC "
                f"isopenicillin-N synthase, penDE acyltransferase — the penicillin origin story + chemical "
                f"warfare vs. bacteria) + saprotroph housekeeping (gpdA, niaD nitrate assimilation). "
                f"P. rubens (chrysogenum) Wisconsin 54-1255: 32.19 Mb / 12,943 genes; the single most common mold "
                f"genus in indoor + outdoor air, and the penicillin origin organism. The immigration kernel reads "
                f"role + trait levers, not specific genes (ADR-019 S0, Mode A contaminant)."
            ),
            "temp_optimum": 0.5,  # mesophilic mold, optimum ~23-25 C — normalized into the sim's [0,1] band
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
