#!/usr/bin/env python3
"""Aggregate per-generation batch stats into one columnar Parquet file (SPEC §5; slice S3.3).

Reads every `data/runs/*/per_gen.csv` (written by `harness --per-gen-stats`, driven across many seeds by
`tools/run_batch.sh`) and concatenates them into a single Parquet table for fast cross-run analysis of
emergent behavior. Column types are pinned so all runs share one schema (concat is then lossless).

Usage:
    .venv/bin/python scripts/aggregate_parquet.py [--runs-dir data/runs] [-o data/runs/batch.parquet]
"""
import argparse
import glob
import os
import sys

try:
    import pyarrow as pa
    import pyarrow.csv as pacsv
    import pyarrow.parquet as pq
except ImportError:  # pragma: no cover - environment guard
    sys.stderr.write(
        "error: pyarrow not available. Install the project venv:\n"
        "  .venv/bin/pip install -r scripts/requirements.txt\n"
    )
    sys.exit(2)

# Pinned schema for the per_gen.csv columns (harness --per-gen-stats). Forcing types makes every run's
# table identical so pa.concat_tables is lossless even when a column is constant in one run.
COLUMN_TYPES = {
    "run_index": pa.int64(),
    "generation": pa.int64(),
    "population_size": pa.int64(),
    "allele_freq": pa.float64(),
    "growth_rate": pa.float64(),
    "reflectance": pa.float64(),
    "drought_tolerance": pa.float64(),
    "fecundity": pa.float64(),
    "kill_switch_linkage": pa.float64(),
}


def aggregate(runs_dir: str, out: str) -> int:
    paths = sorted(glob.glob(os.path.join(runs_dir, "*", "per_gen.csv")))
    if not paths:
        sys.stderr.write(f"no per_gen.csv found under {runs_dir} (run tools/run_batch.sh first)\n")
        return 1

    convert = pacsv.ConvertOptions(column_types=COLUMN_TYPES)
    tables = [pacsv.read_csv(p, convert_options=convert) for p in paths]
    table = pa.concat_tables(tables)

    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    pq.write_table(table, out)
    runs = table.column("run_index").unique().to_pylist() if table.num_rows else []
    print(
        f"wrote {out}: {table.num_rows} rows x {table.num_columns} cols "
        f"from {len(paths)} files ({len(runs)} runs)"
    )
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Aggregate per_gen.csv batch stats into a Parquet file.")
    ap.add_argument("--runs-dir", default="data/runs", help="directory containing <run_id>/per_gen.csv")
    ap.add_argument("-o", "--out", default="data/runs/batch.parquet", help="output Parquet path")
    args = ap.parse_args()
    return aggregate(args.runs_dir, args.out)


if __name__ == "__main__":
    raise SystemExit(main())
