"""The synthetic SEC ``raw/`` fixture — single source of truth.

Two CIKs in a ``submissions.zip`` plus one historical ``master.idx`` filing
that is deliberately absent from submissions. Offline: nothing here touches
the network.

This fixture's *content* is load-bearing — the frozen extract golden
(``tests/goldens/sec-extract-csv.sha256``) and the frozen graph-build golden
(``test_graph_build_golden.py``) are both digests of what this input produces.
Changing any value below re-baselines both goldens, so change it only
deliberately and refreeze in the same commit.

``benchmarks/bench_sec_extract.py`` keeps its own inlined copy on purpose: the
bench is a standalone script whose fixture is part of the frozen measurement
conditions in ``benchmarks/baseline.json``, and it must stay runnable without
importing the test package.
"""

from __future__ import annotations

import io
import json
from pathlib import Path
import zipfile

APPLE = {
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

MSFT = {
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

MASTER_IDX = (
    "Description: Master Index\n----\n"
    "1000045|NICHOLAS FINANCIAL INC|10-Q|2020-12-15|"
    "edgar/data/1000045/0001654954-20-001234-index.htm\n"
)


def write_synth_raw(workdir: Path) -> Path:
    """Write the synthetic ``raw/`` tier under ``workdir``; returns ``workdir``."""
    raw = workdir / "raw"
    (raw / "submissions").mkdir(parents=True, exist_ok=True)
    (raw / "index").mkdir(parents=True, exist_ok=True)

    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as z:
        z.writestr("CIK0000320193.json", json.dumps(APPLE))
        z.writestr("CIK0000789019.json", json.dumps(MSFT))
    (raw / "submissions" / "submissions.zip").write_bytes(buf.getvalue())

    (raw / "index" / "master.2020_QTR4.idx").write_text(MASTER_IDX)
    return workdir
