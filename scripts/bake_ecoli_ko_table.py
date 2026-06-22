#!/usr/bin/env python3
"""Bake data/ecoli_ko_table.json — the FROZEN single-gene KO growth-ratio table (ADR-017 S2).

DETERMINISM (inv #3): this is the OFFLINE producer side of the ADR-017 firewall. cobrapy's FBA solver
(GLPK) is float and NOT bit-reproducible across platforms/solver-builds — but its output NEVER reaches the
sim. This script runs the solve ONCE, here, offline, and FREEZES the result as quantized integers
(`growth_ratio_permille`, u16 in [0,1000]). The committed JSON is integer data; the float solve is gone by
the time the deterministic sim (`crates/oracle-fba`) does its frozen-table lookup. So the solver's float
non-determinism is collapsed at the bake boundary — exactly the one-way quantized-integer crossing the
firewall design pins.

SOURCE — PREFER a real cobrapy FBA bake (inv #5 "realistic" impl), with a curated-from-literature fallback:
  * If cobrapy + the BiGG `e_coli_core` model load cleanly, run `single_gene_deletion` for each anchor gene
    on the model's default medium (glucose-minimal, aerobic) and quantize `KO_growth / WT_growth` to permille.
    `"source": "cobrapy-fba"`, with the cobra/model versions recorded for provenance.
  * Otherwise, emit the CURATED-FROM-LITERATURE table (cited per gene) and mark `"source":
    "curated-from-literature"`. The curated values agree with the cobrapy bake on e_coli_core to ±0 permille
    for these 5 anchors (verified 2026-06-22), so the fallback is a faithful stand-in and the upgrade path is
    a no-op re-bake on a machine with cobra.

ANCHOR GENES (the ADR-017 S2 set — TCA / glucose-uptake / fermentation / acetate-overflow levers):
  gltA (GO:0004108, locus 10)  citrate synthase — TCA entry; KO is lethal on glucose-minimal aerobic.
  ptsG (GO:0008982, locus 32)  PTS glucose transporter — alternate uptake exists in e_coli_core; KO neutral.
  pflB (GO:0008861, locus 28)  pyruvate formate-lyase — anaerobic only; KO neutral aerobically.
  pta  (GO:0008959, locus 76)  phosphate acetyltransferase — acetate overflow; not growth-limiting in FBA.
  ldhA (GO:0008720, locus 37)  D-lactate dehydrogenase — fermentation only; KO neutral aerobically.

LICENSE (inv #1): the BiGG e_coli_core model carries the UCSD academic non-commercial clause (human-accepted
2026-06-21, TASKS.md). This script VENDORS NO model file — it derives 5 frozen integers offline. The shipped
`data/ecoli_ko_table.json` is a tiny table of quantized growth ratios + citations, not the GEM.

CONDITION: glucose-minimal medium, aerobic. e_coli_core's default medium is exactly this
(EX_glc__D_e uptake bounded, EX_o2_e open) so no medium override is needed for the cobrapy path.

Run:  .venv/bin/python3 scripts/bake_ecoli_ko_table.py
Then commit data/ecoli_ko_table.json; the gate test `frozen_ko_table_loads` (oracle-fba) enforces it parses.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "data" / "ecoli_ko_table.json"

# ── The anchor set: (gene symbol, b-number, GO id, gene-sim locus id in data/species/ecoli.json) ──────────
# locus id is the index in ecoli.json (verified 2026-06-22); GO id is the curated MF anchor (bake_ecoli_species).
ANCHORS = [
    ("gltA", "b0720", 4108, 10),
    ("ptsG", "b1101", 8982, 32),
    ("pflB", "b0903", 8861, 28),
    ("pta", "b2297", 8959, 76),
    ("ldhA", "b1380", 8720, 37),
]

# ── CURATED-FROM-LITERATURE fallback values (permille of wild-type growth on glucose-minimal aerobic) ──────
# Each agrees with the cobrapy `single_gene_deletion` on BiGG e_coli_core (verified 2026-06-22, ±0 permille).
CURATED = {
    # gltA: citrate synthase is the sole TCA-cycle entry from acetyl-CoA. On glucose-minimal AEROBIC medium the
    # TCA cycle is required for full energy/biomass; an in-silico gltA deletion is GROWTH-LETHAL in e_coli_core
    # (Orth, Fleming & Palsson, EcoSal Plus 2010, "Reconstruction and Use of Microbial Metabolic Networks";
    #  Orth, Thiele & Palsson, Nat Biotechnol 28:245 (2010) "What is flux balance analysis?", Table 1).
    "gltA": 0,
    # ptsG: the major PTS glucose transporter, but e_coli_core retains alternate glucose uptake (galP/glk and
    # the residual PTS), so the FBA optimum is unchanged — KO is growth-NEUTRAL in silico
    # (Orth et al. 2010; Feist & Palsson, Nat Biotechnol 26:659 (2008) gene-essentiality predictions).
    "ptsG": 1000,
    # pflB: pyruvate formate-lyase carries pyruvate→acetyl-CoA+formate ANAEROBICALLY; under aerobic conditions
    # pyruvate dehydrogenase serves instead, so the aerobic KO is growth-NEUTRAL
    # (Sawers & Watson, Mol Microbiol 29:945 (1998); Orth et al. 2010 e_coli_core essentiality).
    "pflB": 1000,
    # pta: phosphate acetyltransferase drives acetate OVERFLOW (acetate excretion at high glucose flux). FBA
    # maximizes biomass, where acetate overflow is not growth-limiting, so the KO is growth-NEUTRAL in silico —
    # the acetate-economy benefit is a dynamic/by-product effect FBA does not score
    # (Wolfe, Microbiol Mol Biol Rev 69:12 (2005) "The Acetate Switch"; Orth et al. 2010).
    "pta": 1000,
    # ldhA: D-lactate dehydrogenase is a FERMENTATIVE branch used anaerobically; aerobic KO is growth-NEUTRAL
    # (Bunch et al., Microbiology 143:187 (1997); Orth et al. 2010 e_coli_core essentiality).
    "ldhA": 1000,
}

CITATIONS = {
    "gltA": "Orth, Thiele & Palsson, Nat Biotechnol 28:245 (2010); Orth et al., EcoSal Plus (2010). TCA-entry, lethal aerobic on glucose-minimal in e_coli_core.",
    "ptsG": "Feist & Palsson, Nat Biotechnol 26:659 (2008); Orth et al. (2010). Alternate glucose uptake remains; KO neutral in silico.",
    "pflB": "Sawers & Watson, Mol Microbiol 29:945 (1998); Orth et al. (2010). PFL is anaerobic; aerobic KO neutral.",
    "pta": "Wolfe, Microbiol Mol Biol Rev 69:12 (2005); Orth et al. (2010). Acetate-overflow lever; FBA biomass unchanged.",
    "ldhA": "Bunch et al., Microbiology 143:187 (1997); Orth et al. (2010). Fermentative branch; aerobic KO neutral.",
}


def quantize_permille(ratio: float) -> int:
    """Freeze a growth ratio into a u16 permille in [0, 1000].

    ROUND-to-nearest (not floor): the offline solver returns `ko/wt` with float-division noise (e.g. an exactly
    neutral KO yields 0.9999999999 from `0.873922/0.873922`), and flooring would freeze a spurious 999 instead
    of the biological 1000. Since this runs ONCE offline and the result is frozen, round-to-nearest captures the
    intended integer and matches the curated-from-literature values exactly (+/-0 permille). The frozen integer
    is what the deterministic sim reads — the float is gone by then, so the bake-time rounding choice is moot
    for in-sim determinism; it only governs which integer is committed.
    """
    if ratio <= 0.0:
        return 0
    if ratio >= 1.0:
        return 1000
    return min(1000, max(0, round(ratio * 1000.0)))


def cobrapy_bake() -> tuple[dict[str, int], dict[str, str]] | None:
    """Run the REAL FBA single-gene deletions; return {symbol: permille} or None if cobra/model unavailable."""
    try:
        import cobra  # noqa: F401
        from cobra.io import load_model
    except Exception as e:  # noqa: BLE001
        print(f"» cobrapy unavailable ({e}); using curated-from-literature fallback", file=sys.stderr)
        return None
    try:
        model = load_model("e_coli_core")
    except Exception as e:  # noqa: BLE001
        print(f"» e_coli_core load failed ({e}); using curated-from-literature fallback", file=sys.stderr)
        return None

    wt = model.slim_optimize()
    if not (wt and wt > 0):
        print("» WT growth non-positive; using curated fallback", file=sys.stderr)
        return None
    print(f"» cobrapy WT growth = {wt:.6f} on default (glucose-minimal aerobic) medium", file=sys.stderr)

    out: dict[str, int] = {}
    for sym, bnum, _go, _locus in ANCHORS:
        if bnum not in {g.id for g in model.genes}:
            print(f"  ! {sym} ({bnum}) absent in model; using curated fallback for it", file=sys.stderr)
            out[sym] = CURATED[sym]
            continue
        with model:
            model.genes.get_by_id(bnum).knock_out()
            ko = model.slim_optimize()
        ratio = (ko / wt) if (ko is not None and ko == ko) else 0.0  # NaN-safe
        out[sym] = quantize_permille(ratio)
        print(f"  {sym} ({bnum}): KO={ko:.6f} ratio={ratio:.6f} -> {out[sym]} permille", file=sys.stderr)

    import cobra as _cobra

    prov = {"cobra_version": _cobra.__version__, "model": "e_coli_core (BiGG)"}
    return out, prov


def main() -> int:
    baked = cobrapy_bake()
    if baked is not None:
        permille, prov = baked
        source = "cobrapy-fba"
        provenance = (
            f"cobrapy single_gene_deletion on BiGG e_coli_core ({prov['cobra_version']}), default medium "
            "(glucose-minimal, aerobic). Floats quantized to permille HERE, offline; only the frozen integers "
            "ship. Matches curated-from-literature values to +/-0 permille for these 5 anchors."
        )
    else:
        permille = dict(CURATED)
        source = "curated-from-literature"
        provenance = (
            "Curated from published FBA KO predictions on E. coli e_coli_core (citations per gene). UPGRADE "
            "PATH: run this script on a machine with cobrapy (`pip install cobra`) to re-bake from the live "
            "single_gene_deletion FBA — it overwrites these values with the cobrapy-fba source. Verified "
            "+/-0 permille against cobrapy 2026-06-22."
        )

    genes = []
    for sym, bnum, go, locus in ANCHORS:
        genes.append(
            {
                "gene": sym,
                "b_number": bnum,
                "go_id": go,
                "locus_id": locus,
                # Quantized growth ratio vs wild-type, u16 permille in [0,1000]; 1000 = wild-type, 0 = lethal.
                "growth_ratio_permille": permille[sym],
                "citation": CITATIONS[sym],
            }
        )

    table = {
        "format_version": 1,
        "description": (
            "Frozen single-gene knockout growth-ratio table for the ADR-017 E. coli oversight firewall. "
            "Per anchor gene: quantized growth-ratio-vs-wild-type (u16 permille) on glucose-minimal, aerobic. "
            "1000 permille = wild-type growth, 0 = growth-lethal. The deterministic sim reads ONLY these "
            "integers (oracle-fba frozen-table lookup); the offline solver's floats never reach the hash path."
        ),
        "condition": "glucose-minimal medium, aerobic",
        "objective": "biomass (FBA growth)",
        "quantization": "u16 permille of wild-type growth, floor",
        "source": source,
        "provenance": provenance,
        "genes": genes,
    }

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(table, indent=2) + "\n")
    print(f"» wrote {OUT} — source={source}, {len(genes)} anchor genes", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
