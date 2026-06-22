#!/usr/bin/env python3
"""Bake data/species/aspergillus-niger.json — a curated representative-locus set of the real Aspergillus niger
CBS 513.88 CONTAMINANT genome (a EUKARYOTE) as a gene-sim SpeciesSpec (ADR-019 S0, Mode A — the fast hyphal
"black mold" that takes the plate). Mirrors scripts/bake_mycoplasma_species.py / bake_bacillus_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
A. niger species file: it reads ONE pinned public source and emits a byte-identical aspergillus-niger.json on
every run.

WHY ASPERGILLUS NIGER IS A KEYSTONE CONTAMINANT (contamination-immigration-draft §4 Mode A):
  Mold defeats NEARLY EVERY barrier — prolific airborne melanized conidia (you inhale hundreds of Aspergillus
  spores a day), a single landed spore germinates and its hyphae OUTGROW even Gram-negative bacteria
  (A. niger grows faster than E. coli on EMB and suppresses it by ACIDIFYING the medium), then sporulates and
  visibly takes the plate — "lab weeds" [Nierman 2005; Pel 2007, Nat Biotechnol 25:221; van den Berg 2008].
  ConsortiumConfig::default_mode_a references the `aspergillus-niger` key directly. As an osmotrophic
  saprotroph it externally digests and mineralizes detritus → `niche.trophic_role = "decomposer"` (the
  data-driven gp::role_from_override -> Decomposer), the same detritus -> free_nutrient flow class E. coli,
  B. subtilis and C. acnes already occupy.

GENOME PROVENANCE (verified, primary-sourced):
  - 33.9 Mb across 8 chromosomes / ~14,165 predicted ORFs [Pel et al. 2007, Nat Biotechnol 25:221, the
    CBS 513.88 reference — the citric-acid / industrial-enzyme cell factory]. The RefSeq annotation
    (GCF_000002855.3 / ASM285v2) carries ~10,609 CDS in cds_from_genomic.
  - Reference assembly (pinned): NCBI RefSeq GCF_000002855.3 (ASM285v2), public domain.

WHY A CURATED ANCHOR ROSTER (NOT genome-complete — this is a EUKARYOTE): the task is explicit that for a
33.9 Mb / ~14k-gene eukaryote the bake is a CURATED ANCHOR ROSTER, not the whole genome — and the immigration
MECHANIC reads role + trait levers, NOT specific genes (contamination-immigration-draft §5.1). So a small,
faithful representative set of A. niger's defining apparatus is sufficient: (a) the CONIDIATION / AIRBORNE-
SPORE cascade (the central regulators brlA -> abaA -> wetA, the upstream fluG and stuA, the dewA spore-wall
hydrophobin) — the airborne-propagule machinery that makes mold the universal contaminant; (b) the conidial
PIGMENT oxidase — the melanized "black mold" identity (UV/desiccation-resistant spores); (c) the SAPROTROPH /
black-mold-metabolism backbone (glaA glucoamylase = external starch digestion, the citrate synthase of the
citric-acid factory that acidifies out competitors, gpdA housekeeping, niaD nitrate assimilation). Each
curated locus is a REAL CBS 513.88 spliced CDS (pure ACGT). Because this annotation uses systematic
An##g##### names rather than standard gene symbols, loci are pinned by RefSeq LOCUS_TAG (a hybrid
gene-symbol-OR-locus_tag selector), keeping the bake reproducible.

The roster ships with empty go_refs (the contaminant is inert DATA on disk until an immigration config
references it — hash-neutral per ADR-019 S0; no sim-core TraitMap binds it in this slice). The niche declares
`trophic_role` + the entity_count.

Run (needs network):  python3 scripts/bake_aspergillus_niger_species.py
Then commit data/species/aspergillus-niger.json + the godot mirror; the harness gate test
`shipped_aspergillus_niger_species_loads` enforces it builds + round-trips + declares the decomposer role.
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
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/002/855/"
    "GCF_000002855.3_ASM285v2/GCF_000002855.3_ASM285v2_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000002855.3 ASM285v2"
SO_GENE = 704  # SO:0000704 "gene"

# ── CURATED CONTAMINANT-ANCHOR ROSTER ────────────────────────────────────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (selector, display name, [go_refs]) where `selector`
# is matched FIRST against `[gene=...]`, then against `[locus_tag=...]` (CBS 513.88 uses systematic An-style
# names, so anchors are pinned by RefSeq locus_tag). All ship with empty go_refs in S0 (hash-neutral). The set:
# the conidiation / airborne-spore cascade + the melanin pigment oxidase + the saprotroph metabolism backbone.
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── conidiation / airborne-spore cascade (the universal-contaminant propagule machinery) ──
    ("ANI_1_494124", "fluG", []),       # FluG — conidiation initiation signal
    ("ANI_1_378044", "stuA", []),       # StuA — APSES developmental regulator
    ("ANI_1_2984014", "brlA", []),      # BrlA — central conidiophore-development master regulator
    ("ANI_1_2446014", "abaA", []),      # AbaA — phialide-stage developmental regulator (brlA->abaA->wetA)
    ("ANI_1_1202014", "wetA", []),      # WetA — conidial maturation / spore-wall regulator
    ("ANI_1_266034", "dewA", []),       # DewA — spore-wall hydrophobin (rodlet layer, surface/airborne)
    # ── melanin pigment (the "black mold" identity — UV/desiccation-resistant conidia) ──
    ("ANI_1_1496124", "pigA", []),      # conidial pigment biosynthesis oxidase (Abr1/brown1 — melanin)
    # ── saprotroph / black-mold metabolism backbone ──
    ("ANI_1_820034", "glaA", []),       # glucoamylase — external starch digestion (osmotrophic saprotroph)
    ("ANI_1_876084", "citA", []),       # citrate synthase — the citric-acid factory that acidifies competitors
    ("ANI_1_256144", "gpdA", []),       # glyceraldehyde-3-phosphate dehydrogenase (glycolysis housekeeping)
    ("ANI_1_2088064", "niaD", []),      # nitrate reductase — nitrate assimilation (broad N niche)
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "aspergillus-niger.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = (
    Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "aspergillus-niger.json"
)


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
        "key": "aspergillus-niger",
        "name": "Aspergillus niger CBS 513.88",
        "niche": {
            "entity_count": 120,  # contaminants seed SPARSE — immigration pressure, not a founding population
            "description": (
                f"Curated EUKARYOTE contaminant-anchor roster x spliced CDS {ASSEMBLY} (NCBI, public domain). "
                f"{len(loci)} representative loci (NOT genome-complete — a 33.9 Mb / ~14k-gene mold): the "
                f"conidiation / airborne-spore cascade (fluG, stuA, brlA->abaA->wetA, the dewA spore-wall "
                f"hydrophobin) + the conidial melanin pigment oxidase (the 'black mold' identity) + the "
                f"saprotroph metabolism backbone (glaA glucoamylase = external starch digestion, the citA "
                f"citrate synthase of the citric-acid factory that acidifies out competitors, gpdA, niaD nitrate "
                f"assimilation). A. niger CBS 513.88: 33.9 Mb / 8 chr / ~14,165 ORFs; prolific airborne conidia "
                f"(you inhale hundreds daily), hyphae outgrow + acidify out bacteria -> takes the plate ('lab "
                f"weeds'). The immigration kernel reads role + trait levers, not specific genes (ADR-019 S0)."
            ),
            "temp_optimum": 0.62,  # thermotolerant mold, optimum ~35 C — normalized into the sim's [0,1] band
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
