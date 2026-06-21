#!/usr/bin/env python3
"""Bake data/species/ecoli.json — the real E. coli K-12 core genome as a gene-sim SpeciesSpec (ADR-017 B-1).

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the E. coli
species file: it joins three PINNED public sources and emits a byte-identical ecoli.json on every run.

Sources (pinned):
  1. BiGG `e_coli_core` model  — the gene roster (~137 geneProducts = 136 real b-numbers + the s0001 placeholder).
     License: the model carries the UCSD academic non-commercial clause (human-accepted 2026-06-21); only the
     b-number ROSTER (a list of public locus tags) is read here, and the BiGG GPR/stoichiometry stays OUT of
     ecoli.json (it belongs in the Ring-1 sidecar / oracle-fba), keeping the shipped species file license-clean.
  2. NCBI RefSeq GCF_000005845.2 (ASM584v2) cds_from_genomic — the real K-12 MG1655 CDS, public domain.
  3. Curated GO molecular-function terms for the metabolic ANCHOR genes the microbe TraitMap (B-2) binds to.

Output: a `genome::spec::SpeciesSpec` JSON (same shape as data/species/default.json) with 136 per-gene loci,
each {id==index sorted by b-number, name=gene symbol, sequence=real CDS (pure ACGT), tags.so_term=704 "gene",
tags.go_refs=curated GO MF, one Numeric activity param value 1.0 in [0,1] (1.0=wild-type, 0=knockout)}.

Run (needs network):  python3 scripts/bake_ecoli_species.py
Then commit data/species/ecoli.json; the harness gate test `shipped_ecoli_species_loads` enforces it builds.
"""

from __future__ import annotations

import gzip
import io
import json
import sys
import urllib.request
from pathlib import Path

# ── PINNED SOURCES (inv #7 — change only deliberately; a different pin re-bakes a different file) ──────────────
BIGG_MODEL_URL = "http://bigg.ucsd.edu/static/models/e_coli_core.json"
NCBI_CDS_URL = (
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/005/845/"
    "GCF_000005845.2_ASM584v2/GCF_000005845.2_ASM584v2_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000005845.2 ASM584v2"
SO_GENE = 704  # SO:0000704 "gene"

# Curated GO molecular-function terms (bare GO numeric ids) for the metabolic ANCHOR genes the E. coli microbe
# TraitMap (B-2) binds via ByGoAnchor. Non-anchor genes ship with empty go_refs (GO enrichment is a follow-up).
ANCHOR_GO: dict[str, list[int]] = {
    "b0720": [4108],   # gltA  citrate synthase (GO:0004108)
    "b3916": [3872],   # pfkA  6-phosphofructokinase (GO:0003872)
    "b1723": [3872],   # pfkB  6-phosphofructokinase 2
    "b3956": [8964],   # ppc   PEP carboxylase (GO:0008964)
    "b1779": [4365],   # gapA  GAPDH (GO:0004365)
    "b1136": [4450],   # icd   isocitrate dehydrogenase (GO:0004450)
    "b4025": [4347],   # pgi   glucose-6-phosphate isomerase (GO:0004347)
    "b2297": [8959],   # pta   phosphate acetyltransferase (GO:0008959)
    "b2296": [8776],   # ackA  acetate kinase (GO:0008776)
    "b1380": [8720],   # ldhA  D-lactate dehydrogenase (GO:0008720)
    "b0903": [8861],   # pflB  pyruvate formate-lyase (GO:0008861)
    "b1101": [8982],   # ptsG  PTS glucose transporter (GO:0008982)
}

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "ecoli.json"


def fetch(url: str) -> bytes:
    print(f"» fetch {url}", file=sys.stderr)
    req = urllib.request.Request(url, headers={"User-Agent": "gene-sim-bake/1.0"})
    with urllib.request.urlopen(req, timeout=120) as r:  # noqa: S310 (pinned trusted hosts)
        return r.read()


def bigg_roster() -> list[tuple[str, str]]:
    """(b-number, gene symbol) pairs from the BiGG e_coli_core gene list, excluding the s0001 placeholder."""
    model = json.loads(fetch(BIGG_MODEL_URL))
    genes = [(g["id"], g.get("name") or g["id"]) for g in model["genes"]]
    roster = [(bnum, name) for (bnum, name) in genes if bnum.lower().startswith("b")]
    roster.sort(key=lambda gn: gn[0])  # stable order by b-number → deterministic locus ids
    print(f"» BiGG roster: {len(roster)} real genes ({len(genes) - len(roster)} non-b excluded)", file=sys.stderr)
    return roster


def ncbi_cds() -> dict[str, str]:
    """{locus_tag(b-number): CDS sequence} from the NCBI cds_from_genomic FASTA."""
    raw = gzip.GzipFile(fileobj=io.BytesIO(fetch(NCBI_CDS_URL))).read().decode("ascii")
    by_tag: dict[str, str] = {}
    tag, seq = None, []
    for line in raw.splitlines():
        if line.startswith(">"):
            if tag is not None:
                by_tag[tag] = "".join(seq)
            seq = []
            # header carries [locus_tag=b####]
            tag = None
            for field in line.split("["):
                if field.startswith("locus_tag="):
                    tag = field[len("locus_tag="):].rstrip("] ").strip()
                    break
        else:
            seq.append(line.strip())
    if tag is not None:
        by_tag[tag] = "".join(seq)
    print(f"» NCBI CDS: {len(by_tag)} locus_tag sequences", file=sys.stderr)
    return by_tag


def main() -> int:
    roster = bigg_roster()
    cds = ncbi_cds()

    loci = []
    missing = []
    for idx, (bnum, symbol) in enumerate(roster):
        seq = cds.get(bnum, "").upper()
        if not seq:
            missing.append(bnum)
            continue
        bad = next((i for i, c in enumerate(seq) if c not in "ACGT"), None)
        if bad is not None:
            print(f"  ! {bnum} ({symbol}) has non-ACGT base at {bad}; skipping", file=sys.stderr)
            missing.append(bnum)
            continue
        loci.append({
            "id": idx,
            "name": symbol,
            "sequence": seq,
            "parameters": [
                {"id": 0, "value": {"kind": "numeric", "value": 1.0, "min": 0.0, "max": 1.0}},
            ],
            "tags": {"so_term": SO_GENE, "go_refs": ANCHOR_GO.get(bnum, [])},
        })

    # Re-index so id == position after any skips (the SpeciesSpec builder asserts id==index).
    for i, locus in enumerate(loci):
        locus["id"] = i

    if missing:
        print(f"» WARNING: {len(missing)} roster genes had no usable CDS: {', '.join(missing)}", file=sys.stderr)

    spec = {
        "format_version": 1,
        "key": "ecoli-core",
        "name": "Escherichia coli K-12 core",
        "niche": {
            "entity_count": 800,
            "description": (
                f"BiGG e_coli_core roster × CDS {ASSEMBLY} (NCBI, public domain) × curated GO MF. "
                f"{len(loci)} genes. BiGG model carries the UCSD academic non-commercial clause; only the "
                f"b-number roster is used here (GPR/stoichiometry live in the Ring-1 sidecar)."
            ),
            "temp_optimum": 0.62,  # ~37 °C normalized into the sim's [0,1] temperature band
            "parent_key": None,
        },
        "genome": {"version": 2, "loci": loci},
    }

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(spec, indent=2) + "\n")
    print(f"» wrote {OUT} — {len(loci)} loci", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
