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

## Valid time

The packaged graph declares Sodir's history tables as validity intervals.
A query with no `FOR VALID_TIME` prefix reads the state as of today.

```python
g.cypher("MATCH (l:Licence)-[r:HAS_LICENSEE]->(c) RETURN c.title, r.prlLicenseeInterest")  # today
g.cypher("MATCH (l:Licence)-[r:HAS_LICENSEE]->(c) RETURN c.title", valid_at="2015-06-30")
g.cypher("FOR VALID_TIME ALL MATCH (l:Licence)-[r:HAS_LICENSEE]->(c) RETURN c.title")    # every version
g.set_valid_time_default("all")  # this session reads history by default
```

Declared types use Sodir's `closed` convention: the `…To` day is the last
valid day.

| Kind | Declared |
|---|---|
| Relationships, per source type | `HAS_LICENSEE` from `Field`, `Licence`, `BusinessArrangement`; `HAS_OPERATOR` from `Field`, `Discovery`, `Licence`, `TUF`, `BusinessArrangement`; `HAS_OWNER` from `TUF`; `INCLUDES_DISCOVERY` from `Field` |
| Nodes | `FieldStatusHistory`, `FieldOwnerHistory`, `DiscoveryPoly`, `LicencePhase`, `LicenceAreaPoly`, `TUF`, `SeismicFishery`, `BusinessArrangement`, `BusinessArrangementHistory`, `AfexArea`, `AfexAreaHistory`, `PetregLicence` |

Rules that follow from the source:

- Undated links stay unfiltered: `Discovery` and `PetregLicence` licensees,
  `PetregLicence` operators. Sodir publishes no dates for them.
- A relationship is hidden while its declared endpoint is not valid. A `TUF`
  that ended hides its owners, even when their rows are still open.
- A version superseded the day it was registered (`to` is the day before
  `from`) is kept as an empty interval, valid on no day.
- A one-day version (`from` equals `to`) is valid on that day.
- A row whose `to` lies more than one day before its `from` is a data error.
  kglite cannot declare it, so it is left out of the graph and listed in
  `workdir/csv/_derived_temporal_rejects.csv` with its source record
  (geometry columns left out).
  The cached source CSV keeps the row.

Seismic plan and weekly windows (`seaPlanFromDate`, `seaWeekly…`) are activity
dates, not validity, and are plain `date` properties.

## Status timelines

`(Wellbore)-[:HAS_STATUS {validFrom, validTo, basis}]->(:WellStatus)` is the
well's status history, declared `half_open`: `validTo` is the first day the
status no longer holds.

```python
g.cypher("MATCH (w:Wellbore {title: '15/9-F-12'})-[:HAS_STATUS]->(s) RETURN s.title")  # P&A
g.cypher("MATCH (w:Wellbore {title: '15/9-F-12'})-[:HAS_STATUS]->(s) RETURN s.title",
         valid_at="2007-07-15")                                                     # DRILLING
```

- Each dated event opens a period that lasts until the next one: permit,
  predrill entry and completion, entry (`DRILLING`), completion, reclass,
  plugged, P&A.
- The reclass date opens the well's `RE-CLASS TO DEV` / `RE-CLASS TO TEST`
  status, and only for a well that has that status.
- Sodir publishes the current `wlbStatus` but not the day it took effect.
  When it differs from the last dated event, it closes the timeline with
  `basis: "inferred-start"`. It replaces a final `COMPLETED` period (a
  producing well is producing from completion); otherwise it starts on the
  last event's date. Every other period has `basis: "reported"`.
- `JUNKED` and `BLOWOUT` are outcomes, not stages: they are in
  `Wellbore.wlbOutcome` and never in the timeline.
- Two events on the same day give an empty period, valid on no day.
- A well with no dated event has no status.
- `WellStatus.phase_order` is the lifecycle position (`PERMITTED` = 1 …
  `WILL NEVER BE DRILLED` = 16); a status outside that list has none.

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

`DiscoveryVolume` emits exactly one row for every discovery. For discoveries
assigned to a field, only its earliest included discovery may carry volume. It
uses its latest valid `DiscoveryReserves` snapshot when present, or the field's
latest valid `FieldReserves` snapshot as `method=field_reserves_fallback`.
Every later field discovery remains present as `method=no_volume`,
`usable=false`, with every volume component null, even when it has its own
structured discovery-reserves snapshot. The field fallback never shifts to a
later discovery when the earliest already has direct data; the field total is
unused in that case. A discovery without a field may still use its own latest
valid structured volume. Numeric zero remains a reported value.

Redirected discoveries do not copy their reporting root's values: the terminal
reporting root holds a selected structured volume once, while included
discoveries receive explicit null components.

Duplicate latest snapshots with different bookkeeping IDs are collapsed by
their component values (and by resource class for discovery rows), so they are
not summed twice. `source_record_json` retains every raw row and
`source_duplicate_count` records how many semantic duplicates were collapsed.

The earliest member is selected by the discovery's current field-inclusion
date, then its earliest field-inclusion-history date. If neither inclusion date
is available, the loader falls back to the designated discovery well's
completion date, discovery year, and finally numeric discovery ID. Equal dates
are resolved by numeric discovery ID. The output itself is sorted by numeric
discovery ID, so source row order cannot change the selected fallback or emitted
order.

```python
g.cypher(
    "MATCH (v:DiscoveryVolume)-[:OF_DISCOVERY]->(d:Discovery) "
    "RETURN d.title, v.recoverable_oe, v.method, "
    "v.estimate_date, v.unresolved_reason LIMIT 20"
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
identifiers. `wlbPressReleaseUrl` remains a raw Wellbore property for callers
that want to open Sodir's source document themselves; the loader does not fetch
or parse it. Derived volume provenance retains exact source rows, including
their identifiers, so the original records can be traced.

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
