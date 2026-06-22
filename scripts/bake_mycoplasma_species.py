#!/usr/bin/env python3
"""Bake data/species/mycoplasma.json — the real Mycoplasma genitalium G37 CONTAMINANT genome as a gene-sim
SpeciesSpec (ADR-019 S0, Mode A — the silent filter-passing parasite). Mirrors scripts/bake_bdellovibrio_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Mycoplasma species file: it reads ONE pinned public source and emits a byte-identical mycoplasma.json on every run.

WHY MYCOPLASMA IS THE FIRST CONTAMINANT (contamination-immigration-draft §4 Mode A):
  M. genitalium G37 has NO CELL WALL (class Mollicutes) → it DEFORMS through the 0.22 µm filters used to
  sterilize media AND is intrinsically resistant to cell-wall antibiotics (penicillin/β-lactams). It reaches
  10^8 organisms/mL WITHOUT clouding the medium — cryptic, chronic contamination; 15-35 % of continuous
  mammalian cell lines are estimated to be mycoplasma-contaminated [PMC10668599]. It is ALSO the famous
  minimal-cell template (the JCVI-Syn3.0 lineage descends from it). It is a host/serum-dependent parasite →
  `niche.trophic_role = "heterotroph"` (the data-driven gp::role_from_override → Heterotroph).

GENOME PROVENANCE (verified, primary-sourced):
  - First sequenced genome era: 580,070 bp, ~470-485 protein-coding genes [Fraser et al. 1995, Science 270:397;
    Glass et al. 2006, PNAS 16407165: 482 protein genes, 382 essential]. The modern RefSeq re-annotation
    (NC_000908.2 / ASM2732v1) carries 524 CDS in cds_from_genomic.
  - Reference assembly (pinned): NCBI RefSeq GCF_000027325.1 (ASM2732v1), public domain.

WHY A CURATED ROSTER (not genome-complete): like bdellovibrio.json, the immigration/contamination MECHANIC
reads role + trait levers, NOT specific genes (contamination-immigration-draft §5.1: "The kernel reads role +
trait levers, not specific genes"). So a hand-curated set of M. genitalium's defining apparatus is sufficient
and faithful: (a) its GLYCOLYSIS backbone — Mycoplasma has lost the TCA cycle and cytochromes, so ALL its ATP
comes from substrate-level phosphorylation in glycolysis (the metabolic identity of this minimal heterotroph);
and (b) its CYTADHERENCE / ADHESIN apparatus — the MgPa operon (P1/MgpB adhesin, P110/MgpC, P65, P32) that the
filter-passing parasite uses to grip host cells. Each curated locus is a REAL G37 CDS (pure ACGT).

The roster ships with empty go_refs (the contaminant is inert DATA on disk until an immigration config references
it — hash-neutral per ADR-019 S0; no sim-core TraitMap binds it in this slice, exactly as the non-anchor
bdellovibrio loci ship with empty go_refs). The niche declares `trophic_role` and the entity_count.

Output: a `genome::spec::SpeciesSpec` JSON (same shape as data/species/bdellovibrio.json) with one locus per
curated gene {id==index in roster order, name=gene/role label, sequence=real G37 CDS (pure ACGT), tags.so_term=704
"gene", tags.go_refs=[] (no anchor wired in S0), one Numeric activity param value 1.0 in [0,1]}.

Run (needs network):  python3 scripts/bake_mycoplasma_species.py
Then commit data/species/mycoplasma.json + godot/data/species/mycoplasma.json (renderer mirror); the harness gate
test `shipped_mycoplasma_species_loads` enforces it builds + round-trips + declares the heterotroph role.

──────────────────────────────────────────────────────────────────────────────────────────────────────────────
FOLLOW-UP BAKES (ADR-019 S0 Mode A roster — bake the same way when promoted; each is its own script/commit):
  - Bacillus subtilis 168       GCF_000009045.1  4,214,810 bp / ~4,100-4,300 CDS  → decomposer (endospore-former)
  - Pseudomonas aeruginosa PAO1 GCF_000006765.1  6,264,404 bp / 5,570 ORFs        → mixotroph  (biofilm oligotroph)
  - Staphylococcus epidermidis ATCC 12228  2,570,371 bp / 2,462 CDS               → heterotroph (skin commensal)
  - Cutibacterium acnes KPA171202          2,560,265 bp / 2,333 ORFs              → decomposer  (lipophilic anaerobe)
  - Aspergillus niger CBS 513.88           33.9 Mb / ~14,165 ORFs                 → decomposer  (fast hyphal mold)
  - Penicillium chrysogenum Wis 54-1255    32.19 Mb / 12,943 genes                → decomposer  (saprotroph mold)
A scaffolded Bacillus subtilis 168 baker (real reference assembly, decomposer role) ships as
scripts/bake_bacillus_species.py — run it to add data/species/bacillus.json the same way.
──────────────────────────────────────────────────────────────────────────────────────────────────────────────
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
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/027/325/"
    "GCF_000027325.1_ASM2732v1/GCF_000027325.1_ASM2732v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000027325.1 ASM2732v1"
SO_GENE = 704  # SO:0000704 "gene"

# ── CURATED CONTAMINANT-ANCHOR ROSTER (by RefSeq locus_tag) ──────────────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (locus_tag, display name, [go_refs]). All ship with
# empty go_refs in S0 (no sim-core TraitMap binds a contaminant yet — hash-neutral). Curated from the live G37
# RefSeq annotation (protein= descriptions): the GLYCOLYSIS backbone (Mycoplasma's sole ATP route — it lost the
# TCA cycle / cytochromes, the defining metabolism of this minimal heterotroph) + the CYTADHERENCE / MgPa adhesin
# apparatus (the host-grip machinery of the filter-passing parasite).
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── glycolysis (substrate-level phosphorylation — the parasite's only ATP source) ──
    ("MG_RS00115", "fba", []),          # class II fructose-1,6-bisphosphate aldolase
    ("MG_RS01225", "pfkA", []),         # ATP-dependent 6-phosphofructokinase
    ("MG_RS02565", "tpiA", []),         # triose-phosphate isomerase
    ("MG_RS01810", "gap", []),          # type I glyceraldehyde-3-phosphate dehydrogenase
    ("MG_RS01805", "pgk", []),          # phosphoglycerate kinase
    ("MG_RS02465", "eno", []),          # enolase (phosphopyruvate hydratase)
    ("MG_RS01230", "pyk", []),          # pyruvate kinase (ATP-yielding terminal step)
    ("MG_RS02710", "ldh", []),          # L-lactate dehydrogenase (NAD+ regeneration)
    ("MG_RS01800", "pta", []),          # phosphate acetyltransferase (acetate overflow)
    ("MG_RS02435", "atpA", []),         # F0F1 ATP synthase subunit alpha
    ("MG_RS02425", "atpD", []),         # F0F1 ATP synthase subunit beta
    # ── cytadherence / MgPa adhesin operon (the filter-passer's host-grip apparatus) ──
    ("MG_RS01075", "mgpB_p1", []),      # adhesin P1 / MgPa (the major cytadhesin)
    ("MG_RS01080", "mgpC_p110", []),    # adhesin P110 / MgpC (cytadherence accessory)
    ("MG_RS01275", "p65", []),          # cytadherence-associated protein P65
    ("MG_RS01895", "p32", []),          # adhesin P32
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "mycoplasma.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "mycoplasma.json"


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
    # The curated roster must be non-empty (a contaminant with no genome would be meaningless) — fail loudly.
    if not loci:
        print("» FATAL: no curated locus resolved to a real CDS — check the pinned assembly", file=sys.stderr)
        return 1

    spec = {
        "format_version": 1,
        "key": "mycoplasma",
        "name": "Mycoplasma genitalium G37",
        "niche": {
            "entity_count": 120,  # contaminants seed SPARSE — immigration pressure, not a founding population
            "description": (
                f"Curated contaminant-anchor roster × CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: "
                f"the glycolysis backbone (Mycoplasma's sole ATP route — it has lost the TCA cycle / cytochromes) + "
                f"the MgPa cytadherence/adhesin apparatus (the host-grip machinery of the filter-passing parasite). "
                f"M. genitalium G37: ~580 kb / ~470-485 genes; no cell wall (Mollicutes) -> passes 0.22 um filters, "
                f"intrinsically beta-lactam-resistant; reaches 10^8/mL non-turbid (cryptic contamination). The "
                f"immigration kernel reads role + trait levers, not specific genes (ADR-019 S0, Mode A contaminant)."
            ),
            "temp_optimum": 0.62,  # host/serum parasite ~37 C — normalized into the sim's [0,1] temperature band
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
