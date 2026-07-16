# Parity & provenance

`kglite-datasets` did not start as fresh code — it is a **behavior-preserving
extraction** of loaders that lived inside kglite. This page explains how that
extraction was verified correct, and why the frozen oracles in this repo now
carry that authority forward.

## The extraction

The loaders were moved out of kglite via a mechanical import transform
(`kglite::(api::)datasets::` → `kglite_datasets::`, `kglite.` →
`kglite_datasets.`) with **zero engine coupling** — the engine-free loaders
fetch, parse, and emit CSV / blueprint JSON / N-Triples, and the graph build
stays in kglite. Because the transform only rewrote import paths, the copied
inline unit tests — byte-identical to kglite's — pass against the transformed
code, proving the port is behavior-preserving.

The **one** behavioral change in the whole extraction is the SEC injectable
clock: `sec::run_all` now delegates to `run_all_at(..., extracted_at)`, so the
`source_extracted_at` provenance column can be pinned. That makes SEC output
deterministic and therefore goldenable — the only clock leak on the SEC extract
path.

## Authority transfer

While kglite still had its in-tree copy, a two-builder equivalence check
verified that this crate produced **byte-for-byte identical output**. At that
moment — 2026-07-16 — three oracles were frozen:

- **`tests/goldens/sodir-csv.sha256`** — the Rust output-boundary golden.
  `tests/csv_golden.rs` runs the sodir `preprocess` FK-derivation joins over
  synthetic fixtures and digests a canonical, order-insensitive rendering of the
  emitted CSVs. Runs under `cargo test --workspace`.
- **`tests/goldens/sec-extract-csv.sha256`** — the Python SEC golden.
  `kglite_datasets/tests/test_parity_golden.py` runs the SEC extract on a
  synthetic 2-CIK fixture and digests the `processed/` CSVs (the volatile
  `source_extracted_at` column is dropped). Runs under `make pytest`.
- **`benchmarks/baseline.json`** — the SEC-extract perf anchor (median
  3.369 ms / min 3.252 ms on Apple M4), with a 1.5× regression tolerance.

kglite **deleted** its in-tree loaders on 2026-07-16. The live cross-builder
comparison is therefore impossible forever, and the two-builder parity test has
been retired. The single-builder golden checks — each building with *this*
loader only and comparing against the frozen digests — are now the sole
guardians that this crate still produces the historically-correct output.

Freezing a golden from our own builder *after* deletion would bless any silent
transform bug as "correct." The digests stay anchored to the verified-correct
upstream output captured while both builders existed.

## Regenerating a golden

Do **not** regenerate a golden to turn a red test green — a digest change means
the emitted CSVs changed. Regenerate only when that change is intended (a parser
fix, a new column, a shape change), in the **same commit** as the code change,
with a recorded reason. Both capture paths honour `UPDATE_GOLDEN=1`:

```bash
# Rust (sodir):
UPDATE_GOLDEN=1 cargo test -p kglite-datasets --test csv_golden

# Python (SEC):
UPDATE_GOLDEN=1 .venv/bin/python -m pytest \
    kglite_datasets/tests/test_parity_golden.py
```

Re-baseline `benchmarks/baseline.json` only for a deliberate, measured perf
change — never to paper over a regression. Judge by the ratio, not absolute ms
(which is machine-specific).
