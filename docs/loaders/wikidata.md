# Wikidata

Loads the Wikimedia Foundation's official
[`latest-truthy`](https://dumps.wikimedia.org/wikidatawiki/entities/) RDF dumps
into a kglite `KnowledgeGraph`. The loader orchestrates a resumable download and
a staleness / cooldown cache; the heavy N-Triples ingestion is done by the
kglite engine.

`kglite-datasets` is an independent project, not affiliated with the Wikimedia
Foundation or Wikidata.

## Python

```python
from kglite_datasets import wikidata

# Full lifecycle — download (resumable), cache-check, build, return a graph:
g = wikidata.open(workdir)

# Dump path only (no graph build):
dump = wikidata.fetch_truthy(workdir)

# Drop the process-local graph cache:
wikidata.cache_clear()
```

Layout managed under `workdir`:

```text
workdir/
    latest-truthy.nt.bz2          # cached dump
    latest-truthy.nt.bz2.part     # in-progress download (resumable)
    graph[_<N>m]/                 # disk graph built from the dump
```

The resumable download and the staleness / cooldown cache live in Rust (the
`kglite_datasets._wikidata_internal` extension); Python keeps the process-local
graph cache, the disk-mode rebuild decision, and the `load_ntriples` graph build.

Because the truthy dump is large, `open` builds a **disk-mode** graph by default;
`kglite`'s mapped/disk storage is what makes Wikidata-scale exploration
practical.

## Rust

```rust
use kglite_datasets::wikidata;
```

The Rust side handles download / cache-decision / freshness only; the graph is
built by kglite's `load_ntriples`.

## Offline tests

Bundled tests (cache-decision + freshness logic) are offline. The live-download
suite self-skips unless its integration env var is set.
