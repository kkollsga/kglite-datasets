# Sodir

Loads the [Sodir](https://factmaps.sodir.no) (Norwegian Offshore Directorate)
FactMaps registry — Norwegian Continental Shelf petroleum data (fields,
wellbores, licences, and their relationships) — into a kglite `KnowledgeGraph`.

`kglite-datasets` is an independent project, not affiliated with Sodir or the
Norwegian Offshore Directorate. The dataset structure and licensing are defined
upstream; this loader only handles the client-side cache + build lifecycle.

## Python

```python
from kglite_datasets import sodir

# Full lifecycle — fetch (or reuse cache), build, return a graph:
g = sodir.open(workdir)

# CSVs only, no graph build:
csvs = sodir.fetch_all(workdir)

g.cypher("MATCH (w:Wellbore)-[:IN_FIELD]->(f:Field) RETURN f.title, w.title LIMIT 5")
```

Layout managed under `workdir`:

```text
workdir/
    sodir_index.json          # fetch manifest (per-dataset row count, timestamps)
    csv/                      # cached CSVs, flat (field.csv, wellbore.csv, ...)
    graph/                    # disk graph dir built from the CSVs
```

The loader derives foreign-key relationships during a `preprocess` join pass and
converts ArcGIS geometry to WKT before the graph build.

## Rust

```rust
use kglite_datasets::sodir;
```

The Rust side emits the CSV set (including the FK-derivation output); the graph
build is the caller's job via kglite. The sodir `preprocess` output is anchored
by a frozen Rust golden — see [Parity & provenance](../parity-and-provenance.md).

## Offline tests

Bundled tests are offline (recorded ArcGIS fixtures). The live-API suite
self-skips unless its integration env var is set.
