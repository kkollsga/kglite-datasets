# tests/goldens — frozen digests (the durable oracle)

One `<corpus>.sha256` file per goldened dataset build: a single lowercase-hex
SHA-256 line = the digest of a **canonical, order-insensitive rendering** of the
CSV set a loader emits from a synthetic offline input. Two renderings share a
digest iff they hold the same CSVs with the same headers and the same row
multiset — so the digest catches any extraction / shape / preprocess regression
without a network round-trip or the graph engine.

There are two goldens, one per builder surface:

- **`sodir-csv.sha256`** — the **Rust** oracle. Frozen by `tests/csv_golden.rs`
  (`canonical_csv_dir` → `digest`), which runs the sodir `preprocess::apply`
  FK-derivation joins on `tests/fixtures/sodir/csv-in/` and digests the result.
  Runs under the normal gate (`cargo test --workspace`, i.e. `make test`).
- **`sec-extract-csv.sha256`** — the **Python** oracle. Frozen by
  `kglite_datasets/tests/test_parity_golden.py` (`_canonical_processed` →
  `_digest`), which runs `_sec_internal.extract_all_py` on a synthetic 2-CIK
  `raw/` fixture and digests the `processed/` CSVs (the volatile
  `source_extracted_at` provenance column is dropped). Runs under `make pytest`.

## Provenance and authority

**Captured 2026-07-16 from the last in-sync in-tree builder** — i.e. frozen from
`kglite`'s in-tree loaders (`kglite::api::datasets` / `kglite._sec_internal`)
while a two-builder equivalence check verified ours produced byte-for-byte
identical output. **These digests remain the anchor.**

KGLite deleted its in-tree loaders on 2026-07-16 (`kglite::api::datasets` and
`kglite._sec_internal` are gone), so the live cross-builder comparison is
impossible forever. The single-builder golden checks — `tests/csv_golden.rs`
(sodir) and `test_parity_golden.py::test_sec_extract_golden` (SEC), each
building with **our** loader only and comparing to these frozen digests — are
now the sole guardians that this crate still produces the historically-correct
output. The two-builder parity test has been retired.

Freezing from our own builder *after* deletion would bless any silent transform
bug as "correct." These digests stay anchored to the verified-correct upstream
output captured while both builders existed.

## Golden vs self-consistency

- **Golden digest** — for output that is fully deterministic across from-scratch
  runs (sodir preprocess).
- **Volatile-column exclusion** — **SEC** stamps `source_extracted_at` (wall
  clock) into a provenance column; the canonical rendering drops that column so
  the remaining CSV content is deterministic and goldenable. If any residual
  run-to-run volatility ever appears, prefer a build-twice self-consistency
  assertion over freezing a flapping digest.

## Regenerating (deliberate builder-behavior changes only)

Do **not** regenerate a golden to make a red test green — a digest change means
the emitted CSVs changed. Regenerate only when that change is intended (a parser
fix, a new column, a shape change), in the **same commit** as the code change,
with a recorded reason. Both capture paths honor `UPDATE_GOLDEN=1`:

```bash
# Rust (sodir):
UPDATE_GOLDEN=1 cargo test -p kglite-datasets --test csv_golden

# Python (SEC):
UPDATE_GOLDEN=1 .venv/bin/python -m pytest \
    kglite_datasets/tests/test_parity_golden.py
```

Review the diff, and land the changed `.sha256` in the same commit as the
builder change that caused it.
