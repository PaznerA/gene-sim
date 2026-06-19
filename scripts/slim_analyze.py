#!/usr/bin/env python3
"""Read a SLiM tree-sequence (.trees) and emit summary genetics stats as JSON (SPEC §W9; slice S2.3).

The SLiM oracle (crates/oracle-slim) shells out to `slim` and produces a `.trees` file; this script reads
it back via tskit and reduces it to a small, stable stats dict — the basis of the Stage 2 golden gate (S2.4).

Usage:
    .venv/bin/python scripts/slim_analyze.py <path.trees>        # JSON to stdout
    .venv/bin/python scripts/slim_analyze.py <path.trees> -o out.json

Determinism note: stats are computed from the genealogy/mutations only — NOT from the file bytes (SLiM
writes provenance with a timestamp, so two same-seed runs differ byte-wise but yield identical genetics).
Allele frequencies are derived-allele-count / num_samples, so each is in [0, 1] (SPEC §10.4).
"""
import argparse
import json
import sys

try:
    import tskit
except ImportError:  # pragma: no cover - environment guard
    sys.stderr.write(
        "error: tskit not available. Install the project venv:\n"
        "  python3 -m venv .venv && .venv/bin/pip install tskit pyslim msprime numpy\n"
        "and run this script with .venv/bin/python.\n"
    )
    sys.exit(2)


def analyze(path: str) -> dict:
    ts = tskit.load(path)
    n = ts.num_samples

    freqs = []  # derived-allele frequencies, one per non-ancestral allele observed
    for var in ts.variants():
        g = var.genotypes
        for allele_idx in range(1, len(var.alleles)):
            count = int((g == allele_idx).sum())
            if count:
                freqs.append(count / n)

    has_sites = ts.num_sites > 0
    stats = {
        "num_samples": int(n),
        "num_individuals": int(ts.num_individuals),
        "num_trees": int(ts.num_trees),
        "num_sites": int(ts.num_sites),
        "num_mutations": int(ts.num_mutations),
        "sequence_length": float(ts.sequence_length),
        "num_segregating_sites": len(freqs),
        # Mean derived-allele frequency across segregating sites (in [0,1]); 0.0 if monomorphic.
        "mean_allele_freq": (sum(freqs) / len(freqs)) if freqs else 0.0,
        "max_allele_freq": max(freqs) if freqs else 0.0,
        # Nucleotide diversity (tskit site-mode); 0.0 when there are no sites.
        "nucleotide_diversity": float(ts.diversity()) if has_sites else 0.0,
    }
    return stats


def main() -> int:
    ap = argparse.ArgumentParser(description="Summarize a SLiM .trees into JSON stats.")
    ap.add_argument("trees", help="path to a .trees tree-sequence file")
    ap.add_argument("-o", "--out", help="write JSON here instead of stdout")
    args = ap.parse_args()

    stats = analyze(args.trees)
    text = json.dumps(stats, indent=2, sort_keys=True)
    if args.out:
        with open(args.out, "w") as fh:
            fh.write(text + "\n")
    else:
        sys.stdout.write(text + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
