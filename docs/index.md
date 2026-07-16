# kglite-datasets

Fetch-build-cache dataset loaders for [kglite](https://github.com/kkollsga/kglite).
Each loader pulls a public registry, builds it into a queryable kglite
`KnowledgeGraph`, and caches the result — so an application can treat a public
dataset as a typed Python value.

`kglite-datasets` is a standalone extraction of kglite's former in-tree
`kglite::datasets` loaders, moved into their own crate + wheel so the kglite
engine links **zero network code**. The loaders **fetch → parse → emit**
(CSV / blueprint JSON / N-Triples); the graph build reuses the kglite engine
(`from kglite import KnowledgeGraph`), which is a runtime dependency — reused,
not forked.

## Install

```bash
pip install kglite-datasets      # ships the loaders + Python interface; pulls kglite
```

The single wheel bundles the Rust loaders **and** the Python interface; the
graph engine comes from `kglite`.

## Quickstart

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
submodules, PEP 562), so the SEC path stays light.

### Rust

Rust embedders use the crate-root modules directly (they mirror kglite's former
`api::datasets::<loader>` surface): `kglite_datasets::{sec, sodir, wikidata}`.
The loaders are engine-free — they emit CSV / blueprint JSON / N-Triples, and
the caller builds the graph via kglite's `from_blueprint` / `load_ntriples`.

```toml
[dependencies]
kglite-datasets = "0.1"
```

## Contents

```{toctree}
:maxdepth: 1
:caption: Loaders

loaders/sec
loaders/sodir
loaders/wikidata
```

```{toctree}
:maxdepth: 1
:caption: About

parity-and-provenance
migration
```

## License

MIT © Kristian dF Kollsgård. `kglite-datasets` is an independent project — not
affiliated with the SEC, the Norwegian Offshore Directorate, or the Wikimedia
Foundation. Dataset structure and licensing are defined by each upstream source;
these loaders only handle the client-side cache + build lifecycle.
