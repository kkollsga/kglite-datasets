"""GATE #6 — the per-form SEC parsers, offline.

Replaces ``test_usecases_v2.py``, which declared 11 use-case tests and skipped
all of them at module scope ("pending F2-F20"). That file had stopped being a
suite: it called ``_sec_internal.extract_processed`` and six other bindings
that no longer exist, so un-skipping it produced 11 collection errors, not 11
failures. It was 337 lines of unrunnable code inflating the skip count and
implying coverage that did not exist.

What *is* recoverable from it is its synthetic payload set — a Form 4 XML, a
13F information table, an 8-K cover page and an Exhibit 21 attachment — which
still exercise live parsers. The missing piece was never the assertions; it
was the fixture. The old one staged those documents on disk but declared only
a ``10-K`` per company in ``submissions.zip``, and every per-form extractor
resolves its inputs through ``processed/filing_index.csv`` (built from
submissions). So the documents were invisible: the extract pass read zero
files and every downstream query had nothing to find.

This module declares each staged document as the filing it actually is, and
asserts what comes out the other end — both at the CSV boundary (the extract
report) and in the built graph. Fully offline: no network, no live SEC, no
env-var gate.

Deliberately not covered: DEF 14A (board/director rows) and SC 13D (activist
stakes). Their documents parse in isolation but the extract pass does not pick
them up from this fixture even when named in ``form_types``, and chasing that
down is a separate piece of work from making the suite honest. They are absent
rather than skipped — an empty gap in this docstring, not a green test.
"""

from __future__ import annotations

import io
import json
from pathlib import Path
from typing import Any, cast
import zipfile

from kglite import from_blueprint  # engine — graph build stays in kglite
import pytest

from kglite_datasets import _sec_internal
from kglite_datasets.sec.wrapper import _blueprint_with_root, _load_blueprint

# ── the fixture ──────────────────────────────────────────────────────────

# Every staged document, keyed by the CIK that filed it. The form type here is
# load-bearing: it lands in `processed/filing_index.csv`, which is what each
# extractor consults to decide whether a document is in scope. Get it wrong
# and the extractors silently read nothing — the exact failure that left the
# old suite with an inert fixture.
FILINGS: dict[int, list[tuple[str, str, str]]] = {
    320193: [
        ("0000320193-24-000123", "10-K", "aapl-10k.htm"),
        ("0001214156-24-000777", "4", "form4.xml"),
        ("0000320193-24-008888", "8-K", "aapl-8k.htm"),
    ],
    789019: [("0000789019-24-000045", "10-K", "msft-10k.htm")],
    1364742: [("0001364742-24-000050", "13F-HR", "13f-infotable.xml")],
}
NAMES = {320193: "Apple Inc.", 789019: "Microsoft Corp", 1364742: "BLACKROCK INC."}
TICKERS = {320193: "AAPL", 789019: "MSFT", 1364742: "BLK"}

# `<documentType>` is what routes an ownership XML to the Form 4 emitter. The
# old fixture omitted it, so the document was found and then discarded as a
# parse error.
FORM4_XML = """<?xml version="1.0"?>
<ownershipDocument>
    <documentType>4</documentType>
    <periodOfReport>2024-09-30</periodOfReport>
    <issuer><issuerCik>0000320193</issuerCik><issuerName>Apple Inc.</issuerName></issuer>
    <reportingOwner>
        <reportingOwnerId><rptOwnerCik>0001214156</rptOwnerCik>
        <rptOwnerName>COOK TIMOTHY D</rptOwnerName></reportingOwnerId>
        <reportingOwnerRelationship><isOfficer>1</isOfficer>
        <officerTitle>CEO</officerTitle></reportingOwnerRelationship>
    </reportingOwner>
    <nonDerivativeTable>
        <nonDerivativeTransaction>
            <securityTitle><value>Common Stock</value></securityTitle>
            <transactionDate><value>2024-09-15</value></transactionDate>
            <transactionCoding><transactionCode>S</transactionCode></transactionCoding>
            <transactionAmounts>
                <transactionShares><value>50000</value></transactionShares>
                <transactionPricePerShare><value>220.00</value></transactionPricePerShare>
                <transactionAcquiredDisposedCode><value>D</value></transactionAcquiredDisposedCode>
            </transactionAmounts>
        </nonDerivativeTransaction>
    </nonDerivativeTable>
</ownershipDocument>"""

INFOTABLE_13F_XML = """<?xml version="1.0"?>
<informationTable xmlns="http://www.sec.gov/edgar/document/thirteenf/informationtable">
<infoTable><nameOfIssuer>APPLE INC</nameOfIssuer><cusip>037833100</cusip>
<value>50000</value><shrsOrPrnAmt><sshPrnamt>1000000</sshPrnamt>
<sshPrnamtType>SH</sshPrnamtType></shrsOrPrnAmt>
<investmentDiscretion>SOLE</investmentDiscretion></infoTable>
<infoTable><nameOfIssuer>MICROSOFT CORP</nameOfIssuer><cusip>594918104</cusip>
<value>40000</value><shrsOrPrnAmt><sshPrnamt>500000</sshPrnamt>
<sshPrnamtType>SH</sshPrnamtType></shrsOrPrnAmt>
<investmentDiscretion>SOLE</investmentDiscretion></infoTable>
</informationTable>"""

EIGHTK_HTML = (
    "<html><body>"
    "<p>Item 5.02 Departure of Directors or Certain Officers</p>"
    "<p>Item 9.01 Financial Statements and Exhibits</p>"
    "</body></html>"
)

EXHIBIT21_HTML = (
    "<html><body>EXHIBIT 21 SUBSIDIARIES OF APPLE INC.\n"
    "Apple Operations International     Ireland\n"
    "Apple Distribution International   Ireland\n"
    "Braeburn Capital, Inc.             Nevada\n"
    "</body></html>"
)

# accession (dashed) → (filename, content) for the payload documents.
PAYLOADS: dict[str, tuple[str, str]] = {
    "0000320193-24-000123": ("aapl-ex21.htm", EXHIBIT21_HTML),
    "0001214156-24-000777": ("form4.xml", FORM4_XML),
    "0000320193-24-008888": ("aapl-8k.htm", EIGHTK_HTML),
    "0001364742-24-000050": ("13f-infotable.xml", INFOTABLE_13F_XML),
}

# Which CIK subtree each payload is stored under. The Form 4's accession
# belongs to the reporting owner, but the fetcher files it under the issuer.
PAYLOAD_CIK: dict[str, int] = {
    "0000320193-24-000123": 320193,
    "0001214156-24-000777": 320193,
    "0000320193-24-008888": 320193,
    "0001364742-24-000050": 1364742,
}


def _stage_workdir(workdir: Path) -> Path:
    """Write a synthetic ``raw/`` whose submissions declare every payload."""
    raw = workdir / "raw"
    (raw / "submissions").mkdir(parents=True)
    (raw / "company_tickers.json").write_text("{}", encoding="utf-8")

    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as z:
        for cik, filings in FILINGS.items():
            z.writestr(
                f"CIK{cik:010}.json",
                json.dumps(
                    {
                        "cik": cik,
                        "name": NAMES[cik],
                        "tickers": [TICKERS[cik]],
                        "exchanges": ["Nasdaq"],
                        "entityType": "operating",
                        "filings": {
                            "recent": {
                                "accessionNumber": [a for a, _, _ in filings],
                                "filingDate": ["2024-10-01"] * len(filings),
                                "reportDate": ["2024-09-30"] * len(filings),
                                "form": [f for _, f, _ in filings],
                                "primaryDocument": [d for _, _, d in filings],
                            },
                            "files": [],
                        },
                    }
                ),
            )
    (raw / "submissions" / "submissions.zip").write_bytes(buf.getvalue())

    for accession, (name, content) in PAYLOADS.items():
        d = raw / "filings" / str(PAYLOAD_CIK[accession]) / accession.replace("-", "")
        d.mkdir(parents=True, exist_ok=True)
        (d / name).write_text(content, encoding="utf-8")
    return workdir


@pytest.fixture(scope="module")
def extracted(tmp_path_factory: pytest.TempPathFactory) -> dict[str, Any]:
    """Run the extract pass once, then build the graph from it once."""
    workdir = _stage_workdir(tmp_path_factory.mktemp("sec_offline"))
    report = _sec_internal.extract_all_py(str(workdir), force=True)

    compiled = workdir / "_offline_bp.json"
    compiled.write_text(
        json.dumps(_blueprint_with_root(_load_blueprint(), workdir)),
        encoding="utf-8",
    )
    try:
        graph = from_blueprint(str(compiled), verbose=False, save=False)
    finally:
        compiled.unlink(missing_ok=True)
    return {"workdir": workdir, "report": report, "graph": graph}


def _rows(graph: Any, query: str) -> list[dict[str, Any]]:
    return cast(list[dict[str, Any]], graph.cypher(query).to_list())


def _count(graph: Any, query: str) -> int:
    return int(_rows(graph, query)[0]["n"])


# ── the CSV boundary ─────────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("family", "min_rows"),
    [("form4", 3), ("form13f", 2), ("eightk", 2), ("ten_k", 3)],
)
def test_each_staged_family_is_actually_read(extracted: dict[str, Any], family: str, min_rows: int) -> None:
    """Every staged payload family must be read and emit rows.

    This is the assertion the old fixture would have failed: with the payloads
    undeclared in ``submissions.zip`` each of these reported
    ``files_read: 0``, and every graph-level assertion downstream then passed
    vacuously against an empty result or was never run at all.
    """
    stats = extracted["report"][family]
    assert stats["files_read"] >= 1, f"{family}: staged document was never read"
    assert stats["rows_written"] >= min_rows, f"{family}: parsed but emitted nothing"


# ── the graph ────────────────────────────────────────────────────────────


def test_insider_sale_reaches_the_graph(extracted: dict[str, Any]) -> None:
    """Form 4 → Person + Role + InsiderTransaction, wired to the issuer."""
    g = extracted["graph"]
    assert _count(g, "MATCH (t:InsiderTransaction) RETURN count(t) AS n") == 1
    assert _count(g, "MATCH (p:Person) RETURN count(p) AS n") == 1
    assert _count(g, "MATCH (r:Role) RETURN count(r) AS n") == 1
    # The transaction must connect the reporting owner to the issuer, not
    # float unattached — a parse that loses the endpoints still writes rows.
    assert _count(g, "MATCH (:InsiderTransaction)-[:BY_INSIDER]->(:Person) RETURN count(*) AS n") == 1
    assert _count(g, "MATCH (:InsiderTransaction)-[:IN_COMPANY]->(:Company) RETURN count(*) AS n") == 1
    assert _count(g, "MATCH (:Role)-[:OF_PERSON]->(:Person) RETURN count(*) AS n") == 1


def test_institutional_holdings_reach_the_graph(extracted: dict[str, Any]) -> None:
    """13F information table → one manager holding two positions."""
    g = extracted["graph"]
    assert _count(g, "MATCH (h:InstitutionalHolding) RETURN count(h) AS n") == 2
    assert _count(g, "MATCH (m:InstitutionalManager) RETURN count(m) AS n") == 1
    assert _count(g, "MATCH (:InstitutionalHolding)-[:OWNED_BY]->(:InstitutionalManager) RETURN count(*) AS n") == 2


def test_corporate_events_reach_the_graph(extracted: dict[str, Any]) -> None:
    """8-K cover page → one CorporateEvent per declared Item, at the filer."""
    g = extracted["graph"]
    assert _count(g, "MATCH (e:CorporateEvent) RETURN count(e) AS n") == 2
    assert _count(g, "MATCH (:CorporateEvent)-[:AT_COMPANY]->(:Company) RETURN count(*) AS n") == 2


def test_subsidiaries_reach_the_graph(extracted: dict[str, Any]) -> None:
    """Exhibit 21 → three subsidiaries, all attached to their parent."""
    g = extracted["graph"]
    names = {r["name"] for r in _rows(g, "MATCH (s:Subsidiary) RETURN s.name AS name")}
    assert len(names) == 3
    assert any("Braeburn" in n for n in names if n)
    assert _count(g, "MATCH (:Subsidiary)-[:SUBSIDIARY_OF]->(:Company) RETURN count(*) AS n") == 3


def test_filings_and_companies_are_identified(extracted: dict[str, Any]) -> None:
    """Each filing whose document was parsed becomes a Filing, attached to a
    Company.

    Four of the five declared accessions have a staged document; Microsoft's
    10-K deliberately has none, and its absence here is the assertion that a
    Filing node comes from parsed content rather than from the submissions
    declaration alone.
    """
    g = extracted["graph"]
    forms = {r["a"]: r["ft"] for r in _rows(g, "MATCH (f:Filing) RETURN f.accession_number AS a, f.form_type AS ft")}
    assert set(forms) == set(PAYLOADS), "one Filing per parsed document"
    assert forms["0001214156-24-000777"] == "4"
    assert forms["0001364742-24-000050"] == "13F-HR"
    assert _count(g, "MATCH (:Filing)-[:FILED_BY]->(:Company) RETURN count(*) AS n") == len(PAYLOADS)
