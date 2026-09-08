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
g.cypher(
    "MATCH (d:Discovery)-[r:IN_PLAY]->(p:Play) "
    "RETURN d.title, p.title, r.match_method, r.distance_m, r.matched_ages"
)

# Optional document enrichment. Start with a bounded batch while exploring:
g = sodir.open(
    workdir,
    include_press_releases=True,
    press_release_limit=15,
)
```

Layout managed under `workdir`:

```text
workdir/
    sodir_index.json          # fetch manifest (per-dataset row count, timestamps)
    csv/                      # cached CSVs, flat (field.csv, wellbore.csv, ...)
    press_releases/raw/       # cached source PDFs and HTML
    graph/                    # disk graph dir built from the CSVs
```

The loader derives foreign-key relationships during a `preprocess` join pass and
converts ArcGIS geometry to WKT before the graph build.

## Play assignments

The packaged enhancement assigns every matching `Discovery IN_PLAY`
relationship. Published discovery examples are authoritative and retain their
source URLs. Published example fields receive separate direct `Field IN_PLAY`
links; field affiliation alone never assigns a constituent discovery.

For every discovery, `DISCOVERED_BY` follows the source `wlbNpdidWellbore`.
Published discovery pairs are unioned with spatial matches: the helper
normalizes well HC ages and play ages, then tests the designated well against
full play polygons, including boundaries and excluding holes. Every
age-compatible containing play across the HC slots is retained, with matched
slots recorded as provenance. If no published or containing play matches, the
nearest compatible polygon boundary determines the fallback without a distance
cutoff; exact-distance ties are retained and marked ambiguous. Distances use a
local equirectangular projection, recorded in `distance_method`. Unknown ages
cannot produce a spatial assignment.

Unassigned alternatives remain on diagnostic `CANDIDATE_PLAY` relationships;
an assigned discovery/play pair never also appears there. The published
examples are positive evidence, not a complete membership inventory.
Deduplicate discoveries within each play before aggregating volumes. A
discovery may legitimately belong to several plays, so totals from different
plays overlap and must not be summed into an estate-wide total.

## Discovery volumes

`DiscoveryVolume` provides one selected source hierarchy for volume queries. If
a discovery belongs to a field with a valid reserve snapshot, the latest
`FieldReserves` original-recoverable values are primary. Every constituent gets
a link to the same field snapshot for play reachability, and every copy carries
the same `aggregation_key`; deduplicate that key within each play. The row has
`method=field_reserves_primary`, `scope=shared_field`, and
`coverage=field_total`. It is field context, not a constituent allocation.

Only when no field snapshot exists does the loader use the latest dated
`DiscoveryReserves` records. Distinct resource-class rows on that date are
combined after exact duplicate removal; blanks remain null and numeric zero
remains zero. A conflicting or entirely empty latest field snapshot remains
missing rather than falling through component-by-component. Redirected discoveries share their terminal reporting root's
`aggregation_key` and covered-ID list. Conflicting records remain unusable.
Raw `FieldReserves` and `DiscoveryReserves` nodes and source JSON provenance are
preserved.

Do not sum repeated field keys, and do not sum totals between plays: the same
field can be represented by discoveries in several plays. For chronology, use
the earliest designated discovery-well completion date among the field's
matched discoveries in the selected play. Later discoveries are timing markers,
not another copy of the field volume. Missing components remain missing; no
component falls through to the secondary source.

```python
g.cypher(
    "MATCH (v:DiscoveryVolume)-[:OF_DISCOVERY]->(d:Discovery) "
    "WHERE v.usable = true "
    "RETURN d.title, v.recoverable_oe, v.method, v.scope, "
    "v.aggregation_key, v.estimate_date LIMIT 20"
)
```

## Press releases and volume evidence

Set `include_press_releases=True` on `open()` or `fetch_all()` to fetch the
distinct release URLs referenced by `wellbore.csv`. For an existing workdir,
`sodir.fetch_press_releases(workdir, limit=15)` runs only the document pass.
The source response is cached, the article is stored as Markdown on a
`PressRelease` node, and `Wellbore -[:HAS_PRESS_RELEASE]-> PressRelease`
preserves the source association.

The document pass only selects discoveries with uncertain structured
volumetrics: no usable selected volume, or a selected total shared by more than
one discovery. A usable individual discovery volume or a field total covering
one discovery is sufficient, so its release is skipped. `limit` is applied
after this eligibility filter.

`PressReleaseVolume` children record number-unit expressions such as
`3–5 million Sm3` or `15–30 billion Sm3`, together with commodity, scope,
qualifier, and the complete source sentence. Production rates are excluded.
Discovery-specific estimates, combined-area totals, and otherwise unspecified
mentions remain distinguishable. These rows are document evidence, not a
replacement for the structured `DiscoveryVolume` hierarchy: inspect `scope`
and `sourceText` before using a mention in an aggregate.

```python
g.cypher(
    "MATCH (w:Wellbore)-[:HAS_PRESS_RELEASE]->(p:PressRelease) "
    "MATCH (v:PressReleaseVolume)-[:OF_PRESS_RELEASE]->(p) "
    "RETURN w.title, v.minimum, v.maximum, v.value, v.unit, "
    "v.commodity, v.scope, v.sourceText"
)
```

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

Columns omitted from source-node properties: ArcGIS bookkeeping (`OBJECTID`, a
sequential row counter; `SHAPE`, a duplicate of `_geometry`; the computed
`Shape__Area` / `Shape__Length`, derivable from the WKT), the `*FactPageUrl` /
`*FactMapUrl` links back to Sodir's own web pages and the internal `*GUID`
identifiers. `wlbPressReleaseUrl` is retained because it is the source for the
optional document graph. Derived volume provenance retains exact source
rows, including their identifiers, so the original records can be traced.

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
