# Changelog

All notable changes to kglite-datasets are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); this project adheres to
semantic versioning (workspace version in the root `Cargo.toml`).

## [Unreleased]

## [0.1.0] - 2026-07-16

Initial public release. `kglite-datasets` is the standalone extraction of
kglite's in-tree domain dataset loaders (SEC EDGAR / Sodir / Wikidata), moved
into their own crate + wheel so the kglite engine links zero network code.

### What's included

- **Rust workspace** — `crates/kglite-datasets` (engine-free loaders: fetch →
  parse → emit CSV / blueprint JSON / N-Triples dump) and
  `crates/kglite-datasets-py` (PyO3 wrapper → the single `kglite-datasets`
  wheel). No workspace member links the kglite engine; graph *building* is the
  caller's job via kglite's `from_blueprint` / `load_ntriples`.
- **sec** — SEC EDGAR: quarterly index, bulk submissions, per-form fetchers,
  XBRL company facts, the parser set + extract sinks.
- **sodir** — Norwegian Offshore Directorate FactMaps registry (orchestrator /
  blueprint / preprocess FK-derivation / index / fetch / geojson→WKT).
- **wikidata** — `latest-truthy` RDF dump download / cache-decision / freshness
  orchestration (resumable download + staleness cache in Rust; the heavy
  N-Triples ingestion stays engine-side in kglite).
- **Shared `DatasetClient`** (`http` module) — ureq (rustls + gzip) with a
  process-global rate gate and exponential-backoff retry.
- **Python package** `kglite_datasets` — the loader wrappers with lazy
  submodule loading (PEP 562): importing `sec` never drags in
  `sodir`/`wikidata`'s optional pyarrow stack.

### Provenance & parity

- Extracted via a mechanical `kglite::(api::)datasets::` → `kglite_datasets::`
  transform (zero engine coupling); the copied inline unit tests — byte-identical
  to kglite's — pass against the transformed code, proving the port is
  behavior-preserving.
- The **only** behavioral change in the whole extraction is the SEC
  injectable-clock fix (`run_all_at`), which pins `source_extracted_at` so SEC
  output is deterministic and goldenable.
- **Frozen oracles captured 2026-07-16 from the last in-sync in-tree builder**,
  while a two-builder equivalence check verified byte-for-byte identical output:
  - `tests/goldens/sodir-csv.sha256` — the Rust output-boundary golden
    (`tests/csv_golden.rs` over the sodir preprocess FK-joins).
  - `tests/goldens/sec-extract-csv.sha256` — the Python SEC extract golden
    (`kglite_datasets/tests/test_parity_golden.py`).
  - `benchmarks/baseline.json` — the SEC-extract perf anchor (median 3.369 ms /
    min 3.252 ms on Apple M4).
  kglite removed its in-tree loaders on 2026-07-16, so these frozen digests +
  baseline are now the sole oracles carrying the verified-correct authority
  forward. See `dev-docs/plans/dataset-loader-extraction.md`.

### Compatibility

- Runtime dependency on `kglite>=0.13` (the graph engine, reused not forked).
- All shared loader deps pinned in lockstep with kglite's `Cargo.lock`.
- All tests are **offline** (recorded fixtures); live-API smokes are opt-in
  behind integration env vars and self-skip in CI.
