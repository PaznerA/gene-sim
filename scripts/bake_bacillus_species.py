#!/usr/bin/env python3
"""Bake data/species/bacillus.json — the real Bacillus subtilis 168 CONTAMINANT genome as a gene-sim
SpeciesSpec (ADR-019 S0, Mode A — the endospore-forming "survives anything" contaminant). Mirrors
scripts/bake_mycoplasma_species.py / bake_bdellovibrio_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Bacillus species file: it reads ONE pinned public source and emits a byte-identical bacillus.json on every run.

WHY BACILLUS SUBTILIS IS A KEYSTONE CONTAMINANT (contamination-immigration-draft §4 Mode A):
  B. subtilis 168 forms a heat/desiccation/UV/radiation-resistant ENDOSPORE (CaDPA ~25 % of core dry weight,
  multilayer coat) → dormant-but-viable for years to millennia [PMC99004; PLoS ONE 0208425, the 500-year
  experiment]. The cleanest rooms on Earth are not sterile: of 130 floor isolates over 6 months in NASA JPL's
  spacecraft-assembly cleanroom (Mars 2020 / Perseverance), 97 % were spore-formers [PMC8643001]. It is the
  contaminant that defeats the HEAT/STERILIZATION barrier, and the verified "cull alone fails — the dormant
  reservoir reseeds" dynamic (the §5.4 future re-pin). It is a generalist saprophyte/decomposer →
  `niche.trophic_role = "decomposer"` (the data-driven gp::role_from_override → Decomposer), the same detritus
  → free_nutrient flow class E. coli already occupies.

GENOME PROVENANCE (verified, primary-sourced):
  - 4,214,810 bp, ~4,100-4,300 protein-coding genes [Kunst et al. 1997, Nature 390:249, the first Gram-positive
    genome; Barbe et al. 2009 re-annotation]. The modern RefSeq annotation (NC_000964.3 / ASM904v1) carries
    4,237 CDS in cds_from_genomic.
  - Reference assembly (pinned): NCBI RefSeq GCF_000009045.1 (ASM904v1), public domain.

WHY A CURATED ROSTER (not genome-complete): like bdellovibrio.json / mycoplasma.json, the immigration MECHANIC
reads role + trait levers, NOT specific genes (contamination-immigration-draft §5.1). So a hand-curated set of
B. subtilis's defining apparatus is sufficient and faithful: (a) the TCA backbone citZ (citrate synthase, the
growth anchor); (b) the SPORULATION master-regulator + structural cascade (spo0A → sigF/sigE/sigG, spoIIE,
spoVG, the cotE coat morphogen, the sspA/sspB SASPs that armour spore DNA) — the dormancy apparatus that the
§5.4 spore/germination re-pin will eventually drive; and (c) the gerAA germination receptor (the reseeding
trigger). Each curated locus is a REAL 168 CDS (pure ACGT).

The roster ships with empty go_refs (the contaminant is inert DATA on disk until an immigration config references
it — hash-neutral per ADR-019 S0; no sim-core TraitMap binds it in this slice, exactly as the non-anchor
bdellovibrio loci ship with empty go_refs). The niche declares `trophic_role` and the entity_count.

Output: a `genome::spec::SpeciesSpec` JSON (same shape as data/species/mycoplasma.json) with one locus per
curated gene {id==index in roster order, name=gene/role label, sequence=real 168 CDS (pure ACGT), tags.so_term=704
"gene", tags.go_refs=[] (no anchor wired in S0), one Numeric activity param value 1.0 in [0,1]}.

Run (needs network):  python3 scripts/bake_bacillus_species.py
Then commit data/species/bacillus.json + godot/data/species/bacillus.json (renderer mirror); the harness gate
test `shipped_bacillus_species_loads` enforces it builds + round-trips + declares the decomposer role.
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
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/009/045/"
    "GCF_000009045.1_ASM904v1/GCF_000009045.1_ASM904v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000009045.1 ASM904v1"
SO_GENE = 704  # SO:0000704 "gene"

# ── CURATED CONTAMINANT-ANCHOR ROSTER (by RefSeq locus_tag) ──────────────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (locus_tag, display name, [go_refs]). All ship with
# empty go_refs in S0 (no sim-core TraitMap binds a contaminant yet — hash-neutral). Curated from the live 168
# RefSeq annotation (protein= descriptions): the TCA growth backbone citZ + the SPORULATION cascade (master
# regulator spo0A, the compartment sigma factors sigF/sigE/sigG, spoIIE, spoVG, the cotE coat morphogen, the
# sspA/sspB small acid-soluble spore proteins that armour spore DNA) + the gerAA germination receptor (the
# dormant-reservoir reseed trigger of the §5.4 future spore/germination re-pin) + the oxidative-defence catalase
# katA and superoxide dismutase sodA (desiccation/UV survival).
ROSTER: list[tuple[str, str, list[int]]] = [
    ("BSU_29140", "citZ", []),       # citrate synthase II — TCA growth backbone (the growth anchor)
    ("BSU_28440", "sdhA", []),       # succinate dehydrogenase (TCA, aerobic respiration)
    ("BSU_24220", "spo0A", []),      # sporulation master response regulator (the sporulation switch)
    ("BSU_23450", "sigF", []),       # forespore-specific sigma-F (first compartment sigma)
    ("BSU_15320", "sigE", []),       # mother-cell sigma-E
    ("BSU_15330", "sigG", []),       # late forespore sigma-G
    ("BSU_00640", "spoIIE", []),     # SpoIIE phosphatase (asymmetric-division checkpoint)
    ("BSU_00490", "spoVG", []),      # stage-V regulator (spore cortex synthesis)
    ("BSU_17030", "cotE", []),       # morphogenic spore-coat protein (coat assembly)
    ("BSU_29570", "sspA", []),       # alpha-type small acid-soluble spore protein (DNA armour)
    ("BSU_09750", "sspB", []),       # beta-type small acid-soluble spore protein (DNA armour)
    ("BSU_33050", "gerAA", []),      # GerA germination receptor (the reseed trigger)
    ("BSU_08820", "katA", []),       # vegetative catalase 1 (oxidative-stress / desiccation defence)
    ("BSU_25020", "sodA", []),       # Mn superoxide dismutase (UV / oxidative defence)
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "bacillus.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "bacillus.json"


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
    if not loci:
        print("» FATAL: no curated locus resolved to a real CDS — check the pinned assembly", file=sys.stderr)
        return 1

    spec = {
        "format_version": 1,
        "key": "bacillus",
        "name": "Bacillus subtilis 168",
        "niche": {
            "entity_count": 120,  # contaminants seed SPARSE — immigration pressure, not a founding population
            "description": (
                f"Curated contaminant-anchor roster × CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: the "
                f"TCA growth backbone citZ + the sporulation cascade (spo0A master regulator, sigF/sigE/sigG, "
                f"spoIIE, spoVG, the cotE coat morphogen, the sspA/sspB DNA-armour SASPs) + the gerAA germination "
                f"receptor + oxidative-defence katA/sodA. B. subtilis 168: 4,214,810 bp / ~4,100-4,300 genes; the "
                f"endospore (CaDPA ~25 % core dry wt, multilayer coat) survives heat/desiccation/UV/radiation for "
                f"years-millennia -> defeats the sterilization barrier (97 % of NASA cleanroom isolates were "
                f"spore-formers). The immigration kernel reads role + trait levers, not specific genes (ADR-019 S0)."
            ),
            "temp_optimum": 0.55,  # mesophile optimum ~30-37 C — normalized into the sim's [0,1] temperature band
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
