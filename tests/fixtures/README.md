# tests/fixtures — recorded HTTP fixtures (offline test inputs)

Small, committed, byte-deterministic inputs so the test suite is **fully
offline** — no loader test ever touches the network (CI must never depend on
SEC / Wikidata / Sodir being up). Live-API smokes are separate and opt-in
(behind a `live` marker / env var).

Layout (populated per loader as its phase lands):
```
fixtures/
  sec/        recorded EDGAR responses (quarterly index, submissions, a few filings, XBRL facts)
  sodir/      recorded ArcGIS registry responses
  wikidata/   recorded Last-Modified headers + a tiny N-Triples dump slice
```

Each fixture is the raw response body a fetcher would receive; the parsing +
emit + graph-build runs against it deterministically. Keep them **small** —
enough to exercise each parser/shape, not a full registry mirror. Built `.kgl`
graphs are regenerable and NOT committed (see root `.gitignore`); only the raw
inputs + the frozen `tests/goldens/*.sha256` digests are tracked.
