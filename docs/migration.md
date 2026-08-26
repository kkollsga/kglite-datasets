# Migrating from `kglite.datasets` (≤ 0.13)

Through kglite 0.13, the SEC / Sodir / Wikidata loaders shipped **inside**
kglite, as `kglite.datasets` (Python) and `kglite::api::datasets` (Rust). As of
2026-07-16 those in-tree loaders were removed from kglite and now live here, in
the standalone `kglite-datasets` package. If you imported the loaders from
kglite, switch to `kglite-datasets`.

## Install

```bash
pip install kglite-datasets      # pulls kglite as a runtime dependency
```

You keep `kglite` for the graph engine (querying, `.kgl` persistence, the MCP
server). `kglite-datasets` adds the loaders back on top.

## Python import changes

| Before (kglite ≤ 0.13)            | Now (kglite-datasets)                     |
|-----------------------------------|-------------------------------------------|
| `from kglite.datasets.sec import SEC` | `from kglite_datasets.sec import SEC` |
| `from kglite.datasets import sodir`   | `from kglite_datasets import sodir`   |
| `from kglite.datasets import wikidata`| `from kglite_datasets import wikidata`|

The loader **API is unchanged** — `SEC.fetch` / `SEC.open`, `sodir.open` /
`sodir.fetch_all`, `wikidata.open` / `wikidata.fetch_truthy` all keep their
signatures (this is a behavior-preserving extraction, see
[Parity & provenance](parity-and-provenance.md)). Only the import path moves.

The graph engine imports stay on kglite:

```python
from kglite import KnowledgeGraph, from_blueprint, load   # unchanged
```

## Rust import changes

| Before (kglite ≤ 0.13)         | Now (kglite-datasets)          |
|--------------------------------|--------------------------------|
| `kglite::api::datasets::sec`   | `kglite_datasets::sec`         |
| `kglite::api::datasets::sodir` | `kglite_datasets::sodir`       |
| `kglite::api::datasets::wikidata` | `kglite_datasets::wikidata` |

```toml
[dependencies]
kglite = "0.16.12"        # the engine (graph build, persistence)
kglite-datasets = "0.1"   # the loaders
```

The loaders are engine-free: they emit CSV / blueprint JSON / N-Triples, and you
build the graph with kglite's `from_blueprint` / `load_ntriples`.

## Why the split

The loaders pull network stacks (TLS, zip, XBRL parsing) that the kglite engine
should not carry — and, on macOS, one loader's optional `pyarrow` export path
sat next to a dynamic-linker hazard. Extracting the loaders lets the engine link
zero network code and keeps that blast radius out of every kglite install.
