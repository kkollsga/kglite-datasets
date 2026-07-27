#!/usr/bin/env python
"""GATE #2 — SEC extract perf gate vs the frozen in-tree baseline.

kglite's in-tree loader (`kglite._sec_internal`) has been removed, so there is
no longer a second builder to A/B against at runtime. Instead this bench runs
our extracted loader (`kglite_datasets._sec_internal`) on a synthetic raw/
fixture and compares its median against the **frozen baseline** in
`benchmarks/baseline.json` — captured 2026-07-16 from kglite's in-tree loader
while ours was proven byte-identical to it.

    python benchmarks/bench_sec_extract.py [--json]

Exits non-zero if our loader's median regresses beyond `tolerance` x the
baseline median (default 1.5x — the same guard the old two-builder bench used).
This is the whole point of the retarget: with in-tree gone, the gate must anchor
to a stored number, not silently no-op. Reports median + min (min = least-noise
best case, per kglite's performance protocol). Offline — no network.
"""

from __future__ import annotations

import io
import json
from pathlib import Path
import statistics
import sys
import tempfile
import time
import zipfile

from kglite_datasets import _sec_internal as ours

WARMUP = 5
ITERS = 50
BASELINE_PATH = Path(__file__).resolve().parent / "baseline.json"


def _synth_raw(workdir: Path) -> None:
    raw = workdir / "raw"
    (raw / "submissions").mkdir(parents=True, exist_ok=True)
    (raw / "index").mkdir(parents=True, exist_ok=True)
    apple = {
        "cik": 320193,
        "name": "Apple Inc.",
        "sic": "3571",
        "sicDescription": "Electronic Computers",
        "stateOfIncorporation": "CA",
        "fiscalYearEnd": "0930",
        "tickers": ["AAPL"],
        "exchanges": ["Nasdaq"],
        "entityType": "operating",
        "formerNames": [{"name": "Apple Computer Inc", "from": "1976-04-01", "to": "2007-01-09"}],
        "filings": {
            "recent": {
                "accessionNumber": ["0000320193-24-000123", "0000320193-24-000089"],
                "filingDate": ["2024-11-01", "2024-08-02"],
                "reportDate": ["2024-09-28", "2024-06-29"],
                "form": ["10-K", "10-Q"],
                "primaryDocument": ["aapl-20240928.htm", "aapl-20240629.htm"],
            },
            "files": [],
        },
    }
    msft = {
        "cik": 789019,
        "name": "Microsoft Corp",
        "sic": "7372",
        "stateOfIncorporation": "WA",
        "tickers": ["MSFT"],
        "exchanges": ["Nasdaq"],
        "filings": {
            "recent": {
                "accessionNumber": ["0000789019-24-000045"],
                "filingDate": ["2024-07-30"],
                "reportDate": ["2024-06-30"],
                "form": ["8-K"],
                "primaryDocument": ["msft-20240730.htm"],
            },
            "files": [],
        },
    }
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as z:
        z.writestr("CIK0000320193.json", json.dumps(apple))
        z.writestr("CIK0000789019.json", json.dumps(msft))
    (raw / "submissions" / "submissions.zip").write_bytes(buf.getvalue())
    (raw / "index" / "master.2020_QTR4.idx").write_text(
        "Description: Master Index\n----\n"
        "1000045|NICHOLAS FINANCIAL INC|10-Q|2020-12-15|"
        "edgar/data/1000045/0001654954-20-001234-index.htm\n"
    )


def _time_one(mod) -> float:
    """One extract on a fresh workdir; returns wall-clock milliseconds."""
    with tempfile.TemporaryDirectory() as d:
        wd = Path(d)
        _synth_raw(wd)
        t = time.perf_counter()
        mod.extract_all_py(str(wd), force=True)
        return (time.perf_counter() - t) * 1000.0


def main() -> int:
    as_json = "--json" in sys.argv
    baseline = json.loads(BASELINE_PATH.read_text())
    base_median = float(baseline["median_ms"])
    tolerance = float(baseline.get("tolerance", 1.5))

    for _ in range(WARMUP):
        _time_one(ours)

    ours_ms = [_time_one(ours) for _ in range(ITERS)]

    def summ(xs: list[float]) -> dict[str, float]:
        return {"median_ms": round(statistics.median(xs), 4), "min_ms": round(min(xs), 4)}

    ours_summ = summ(ours_ms)
    ratio = ours_summ["median_ms"] / base_median
    result = {
        "iters": ITERS,
        "ours": ours_summ,
        "baseline": {
            "median_ms": base_median,
            "min_ms": baseline.get("min_ms"),
            "source": baseline.get("source"),
            "date": baseline.get("date"),
        },
        "ratio": round(ratio, 3),
        "tolerance": tolerance,
    }

    if as_json:
        print(json.dumps(result, indent=2))
    else:
        print(f"SEC extract ({ITERS} iters, synthetic 2-CIK fixture):")
        print(f"  ours    : median {ours_summ['median_ms']:.3f} ms  min {ours_summ['min_ms']:.3f} ms")
        print(f"  baseline: median {base_median:.3f} ms  ({baseline.get('source')}, {baseline.get('date')})")
        print(f"  ratio   : {ratio:.2f}x baseline median (tolerance {tolerance}x)")

    if ratio > tolerance:
        print(f"  FAIL: ours {ratio:.2f}x baseline median (> {tolerance}x) — perf regression", file=sys.stderr)
        return 1
    print(f"  OK: within {tolerance}x baseline ({ratio:.2f}x)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
