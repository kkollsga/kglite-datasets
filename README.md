# kglite-datasets

[![Docs](https://img.shields.io/badge/docs-readthedocs-blue)](https://kglite-datasets.readthedocs.io)
[![PyPI](https://img.shields.io/pypi/v/kglite-datasets)](https://pypi.org/project/kglite-datasets/)
[![crates.io](https://img.shields.io/crates/v/kglite-datasets)](https://crates.io/crates/kglite-datasets)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)

Fetch-build-cache dataset loaders for [kglite](https://github.com/kkollsga/kglite) —
pull a public registry, build it into a queryable kglite `KnowledgeGraph`, cache
the result. Extracted from `kglite::datasets` into a standalone crate + wheel so
the kglite engine links zero network code.

**Bundled sources:**
- **SEC EDGAR** — quarterly index, bulk submissions, per-form fetchers, XBRL company facts.
- **Sodir** — Norwegian Offshore Directorate registry.
- **Wikidata** — dump download / cache orchestration (heavy ingestion stays engine-side).

## Install

```bash
pip install kglite-datasets      # ships the loaders + Python interface; pulls kglite
```

The single wheel bundles the Rust loaders **and** the Python interface; the graph
engine is reused from `kglite` (a runtime dependency), not forked.

## Usage

Each loader fetches its source, builds it into a kglite `KnowledgeGraph`, and
caches the result under a workdir — returning a graph ready to query:

```python
# SEC EDGAR — name a form, a company, a span:
from kglite_datasets.sec import SEC
g = SEC.fetch(workdir, "13F-HR", "TSLA", years=2, user_agent="Name email@dom")

# Sodir (Norwegian Offshore Directorate):
from kglite_datasets import sodir
g = sodir.open(workdir)

# Wikidata truthy dump (cache-managed download + N-Triples build):
from kglite_datasets import wikidata
g = wikidata.open(workdir)

g.cypher_query("MATCH (c:Company) RETURN c.name LIMIT 5")
```

Importing `sec` never drags in `sodir`/`wikidata`'s optional stack (lazy
submodules), so the SEC path stays light.

### Rust

Rust embedders use the crate-root modules directly (they mirror kglite's former
`api::datasets::<loader>` surface): `kglite_datasets::{sec, sodir, wikidata}`.

## Status

**Alpha — extraction complete.** Followed the codingest handover playbook:
copy → prove parity → freeze goldens + bench baseline (while kglite still had its
in-tree copy) → kglite removed its loaders (2026-07-16). With the in-tree copy
gone, the frozen goldens (`tests/goldens/`) and the frozen bench baseline
(`benchmarks/baseline.json`) are the sole oracles carrying that verified-correct
authority forward. See `dev-docs/plans/dataset-loader-extraction.md`.

## Development

```bash
make gate        # CI-equivalent local gate: lint · build · test · determinism · bench-smoke
make lint        # cargo fmt --check + clippy -D warnings (+ ruff if installed)
make develop     # maturin develop — build the extension into the active venv
make pytest      # offline Python suite (live-API tests behind the `live` marker)
```

The Rust workspace: `crates/kglite-datasets` (engine-free loaders) +
`crates/kglite-datasets-py` (PyO3 wrapper → the wheel). Loaders **fetch → parse →
emit CSV/blueprint/dump**; the graph build reuses kglite's `from_blueprint` /
`load_ntriples`. All tests are **offline** (recorded HTTP fixtures); live-API
smokes are opt-in.

## Links

- **Documentation:** https://kglite-datasets.readthedocs.io
- **PyPI:** https://pypi.org/project/kglite-datasets/ (`pip install kglite-datasets`)
- **crates.io:** https://crates.io/crates/kglite-datasets (`cargo add kglite-datasets`)
- **kglite engine:** https://github.com/kkollsga/kglite

Badges and links point at the initial `0.1.0` release; there are no prior
published versions.

## License

MIT © Kristian dF Kollsgård
