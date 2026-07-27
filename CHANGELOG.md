# Changelog

All notable changes to kglite-datasets are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); this project adheres to
semantic versioning (workspace version in the root `Cargo.toml`).

## [Unreleased]

### Added

- **GATE #4 — the Python lint gate now actually runs.** `ruff` was wired into
  `make lint` from day one behind a `command -v ruff || skip` guard, was never
  installed by anyone, and was never in CI — so it had executed exactly zero
  times while reporting success, and had quietly accumulated 42 findings and 11
  unformatted files. Three changes make it real: a committed `[tool.ruff]`
  config in `pyproject.toml` (deliberately mirroring `../kglite`'s, so the
  ecosystem shares one Python style); `make ruff-check` now hard-fails when
  ruff is missing instead of skipping, and a new `make venv` target provisions
  it so it cannot be missing; and a new `python-lint` CI job runs
  `ruff check` + `ruff format --check` on every push.

- **GATE #5 — the built-graph cache** (`kglite_datasets/tests/test_graph_cache.py`).
  Eight offline tests pinning the disk-cache probe against a real kglite disk
  graph, the rejection of an uncommitted build and of a dangling `CURRENT`
  pointer, and the rebuild-on-unloadable fallback — the last exercised against
  a directory that legitimately passes the probe and fails only on load. See
  Fixed for what they caught.

- **GATE #6 — the per-form SEC parsers, offline**
  (`kglite_datasets/sec/tests/test_extract_offline.py`). Nine tests taking a
  synthetic Form 4 / 13F information table / 8-K cover page / Exhibit 21
  through the extract pass and into the built graph, asserting both the CSV
  boundary (files read, rows emitted) and the resulting topology. Replaces
  `test_usecases_v2.py`, which declared 11 tests, skipped all of them at module
  scope, and could not have run: it called `_sec_internal.extract_processed`
  and six sibling bindings that no longer exist. Its payloads were recoverable;
  its fixture was the broken part — it staged the documents but declared only a
  `10-K` per company in `submissions.zip`, and every extractor resolves inputs
  through `processed/filing_index.csv`, so nothing was ever read.

- **GATE #7 — skip accounting** (`kglite_datasets/conftest.py`). Every run
  prints a `suite accounting:` line, and any skipped test that neither carries
  `@pytest.mark.live` nor matches an allow-listed reason **fails the run**.
  A skip is the one outcome that costs nothing and never turns red, so left
  ungoverned it accumulates — this suite reported *4 passed, 24 skipped* while
  green. It now reports *21 passed, 13 skipped*, and the 13 are the live-SEC
  suites, marked as such. `--strict-markers` is on so a mistyped marker is a
  collection error rather than an inert decorator.

### Fixed

- **SEC `mode="disk"` never cached, for two independent reasons — both fixed.**
  The Rust `graph_exists(Disk)` probe looked for `graph_manifest.json`, which
  kglite has never written, so the cache-hit branch was unreachable; and
  `_build_graph(mode="disk")` passed `save=True` to `from_blueprint`, which
  only honours the blueprint's `output` key (ours has none), so nothing was
  ever committed to the graph directory in the first place. Every disk-mode
  open therefore rebuilt from scratch, silently.

  The probe now looks for kglite's real commit markers — `CURRENT` (generation
  layout) or `disk_graph_meta.json` (flat) — via a new shared crate module
  `disk_graph`, exposed to Python as `kglite_datasets._disk_graph`. That module
  absorbs the two *other* copies of this logic the repo already had: the sodir
  Rust layout and the wikidata Python wrapper had each derived the right answer
  separately while the SEC one, the only load-bearing copy, was wrong. One
  question should not have three implementations. `CURRENT` is also
  *dereferenced* rather than merely detected: kglite writes it last, by atomic
  rename, only after verifying the staged generation — but a directory a user
  has half-deleted or partially restored can still carry a dangling pointer,
  and one extra `stat` turns that from "probes valid, refuses to load" into an
  ordinary cache miss. The disk build now calls `save()` explicitly, matching
  the sodir and wikidata wrappers.

- **A cached graph that will not re-open is now a cache miss, not an error.**
  All three loaders route their reopen through `kglite_datasets._cache`, which
  returns `None` (with a `StaleGraphCacheWarning`) instead of propagating, so
  the caller rebuilds. This matters here specifically: the broken probe above
  was *masking* the zero-row disk defect below, and repairing the probe alone
  would have turned a silent rebuild into a hard user-facing failure. With the
  fallback, the wrapper is correct either way — while the engine defect stands,
  disk callers rebuild exactly as before; once it is fixed the cache begins
  hitting with no further change. `test_graph_cache.py` asserts both arms.

- **Text file I/O now specifies `encoding="utf-8"` explicitly** across the
  three wrappers (found by the newly-live ruff gate, rule `PLW1514`). Without
  it Python uses the platform's locale encoding, so on a Windows host reading
  SEC's `company_tickers.json` or Sodir's Norwegian dataset text would decode
  under cp1252 and either corrupt names or raise `UnicodeDecodeError`. Only one
  site was flagged by the rule (it only fires where the receiver is statically
  a `Path`); the rest were fixed as the same class of bug.

- **GATE #3 — built-graph golden** (`kglite_datasets/tests/test_graph_build_golden.py`,
  `tests/goldens/sec-graph-build.sha256`). GATE #1 freezes the CSVs we emit;
  everything after that boundary (blueprint → `from_blueprint` → `.kgl` →
  `load`) is kglite's, and the offline suite previously asserted nothing about
  it — an engine upgrade could change node/edge counts or break the
  save→reload round trip with the suite still fully green. The new gate freezes
  the topology of the graph a user actually receives and exercises the SEC
  wrapper's create-then-reopen cache contract.

### Changed

- Verified against **kglite 0.15.0**. The built-graph digest is identical under
  kglite 0.13.0, 0.14.5 and 0.15.0, so the `kglite>=0.13` floor stays as-is;
  `pyproject.toml` now records why, so the pin reads as a decision rather than
  an oversight.
- CI's Python job now runs against **both ends** of the declared engine range
  (`kglite>=0.13` and `kglite==0.13.0`) on each OS. A floor that is never
  exercised is a floor we cannot honestly advertise — previously CI only ever
  resolved the newest kglite, so the lower bound was untested.
- The synthetic SEC `raw/` fixture is now defined once in
  `kglite_datasets/tests/synth_sec.py` instead of being copy-pasted into each
  test module. Content is unchanged — the frozen extract golden is untouched.
  `benchmarks/bench_sec_extract.py` deliberately keeps its own inline copy: its
  fixture is part of the frozen measurement conditions in `baseline.json`.

### Known issues

- **Engine: a blueprint node type with zero data rows makes a disk-mode graph
  directory unreadable.** `from_blueprint(..., storage="disk")` succeeds and
  the graph is usable in-process, but `kglite.load(dir)` afterwards fails with
  `FileFormatError: invalid id_indices.bin: directory contains an unresolved
  type key`. Minimal repro (no kglite-datasets involvement): two CSVs sharing a
  header where the second has no data rows — the same blueprint round-trips
  fine in memory mode, and fine in disk mode as soon as the second type has one
  row. Reproduces identically on kglite **0.13.0, 0.14.5 and 0.15.0**, so it is
  **not** a 0.15 regression. This is an engine defect, not worked around here.

  It reaches us through SEC `mode="disk"`: the blueprint declares 26 node
  types, and any info-row type with no rows for the requested slice trips it —
  in the synthetic fixture only `Company`, `Filing` and `SicCode` have rows, and
  each of the other 23 fails on its own. It was previously masked by the broken
  `graph_exists(Disk)` probe (see Fixed). Now that the probe is correct, the
  cache-reopen fallback is what keeps it non-fatal: the unreadable directory is
  treated as a cache miss and the graph is rebuilt. Users on `mode="disk"`
  therefore pay a full rebuild on every open until the engine fix ships — the
  same cost as before, but now deliberate and warned about rather than silent.

  **The blast radius is wider than blueprints with empty types** (confirmed by
  the kglite engine agent, 2026-07-27). A *read-only* query naming a label that
  does not exist — `MATCH (n:Ghost {id: 1})` — caches a build-on-miss id index
  under that label and poisons the next `save()` identically. So a disk graph
  can be made unloadable by a query that mutates nothing, and **any** cached
  directory in the wild may carry this regardless of how it was built. That is
  why the fallback is required independently of the probe fix, and why it is
  tested against a directory that legitimately passes the probe and only fails
  on load. The engine fix also lands a narrow reader recovery — an unresolvable
  directory entry is skipped only when it holds zero entries, a populated one
  still fails loudly — so already-broken directories become loadable once it
  ships, without weakening corruption detection.

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
