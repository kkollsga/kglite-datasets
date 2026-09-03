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

## What the blueprint deliberately leaves out

Sodir publishes ~150 datasets; the shipped blueprint loads 103 of them, and
each node type declares an explicit `properties` whitelist, so anything below
is absent from the graph by construction rather than by a filter. The reasons
are recorded here because the blueprint is data — kglite reports (and ignores)
any key it does not read, so annotations live in prose, not in the JSON.

Four tables are fetched by nothing:

| Table | Why |
|---|---|
| `strat_litho_wellbore.csv` | Same data as `wellbore_formation_top`. |
| `strat_litho_wellbore_core.csv` | Core subset of the formation tops; reachable via `WellboreCore`. |
| `wellbore_core_photo_aggr.csv` | Aggregated view of `wellbore_core_photo`; no unique data. |
| `seismic_acquisition_licence.csv` | Company-specific seismic permits — not a graph relationship. |

Columns dropped everywhere they appear: ArcGIS bookkeeping (`OBJECTID`, a
sequential row counter; `SHAPE`, a duplicate of `_geometry`; the computed
`Shape__Area` / `Shape__Length`, derivable from the WKT), the `*FactPageUrl` /
`*FactMapUrl` / `*PressReleaseUrl` links back to Sodir's own web pages, and the
internal `*GUID` identifiers.

Columns dropped from `wellbore.csv` and `facility.csv`: the DMS
(`…NsDeg`/`Min`/`Sec`/`Code`, `…EwDeg`/`Min`/`Sec`/`Code`), decimal-degree and
UTM (`…NsUtm`, `…EwUtm`, `…UtmZone`, `fclUtmHemisphere`) coordinate components
plus `…GeodeticDatum`, all superseded by the single WGS84 WKT geometry the
preprocess pass writes; the drill-permit coordinate pair `wlbDrillPerNsDeg` /
`wlbDrillPerEwDeg`; the parsed name components `wlbNamePart1`–`6`, redundant
with `wlbWellboreName`; and the split `wlbEntryDay` / `wlbEntryMonth` /
`wlbCompletionDay` / `wlbCompletionMonth`, redundant with the full
`wlbEntryDate` / `wlbCompletionDate`.

Nine node types are geometry-only — they carry a WKT outline and no
relationships: `SubArea`, `Dome`, `FaultBoundary`, `SedimentBoundary`,
`AreaStatus`, `SbmBlock`, `SbmQuadrant`, `SbmOccurrence`, `SbmPlayEstimate`.

`Stratigraphy`'s `STRAT_PARENT` is a self-reference: its foreign key
`lsuNpdidLithoStratParent` matches the type's own primary key
`lsuNpdidLithoStrat`.

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
