#!/usr/bin/env python3
"""Bake data/species/syn3.json — JCVI-Syn3.0, the DESIGNED minimal cell, as a gene-sim SpeciesSpec
(ADR-019 S5, Mode B). Mirrors scripts/bake_carsonella_species.py / bake_mycoplasma_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for syn3.json:
it reads ONE pinned public source and emits a byte-identical file on every run.

PROVENANCE HONESTY (the real subtlety — flagged in the ADR-019 S5 design, read it):
  JCVI-Syn3.0 is a DESIGNED minimal cell (531,560 bp / 473 genes, 149 of unknown function) [Hutchison/Gibson et
  al. 2016, Science 351:aad6253, "Design and synthesis of a minimal bacterial genome"]. It has NO single wild
  RefSeq assembly. Its chassis is *Mycoplasma mycoides* JCVI-syn1.0 (CP002027), itself derived from M. mycoides
  subsp. capri. Syn3.0 is the minimal essential/quasi-essential gene set of that lineage.
  ⇒ This baker builds from the *Mycoplasma genitalium* G37 minimal-cell reference (RefSeq GCF_000027325.1) — the
  SAME pinned source data/species/mycoplasma.json already uses, and the canonical small-genome template the
  Syn3.0 minimization lineage descends from — restricted to a curated minimal essential gene set (translation +
  glycolysis core + a one-carbon/amino-acid biosynthesis exchange anchor). The CDS are real ACGT G37 sequences;
  the script header documents this provenance honestly (we do NOT claim a wild Syn3.0 assembly exists).

ROLE HONESTY (the open question, signed-off PoC choice):
  Syn3.0 grows ONLY in a rich DEFINED medium — it is arguably substrate-dependent (Heterotroph) rather than
  host-dependent. For S5 we model it as `niche.trophic_role = "symbiont"` with the RICH-MEDIUM NICHE AS the
  "host" abstraction (`niche.host_key` names the medium/host-stand-in species), so it exercises the obligate
  host-coupling mechanic. (The taxonomy owner may instead ship Syn3.0 as a Heterotroph and keep Carsonella as the
  sole true symbiont — that is a one-line `trophic_role` edit + drop the host_key, no code change.)

THE S5 ANCHORS: the one-carbon/amino-acid biosynthesis exchange locus (serine hydroxymethyltransferase glyA)
carries GO:0008652 → Trait::SymbiosisCapacity → Strategy.host_draw_rate (the coupling lever; a CRISPRi knockdown
throttles it). The retained EF-Tu (`tuf`) locus carries GO:0006414 → Trait::GrowthRate. Every other locus ships
empty go_refs (inert DATA).

Run (needs network):  python3 scripts/bake_syn3_species.py
Then commit data/species/syn3.json + godot/data/species/syn3.json; the harness gate test
`shipped_syn3_species_loads` enforces it builds + round-trips + declares the obligate-symbiont role + host_key.
"""

from __future__ import annotations

import gzip
import io
import json
import re
import sys
import urllib.request
from pathlib import Path

# ── PINNED SOURCE (inv #7) — the M. genitalium G37 minimal-cell reference (the Syn3.0 lineage template). ──────
NCBI_CDS_URL = (
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/027/325/"
    "GCF_000027325.1_ASM2732v1/GCF_000027325.1_ASM2732v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000027325.1 ASM2732v1 (M. genitalium G37 — Syn3.0 minimal-cell template)"
SO_GENE = 704
GO_AA_BIOSYNTH = 8652  # GO:0008652 → SymbiosisCapacity (the medium/host-exchange coupling lever)
GO_TRANSLATION = 6414  # GO:0006414 → GrowthRate (the retained growth backbone)

# ── CURATED MINIMAL ESSENTIAL ROSTER (by RefSeq locus_tag) — the Syn3.0-style minimal gene set. ──────────────
# Order IS the locus order (id==index). The glyA provisioning/one-carbon anchor carries GO_AA_BIOSYNTH and tuf
# carries GO_TRANSLATION (the two S5 anchors symbiont_trait_map binds); every other locus ships empty go_refs.
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── translation / transcription / replication core (the minimal informational machinery) ──
    ("MG_RS02660", "tuf", [GO_TRANSLATION]),  # elongation factor Tu — the GrowthRate anchor
    ("MG_RS00495", "fusA", []),               # elongation factor G
    ("MG_RS00400", "rpsB", []),               # 30S ribosomal protein S2
    ("MG_RS00880", "rplB", []),               # 50S ribosomal protein L2
    ("MG_RS02085", "rpoC", []),               # RNA polymerase subunit beta'
    ("MG_RS02090", "rpoB", []),               # RNA polymerase subunit beta
    ("MG_RS00155", "polC", []),               # DNA polymerase III subunit alpha
    ("MG_RS01830", "dnaK", []),               # molecular chaperone DnaK
    ("MG_RS02385", "groL", []),               # chaperonin GroEL
    # ── glycolysis core (Syn3.0's sole ATP route — substrate-level phosphorylation, no TCA) ──
    ("MG_RS00115", "fba", []),                # fructose-1,6-bisphosphate aldolase
    ("MG_RS01810", "gap", []),                # glyceraldehyde-3-phosphate dehydrogenase
    ("MG_RS02465", "eno", []),                # enolase
    ("MG_RS01230", "pyk", []),                # pyruvate kinase
    ("MG_RS02565", "tpiA", []),               # triose-phosphate isomerase
    ("MG_RS00395", "ptsG", []),               # glucose-specific PTS transporter (the medium glucose tap)
    # ── one-carbon / amino-acid exchange (the medium/host-provisioning anchor) ──
    ("MG_RS02395", "glyA_exchange", [GO_AA_BIOSYNTH]),  # serine hydroxymethyltransferase — the SymbiosisCapacity anchor
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "syn3.json"
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "syn3.json"


def fetch(url: str) -> bytes:
    print(f"» fetch {url}", file=sys.stderr)
    req = urllib.request.Request(url, headers={"User-Agent": "gene-sim-bake/1.0"})
    with urllib.request.urlopen(req, timeout=120) as r:  # noqa: S310 (pinned trusted host)
        return r.read()


def ncbi_cds() -> dict[str, str]:
    raw = gzip.GzipFile(fileobj=io.BytesIO(fetch(NCBI_CDS_URL))).read().decode("ascii")
    by_tag: dict[str, str] = {}
    tag, seq = None, []
    for line in raw.splitlines():
        if line.startswith(">"):
            if tag is not None:
                by_tag[tag] = "".join(seq)
            seq = []
            tag = None
            m = re.search(r"\[locus_tag=([^\]]*)\]", line)
            if m:
                tag = m.group(1).strip()
        else:
            seq.append(line.strip())
    if tag is not None:
        by_tag[tag] = "".join(seq)
    print(f"» NCBI CDS: {len(by_tag)} locus_tag sequences", file=sys.stderr)
    return by_tag


def main() -> int:
    cds = ncbi_cds()
    loci = []
    missing = []
    for tag, name, go_refs in ROSTER:
        seq = cds.get(tag, "").upper()
        if not seq:
            missing.append(tag)
            continue
        bad = next((i for i, c in enumerate(seq) if c not in "ACGT"), None)
        if bad is not None:
            print(f"  ! {tag} ({name}) non-ACGT at {bad}; skipping", file=sys.stderr)
            missing.append(tag)
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
        print("» FATAL: no curated locus resolved — check the pinned assembly", file=sys.stderr)
        return 1
    anchor_names = {l["name"] for l in loci}
    if "glyA_exchange" not in anchor_names or "tuf" not in anchor_names:
        print("» FATAL: an S5 anchor (glyA_exchange / tuf) did not resolve", file=sys.stderr)
        return 1

    spec = {
        "format_version": 1,
        "key": "syn3",
        "name": "JCVI-Syn3.0 (minimal cell)",
        "niche": {
            "entity_count": 60,
            "description": (
                f"Curated minimal-essential roster × CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: the "
                f"translation/transcription/replication core (EF-Tu/EF-G, RNA pol, DNA pol III, ribosomal proteins, "
                f"DnaK/GroEL) + the glycolysis ATP core (Syn3.0 has no TCA) + a one-carbon/amino-acid exchange anchor "
                f"(glyA). JCVI-Syn3.0: a DESIGNED minimal cell, 531,560 bp / 473 genes (149 of unknown function) "
                f"[Hutchison/Gibson 2016, Science aad6253]. PROVENANCE: no wild Syn3.0 assembly exists — baked from the "
                f"M. genitalium G37 minimal-cell template (the Syn3.0 minimization lineage). Modeled as an obligate "
                f"symbiont with the RICH DEFINED MEDIUM as the host abstraction (it grows ONLY in rich medium). The "
                f"host-coupling kernel reads role + the SymbiosisCapacity lever (ADR-019 S5, Mode B)."
            ),
            "temp_optimum": 0.62,
            "parent_key": None,
            "trophic_role": "symbiont",  # gp::role_from_override -> ObligateSymbiont (signed-off PoC choice; see header)
            "host_key": "default",       # the rich-medium / host stand-in for the PoC
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
