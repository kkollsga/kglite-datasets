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

## Play assignments

The packaged enhancement assigns at most one `Discovery IN_PLAY` relationship
per discovery. Published SODIR example fields receive direct `Field IN_PLAY`
links with source URLs. A unique field example takes precedence for its
discoveries, with `via_field_example` provenance recording that assignment
basis. Multiple published field examples require further resolving evidence.

For the remaining discoveries, `DISCOVERED_BY` follows the source
`wlbNpdidWellbore`. The helper normalizes well HC ages and play ages, then tests
the designated well against full play polygons, including boundaries and
excluding holes. Within containing candidates, HC Age 1 precedes HC Age 2,
which precedes HC Age 3; full age compatibility precedes partial compatibility
within the selected slot. If none contain the well, the earliest compatible
HC slot and nearest polygon boundary determine the fallback, without a distance
cutoff. Distances use a local equirectangular projection, recorded in
`distance_method`. Unknown ages cannot produce a spatial assignment.

Equal best candidates remain on diagnostic `CANDIDATE_PLAY` relationships;
they do not create multiple assigned `IN_PLAY` edges. Use assigned edges for
aggregation. The published examples are positive evidence, not a complete
membership inventory.

## Discovery volumes

`DiscoveryVolume` provides a common observation shape linked by `OF_DISCOVERY`.
The source `DiscoveryReserves` and `FieldReserves` nodes remain unchanged.
Every observation carries its method, coverage, basis, estimate date, source
record provenance and `generated`/`usable` flags. Missing or unresolved values
stay null; inspect `unresolved_reason` rather than treating them as zero.

Reported observations retain their resource classes and redirects. Generated
methods include latest original recoverable field estimates for strict
single-discovery fields, and explicitly separate inclusion-window change
estimates where a single entrant can be isolated. These bases are not
interchangeable: an annual reserve change is not a current discovery resource
estimate, and a reported contingent component is not necessarily a whole-field
total. Do not sum every date, class and method together.

Troll has a curated approximate allocation: two-thirds of gas-associated
components go to East, and oil to West. The published gas proportion comes
from [SODIR's resource report](https://www.sodir.no/aktuelt/publikasjoner/rapporter/ressursrapporter/ressursrapport-2024/gjenvarende-ressurser/);
the oil allocation, liquids following gas, and carrying the ratio to a later
snapshot are explicit assumptions. Both generated observations reconcile to
the latest source field total. Structural changes or conflicting source
observations prevent the allocation and produce a reason for review.

```python
g.cypher(
    "MATCH (v:DiscoveryVolume)-[:OF_DISCOVERY]->(d:Discovery) "
    "WHERE v.usable = true "
    "RETURN d.title, v.recoverable_oe, v.generated, v.method, "
    "v.basis, v.estimate_date LIMIT 20"
)
```

For discovery chronology, use the designated well's `wlbCompletionDate` via
`DISCOVERED_BY`, keeping the source `dscDiscoveryYear` for comparison. This
completion date is distinct from the resource estimate date: a curve of
current estimates ordered by discovery date is not a history of estimates
known at the time of discovery.

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
`*FactMapUrl` / `*PressReleaseUrl` links back to Sodir's own web pages, and the
internal `*GUID` identifiers. Derived volume provenance retains exact source
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
