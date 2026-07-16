# SEC EDGAR

Loads U.S. SEC [EDGAR](https://www.sec.gov/edgar) filings into a kglite
`KnowledgeGraph`: the quarterly index, bulk submissions, per-form fetchers, and
XBRL company facts. The loader is pure-Rust (no pandas) — it fetches, parses, and
emits CSVs; kglite builds the graph.

## Python

```python
from kglite_datasets.sec import SEC

# Ergonomic shortcut — name a form, a company, a span:
g = SEC.fetch(workdir, "13F-HR", "TSLA", years=2,
              user_agent="Name email@dom")

# Full control — separate index/detail spans, storage mode, flags:
g = SEC.open(workdir, years=10, detailed=2, mode="mapped",
             user_agent="Name email@dom")

g.cypher_query("MATCH (c:Company)-[:FILED]->(f:Filing) RETURN c.name, f.form LIMIT 5")
```

The `workdir` holds three tiers: `raw/` (fetched source), `processed/` (emitted
CSVs), and `graph/{mode}/` (the built graph). See the `SEC` class docstring for
the full lifecycle.

### User agent

SEC's EDGAR API requires a descriptive `User-Agent` with a contact address
(`"Name email@dom"`). The loader threads this through every request and honours
SEC's rate limits via a process-global rate gate.

## Rust

```rust
// The engine-free crate emits CSVs; build the graph with kglite.
use kglite_datasets::sec;
```

`sec::run_all` runs the full fetch → extract pipeline; `sec::run_all_at` takes an
injected `extracted_at` timestamp so the `source_extracted_at` provenance column
is deterministic (the one behavioral change made during extraction — it makes
SEC output goldenable). See [Parity & provenance](../parity-and-provenance.md).

## Offline tests

All bundled tests are offline (recorded fixtures). The live-API integration
suite self-skips unless `KGLITE_SEC_INTEGRATION=1` is set.
