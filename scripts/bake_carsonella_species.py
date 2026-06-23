#!/usr/bin/env python3
"""Bake data/species/carsonella.json — the real *Candidatus* Carsonella ruddii Pv obligate-ENDOSYMBIONT genome
as a gene-sim SpeciesSpec (ADR-019 S5, Mode B — the true obligate symbiont). Mirrors
scripts/bake_mycoplasma_species.py / bake_bacillus_species.py.

EVIDENCE-BASED + DETERMINISTIC + REPRODUCIBLE (inv #3, #7). This script is the SINGLE provenance for the
Carsonella species file: it reads ONE pinned public source and emits a byte-identical carsonella.json on every run.

WHY CARSONELLA IS THE TRUE OBLIGATE SYMBIONT (the ADR-019 S5 Mode-B mechanic):
  *Ca.* Carsonella ruddii is an obligate ENDOSYMBIONT confined to the bacteriocytes of sap-feeding psyllids. Its
  ~160 kb genome is one of the SMALLEST known for any cellular organism — it has SHED cell-envelope, nucleotide,
  lipid, and DNA-repair machinery, so it has NO free-living phase: it CANNOT free-live and survives only inside
  the host, which shelters it and supplies what it can no longer make. In exchange Carsonella retains a curated
  set of AMINO-ACID-BIOSYNTHESIS genes (the aromatic-amino-acid shikimate→chorismate pathway, leucine biosynthesis)
  that PROVISION the host's nutrient-poor phloem diet — the metabolite trade that JUSTIFIES the host coupling and
  is the codex story of genome reduction. → `niche.trophic_role = "symbiont"` (gp::role_from_override →
  ObligateSymbiont); `niche.host_key` names the species it draws its sole income from.

GENOME PROVENANCE (verified, primary-sourced):
  - *Ca.* Carsonella ruddii Pv: ~159,662 bp / ~182 ORFs / ~16.5 % GC / ~97.3 % coding [Nakabachi et al. 2006,
    Science 314:267, doi:10.1126/science.1134196 — "The 160-Kilobase Genome of the Bacterial Endosymbiont
    Carsonella"].
  - Reference assembly (pinned): NCBI RefSeq GCF_000010365.1 (ASM1036v1), public domain. (NOTE: an alternate
    accession GCF_000287275.1 / AP009180 exists for a different Carsonella strain; this baker pins
    GCF_000010365.1, VERIFIED live at bake time — 182 CDS, matching the Pv strain. Change the pin only deliberately,
    inv #7.)

WHY A CURATED ROSTER (not genome-complete): like the Mode-A contaminants, the host-coupling MECHANIC reads role +
the SymbiosisCapacity trait lever, NOT specific genes. So a hand-curated set of Carsonella's retained apparatus is
sufficient and faithful: (a) its TRANSLATION / REPLICATION CORE (ribosomal proteins, EF-Tu/EF-G, RNA polymerase,
DnaK/GroEL chaperones, DNA pol III) — the minimal machinery a reduced cell keeps; and (b) the AMINO-ACID-
BIOSYNTHESIS / PROVISIONING genes (shikimate kinase, 3-dehydroquinate synthase, chorismate synthase aroC, the
leuB/leuD leucine pathway, threonine synthase thrC, aspartate kinase / aspartate-semialdehyde dehydrogenase) —
the host-provisioning exchange that the obligate symbiosis is built on.

THE PROVISIONING ANCHOR (S5 coupling lever): the leucine/aromatic-amino-acid biosynthesis locus carries a single
go_ref GO:0008652 ("amino acid biosynthetic process"), the molecular-function anchor that gp::symbiont_trait_map
binds to Trait::SymbiosisCapacity → Strategy.host_draw_rate (so a provisioning-locus CRISPRi knockdown throttles
the coupling — the OVERSIGHT lever). The retained EF-Tu (`tuf`) locus carries GO:0006414 ("translational
elongation") → Trait::GrowthRate. Every OTHER locus ships with empty go_refs (inert DATA, like the contaminants).

Output: a `genome::spec::SpeciesSpec` JSON (same shape as data/species/bacillus.json) with one locus per curated
gene {id==index, name=gene/role label, sequence=real Carsonella CDS (pure ACGT), tags.so_term=704 "gene", one
Numeric activity param value 1.0 in [0,1]}, plus `niche.trophic_role="symbiont"` and `niche.host_key`.

Run (needs network):  python3 scripts/bake_carsonella_species.py
Then commit data/species/carsonella.json + godot/data/species/carsonella.json (renderer mirror); the harness gate
test `shipped_carsonella_species_loads` enforces it builds + round-trips + declares the obligate-symbiont role +
the host_key + the SymbiosisCapacity anchor.
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
    "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/010/365/"
    "GCF_000010365.1_ASM1036v1/GCF_000010365.1_ASM1036v1_cds_from_genomic.fna.gz"
)
ASSEMBLY = "GCF_000010365.1 ASM1036v1"
SO_GENE = 704  # SO:0000704 "gene"
GO_AA_BIOSYNTH = 8652  # GO:0008652 "amino acid biosynthetic process" → SymbiosisCapacity (the provisioning lever)
GO_TRANSLATION = 6414  # GO:0006414 "translational elongation" → GrowthRate (the retained growth backbone)

# ── CURATED OBLIGATE-SYMBIONT ROSTER (by RefSeq locus_tag) ───────────────────────────────────────────────────
# Order here IS the locus order (id==index). Each entry: (locus_tag, display name, [go_refs]). The provisioning
# (amino-acid-biosynthesis) anchor carries GO_AA_BIOSYNTH and EF-Tu carries GO_TRANSLATION (the two S5 anchors the
# symbiont_trait_map binds); every other locus ships empty go_refs. Curated from the live Pv RefSeq annotation:
# the retained TRANSLATION/REPLICATION core + the AMINO-ACID-PROVISIONING pathway (the host-exchange that the
# obligate symbiosis is built on).
ROSTER: list[tuple[str, str, list[int]]] = [
    # ── translation / replication core (the minimal machinery a reduced endosymbiont keeps) ──
    ("CRP_RS00870", "tuf", [GO_TRANSLATION]),  # elongation factor Tu — the GrowthRate anchor (retained backbone)
    ("CRP_RS00875", "fusA", []),               # elongation factor G
    ("CRP_RS00110", "dnaE", []),               # DNA polymerase III subunit alpha (replication core)
    ("CRP_RS00350", "dnaK", []),               # molecular chaperone DnaK
    ("CRP_RS00270", "groL", []),               # chaperonin GroEL
    ("CRP_RS00890", "rpoC", []),               # DNA-directed RNA polymerase subunit beta'
    ("CRP_RS00095", "rpsB", []),               # 30S ribosomal protein S2
    ("CRP_RS00850", "rplB", []),               # 50S ribosomal protein L2
    ("CRP_RS00180", "infA", []),               # translation initiation factor IF-1
    # ── amino-acid biosynthesis / host PROVISIONING (the metabolite trade that JUSTIFIES the coupling) ──
    ("CRP_RS00455", "leuB_provision", [GO_AA_BIOSYNTH]),  # 3-isopropylmalate dehydrogenase — the SymbiosisCapacity anchor
    ("CRP_RS00470", "leuD", []),               # 3-isopropylmalate dehydratase small subunit (leucine pathway)
    ("CRP_RS00460", "aroC", []),               # chorismate synthase (aromatic amino-acid pathway terminal step)
    ("CRP_RS00130", "aroK", []),               # shikimate kinase (aromatic amino-acid / shikimate pathway)
    ("CRP_RS00135", "aroB", []),               # 3-dehydroquinate synthase (shikimate pathway)
    ("CRP_RS00685", "thrC", []),               # threonine synthase
    ("CRP_RS01050", "lysC", []),               # aspartate kinase (aspartate-family amino-acid biosynthesis)
]

OUT = Path(__file__).resolve().parent.parent / "data" / "species" / "carsonella.json"
# The renderer reads species JSON from godot/data/species/ (a res:// mirror) — keep both in sync.
OUT_GODOT = Path(__file__).resolve().parent.parent / "godot" / "data" / "species" / "carsonella.json"


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
    # The two S5 anchors MUST have resolved (the coupling lever + the growth backbone) — fail loudly otherwise.
    anchor_names = {l["name"] for l in loci}
    if "leuB_provision" not in anchor_names or "tuf" not in anchor_names:
        print("» FATAL: an S5 anchor locus (leuB_provision / tuf) did not resolve — coupling lever missing",
              file=sys.stderr)
        return 1

    spec = {
        "format_version": 1,
        "key": "carsonella",
        "name": "Candidatus Carsonella ruddii Pv",
        "niche": {
            "entity_count": 60,  # symbionts seed SPARSE + host-gated — they establish only where a host is present
            "description": (
                f"Curated obligate-symbiont roster × CDS {ASSEMBLY} (NCBI, public domain). {len(loci)} loci: the "
                f"retained translation/replication core (EF-Tu/EF-G, DNA pol III, DnaK/GroEL, RNA polymerase, "
                f"ribosomal proteins) + the amino-acid-biosynthesis PROVISIONING pathway (the leucine leuB/leuD + "
                f"aromatic shikimate->chorismate aroC/aroK/aroB + threonine/aspartate genes) that Carsonella trades "
                f"its sap-feeding psyllid host for shelter. ~159,662 bp / ~182 ORFs / ~16.5% GC / ~97.3% coding "
                f"[Nakabachi 2006, Science 1134196] — one of the smallest cellular genomes; has SHED cell-envelope/"
                f"nucleotide/lipid/DNA-repair machinery -> NO free-living phase (cannot free-live; the ObligateSymbiont "
                f"role). The host-coupling kernel reads role + the SymbiosisCapacity lever, not specific genes "
                f"(ADR-019 S5, Mode B obligate symbiont)."
            ),
            "temp_optimum": 0.6,  # inside a ~25-30 C insect bacteriocyte — normalized into the sim's [0,1] band
            "parent_key": None,
            "trophic_role": "symbiont",  # the DATA-driven role override (gp::role_from_override -> ObligateSymbiont)
            "host_key": "default",       # the abstract plant/autotroph stand-in host for the PoC (see ADR-019 S5
                                         # open question: a plant is not a real Carsonella host — signed-off PoC shortcut)
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
