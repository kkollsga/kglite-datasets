"""Offline valid-time coverage for the packaged Sodir blueprint.

The packaged specs declare Sodir's history tables `closed` (``…To`` is the
last valid day) and read filtered copies the refresh writes. These tests build
the real Licence / Discovery / Company specs from small synthetic histories and
check what a caller sees: today's state by default, every version under
``FOR VALID_TIME ALL``, the one-day versions Sodir publishes, and the
auditable removal of rows kglite cannot declare.
"""

from __future__ import annotations

import csv
from datetime import datetime, timezone
import json
from pathlib import Path

from kglite_datasets import _sodir_internal
from kglite_datasets.sodir.wrapper import PACKAGED_BLUEPRINT, _build_graph

CSVS = {
    "company": "cmpNpdidCompany,cmpLongName\n1,Alpha\n2,Beta\n3,Gamma\n",
    "licence": "prlNpdidLicence,prlName\n100,PL100\n",
    # Two licensees, a one-day version on 2010-01-01 (Sodir publishes these:
    # every licensee of that day, each from == to), then today's split.
    "licence_licensee_hst": (
        "prlNpdidLicence,cmpNpdidCompany,prlLicenseeDateValidFrom,prlLicenseeDateValidTo,prlLicenseeInterest\n"
        "100,1,2000-01-01,2009-12-31,60\n"
        "100,2,2000-01-01,2009-12-31,40\n"
        "100,1,2010-01-01,2010-01-01,55\n"
        "100,2,2010-01-01,2010-01-01,45\n"
        "100,1,2010-01-02,,50\n"
        "100,3,2010-01-02,,50\n"
        # Inverted by 45 days: kglite refuses it, so it is dropped and logged.
        "100,2,2022-03-31,2022-02-14,99\n"
    ),
    "licence_operator_hst": (
        "prlNpdidLicence,cmpNpdidCompany,prlOperDateValidFrom,prlOperDateValidTo\n"
        "100,1,2000-01-01,2009-12-31\n"
        "100,3,2010-01-01,\n"
    ),
    "discovery": "dscNpdidDiscovery,dscName\n7,D7\n",
    # Epoch-ms bounds, as some FactMaps layers deliver them. The second row
    # ends the day before it starts: superseded on registration, kept empty.
    "discovery_operator_hst": (
        "dscNpdidDiscovery,cmpNpdidCompany,dscOperatorFrom,dscOperatorTo\n"
        "7,1,946684800000,1262217600000\n"
        "7,2,1262304000000,1262217600000\n"
        "7,3,1262304000000,\n"
    ),
    # Undated: a current snapshot, deliberately left undeclared.
    "discovery_licensee_hst": "dscNpdidDiscovery,cmpNpdidCompany\n7,1\n7,2\n",
}


def _workdir(tmp_path: Path) -> dict:
    csv_dir = tmp_path / "csv"
    csv_dir.mkdir()
    for stem, text in CSVS.items():
        (csv_dir / f"{stem}.csv").write_text(text)
    # Stamped at run time so refresh never re-checks the live catalogue.
    now = datetime.now(timezone.utc).isoformat(timespec="seconds")
    entry = {"kind": "user_supplied", "row_count": 1, "fetched_at_iso": now, "count_checked_at_iso": now}
    (tmp_path / "sodir_index.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "endpoint": "offline-test",
                "last_full_check_iso": now,
                "datasets": {s: {**entry, "csv_path": f"csv/{s}.csv"} for s in CSVS},
            }
        )
    )
    packaged = json.loads(PACKAGED_BLUEPRINT.read_text())
    nodes = {name: packaged["nodes"][name] for name in ("Company", "Licence", "Discovery")}
    for spec in nodes.values():
        spec["sub_nodes"] = {}
        spec.setdefault("connections", {})["fk_edges"] = {}
    nodes["Licence"]["connections"]["junction_edges"] = {
        k: nodes["Licence"]["connections"]["junction_edges"][k] for k in ("HAS_LICENSEE", "HAS_OPERATOR")
    }
    nodes["Discovery"]["connections"]["junction_edges"] = {
        k: nodes["Discovery"]["connections"]["junction_edges"][k] for k in ("HAS_LICENSEE", "HAS_OPERATOR")
    }
    return {"settings": packaged["settings"], "nodes": nodes}


def _build(tmp_path: Path):
    blueprint = _workdir(tmp_path)
    report = _sodir_internal.refresh(
        str(tmp_path),
        json.dumps(blueprint),
        index_cooldown_days=14,
        dataset_cooldown_days=30,
        concurrency=1,
        enhance_discovery_play=False,
    )
    return report, _build_graph(tmp_path, blueprint, "memory", None, False)


def _licensees(graph, **kwargs) -> list:
    query = "MATCH (:Licence)-[r:HAS_LICENSEE]->(c:Company) RETURN c.title AS c, r.prlLicenseeInterest AS i ORDER BY c"
    if "prefix" in kwargs:
        query = kwargs.pop("prefix") + " " + query
    return [(r["c"], r["i"]) for r in graph.cypher(query, **kwargs).to_list()]


def test_unprefixed_reads_are_today_and_all_is_history(tmp_path: Path) -> None:
    report, graph = _build(tmp_path)
    assert graph.graph_info()["valid_time_default"] == {"effective": "today", "stored": "today"}
    assert _licensees(graph) == [("Alpha", 50.0), ("Gamma", 50.0)]
    assert _licensees(graph, valid_at="2005-06-30") == [("Alpha", 60.0), ("Beta", 40.0)]
    assert len(_licensees(graph, prefix="FOR VALID_TIME ALL")) == 6
    assert graph.cypher("MATCH (:Licence)-[:HAS_OPERATOR]->(c) RETURN c.title AS c").to_list() == [{"c": "Gamma"}]


def test_one_day_versions_are_kept_and_sum_to_the_whole(tmp_path: Path) -> None:
    _, graph = _build(tmp_path)
    assert _licensees(graph, valid_at="2010-01-01") == [("Alpha", 55.0), ("Beta", 45.0)]
    for day in ("2009-12-31", "2010-01-01", "2010-01-02"):
        assert sum(i for _, i in _licensees(graph, valid_at=day)) == 100.0, day


def test_wide_inversions_are_dropped_and_logged(tmp_path: Path) -> None:
    report, graph = _build(tmp_path)
    pre = report["preprocess"]
    assert (pre["temporal_tables"], pre["temporal_inverted_dropped"], pre["temporal_empty_kept"]) == (3, 1, 1)
    assert 99.0 not in [i for _, i in _licensees(graph, prefix="FOR VALID_TIME ALL")]
    with (tmp_path / "csv" / "_derived_temporal_rejects.csv").open() as fh:
        rejects = list(csv.DictReader(fh))
    assert [(r["table"], r["row"], r["reason"], r["entity"], r["from"], r["to"]) for r in rejects] == [
        ("licence_licensee_hst", "7", "inverted", "100", "2022-03-31", "2022-02-14")
    ]
    assert json.loads(rejects[0]["record"])["prlLicenseeInterest"] == "99"
    # The cached source keeps every row; only the copy the build reads is filtered.
    assert (tmp_path / "csv" / "licence_licensee_hst.csv").read_text() == CSVS["licence_licensee_hst"]


def test_superseded_on_registration_is_an_empty_version(tmp_path: Path) -> None:
    _, graph = _build(tmp_path)
    operators = "MATCH (:Discovery)-[r:HAS_OPERATOR]->(c) RETURN c.title AS c ORDER BY c"
    assert graph.cypher(operators, valid_at="2005-01-01").to_list() == [{"c": "Alpha"}]
    assert graph.cypher(operators, valid_at="2010-01-01").to_list() == [{"c": "Gamma"}]
    assert graph.cypher(operators).to_list() == [{"c": "Gamma"}]
    assert len(graph.cypher("FOR VALID_TIME ALL " + operators).to_list()) == 3
    declared = graph.cypher(
        "CALL db.temporal.declarations() YIELD name, source_type, empty_rows "
        "WHERE name = 'HAS_OPERATOR' AND source_type = 'Discovery' RETURN empty_rows"
    ).to_list()
    assert declared == [{"empty_rows": 1}]


def test_undated_sources_stay_unfiltered(tmp_path: Path) -> None:
    _, graph = _build(tmp_path)
    licensees = "MATCH (:Discovery)-[:HAS_LICENSEE]->(c) RETURN count(c) AS n"
    assert graph.cypher(licensees).to_list() == [{"n": 2}]
    assert graph.cypher(licensees, valid_at="1990-01-01").to_list() == [{"n": 2}]


def test_fetch_list_names_sources_not_copies() -> None:
    stems = _sodir_internal.datasets_for_blueprint(PACKAGED_BLUEPRINT.read_text())
    assert "licence_licensee_hst" in stems
    assert not [s for s in stems if s.startswith("_derived_temporal_")]
