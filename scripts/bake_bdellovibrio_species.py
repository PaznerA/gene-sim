#!/usr/bin/env python3
"""Bake data/species/bdellovibrio.json — the real Bdellovibrio bacteriovorus HD100 PREDATOR genome as a
gene-sim SpeciesSpec (ADR-013 F6). Mirrors scripts/bake_ecoli_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Bdellovibrio species file: it reads ONE pinned public source and emits a byte-identical bdellovibrio.json on
every run.

Source (pinned):
  NCBI RefSeq GCF_000196175.1 (ASM19617v1) cds_from_genomic — the real B. bacteriovorus HD100 CDS, public domain.

WHY A CURATED ROSTER (not genome-complete like the 136-gene E. coli file): Bdellovibrio HD100 has NO BiGG core
model (the e_coli_core 136-locus roster was a BiGG artifact). The predation MECHANIC reads role + the attack-rate
lever (PredationCapacity), NOT specific genes, so a hand-curated ~14-locus set of the host-interaction / lytic
attack machinery + the TCA backbone is sufficient and faithful. Each curated locus is a REAL HD100 CDS (pure
ACGT) carrying the right GO anchor; non-anchor curated loci ship with empty go_refs.

The TraitMap (gp::bdellovibrio_trait_map, B-style wiring) binds:
  GrowthRate         → GO:0004108 (citrate synthase, gltA — the growth backbone, same anchor E. coli uses)
  PredationCapacity  → GO:0008745 (N-acetylmuramoyl-L-alanine amidase / peptidoglycan muralytic activity — the
                        lytic attack machinery; a `hit`-locus CRISPRi Knockdown throttles the attack rate)

Output: a `genome::spec::SpeciesSpec` JSON (same shape as data/species/ecoli.json) with one locus per curated
gene {id==index in roster order, name=gene/role label, sequence=real HD100 CDS (pure ACGT), tags.so_term=704
"gene", tags.go_refs=curated GO MF, one Numeric activity param value 1.0 in [0,1] (1.0=wild-type, 0=knockout)}.

Run (needs network):  python3 scripts/bake_bdellovibrio_species.py
Then commit data/species/bdellovibrio.json + godot/data/species/bdellovibrio.json (renderer mirror); the harness
gate test `shipped_bdellovibrio_species_loads` enforces it builds.

CURATED FALLBACK: if the HD100 fetch is networkless/flaky, the same role/predation machinery is byte-identical
off short synthetic-but-valid ACGT loci carrying the same GO anchors (the determinism/role machinery reads
role + GO, not the specific bases). Hand-author such a file under the same key/niche and the gate still passes.
"""

from __future__ import annotations

import gzip
import io
import json
import sys
import urllib.request
from pathlib import Path

# ── PINNED SOURCE (inv #7 — change only deliberately; a different pin re-bakes a different file) ──────────────
NCBI_CDS_URL = (
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/196/175/"
    "GCF_000196175.1_ASM19617v1/GCF_000196175.1_ASM19617v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000196175.1 ASM19617v1"
SO_GENE = 704  # SO:0000704 "gene"

GO_GROWTH = 4108  # GO:0004108 citrate synthase (gltA) — the GrowthRate anchor
GO_PREDATION = 8745  # GO:0008745 N-acetylmuramoyl-L-alanine amidase (peptidoglycan muralytic) — PredationCapacity

# ── CURATED PREDATION-ANCHOR ROSTER (by RefSeq locus_tag) ──────────────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (locus_tag, display name, [go_refs]). The two ANCHOR
# genes carry their GO MF id; the rest of the lytic/attack/pilus machinery ships with empty go_refs (they are the
# real host-range apparatus, present for fidelity, but only the anchored loci drive the TraitMap). Curated from
# the live HD100 annotation (protein= descriptions): the TCA backbone gltA, the EnvC/amidase/lytic-transglycosylase
# peptidoglycan-remodeling machinery (the periplasm-invasion + bdelloplast-rounding apparatus), the type-IV pilus
# (attachment/retraction), and the host-attachment protein.
ROSTER: list[tuple[str, str, list[int]]] = [
    ("BD_RS02565", "gltA", [GO_GROWTH]),            # citrate synthase — growth backbone (GrowthRate anchor)
    ("BD_RS02735", "amiB_like", [GO_PREDATION]),    # N-acetylmuramoyl-L-alanine amidase — the ATTACK lever
    ("BD_RS00785", "envC", []),                     # murein hydrolase activator EnvC (peptidoglycan remodeling)
    ("BD_RS02415", "mltA_like", []),                # lytic transglycosylase (cell-wall hydrolase)
    ("BD_RS05835", "mltB_like", []),                # lytic transglycosylase (cell-wall hydrolase)
    ("BD_RS04755", "mepM_like", []),                # murein endopeptidase (peptidoglycan-remodeling)
    ("BD_RS03575", "ldtA_like", []),                # murein L,D-transpeptidase
    ("BD_RS00615", "mltG", []),                     # endolytic transglycosylase MltG
    ("BD_RS06930", "hit", []),                      # host attachment protein (the host-interaction locus)
    ("BD_RS03970", "pilQ", []),                     # type IV pilus secretin PilQ (attachment/retraction)
    ("BD_RS03950", "pilM", []),                     # type IV pilus assembly protein PilM
    ("BD_RS01150", "pilV", []),                     # type IV pilus modification PilV (gliding/attack)
    ("BD_RS00535", "cpaB", []),                     # Flp pilus assembly protein CpaB
    ("BD_RS05165", "lysM", []),                     # LysM peptidoglycan-binding domain protein
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "bdellovibrio.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "bdellovibrio.json"


def fetch(url: str) -> bytes:
    print(f"» fetch {url}", file=sys.stderr)
    req = urllib.request.Request(url, headers={"User-Agent": "gene-sim-bake/1.0"})
    with urllib.request.urlopen(req, timeout=120) as r:  # noqa: S310 (pinned trusted host)
        return r.read()


def ncbi_cds() -> dict[str, str]:
    """{locus_tag: CDS sequence} from the NCBI cds_from_genomic FASTA."""
    raw = gzip.GzipFile(fileobj=io.BytesIO(fetch(NCBI_CDS_URL))).read().decode("ascii")
    by_tag: dict[str, str] = {}
    tag, seq = None, []
    for line in raw.splitlines():
        if line.startswith(">"):
            if tag is not None:
                by_tag[tag] = "".join(seq)
            seq = []
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
            print(f"  ! {tag} ({name}) has non-ACGT base at {bad}; skipping", file=sys.stderr)
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
    # The two anchors MUST be present or the TraitMap can't bind — fail loudly (deterministic provenance).
    anchored = {go for locus in loci for go in locus["tags"]["go_refs"]}
    for need in (GO_GROWTH, GO_PREDATION):
        if need not in anchored:
            print(f"» FATAL: anchor GO:{need:07d} absent from the baked roster", file=sys.stderr)
            return 1

    spec = {
        "format_version": 1,
        "key": "bdellovibrio",
        "name": "Bdellovibrio bacteriovorus HD100",
        "niche": {
            "entity_count": 180,  # predators start SPARSE — dense seeding instant-crashes prey then itself
            "description": (
                f"Curated predation-anchor roster × CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: the "
                f"host-interaction / lytic attack machinery (peptidoglycan amidase/transglycosylase, type-IV pilus, "
                f"host-attachment) + the TCA backbone gltA. The predation kernel reads role + the PredationCapacity "
                f"attack-rate lever (GO:0008745), not specific genes."
            ),
            "temp_optimum": 0.6,  # HD100 grows ~30 °C, the same band as E. coli — normalized into the sim's [0,1]
            "parent_key": None,
            "trophic_role": "predator",  # the DATA-driven role override (gp::role_from_override → Predator)
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
