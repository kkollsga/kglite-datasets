"""Offline coverage for the derived Sodir layers the refresh writes.

Each test builds real packaged specs from a small synthetic workdir through
the same refresh + ``from_blueprint`` path ``sodir.open`` uses, then checks
what a caller sees in the graph.
"""

from __future__ import annotations

from datetime import datetime, timezone
import json
from pathlib import Path

from kglite_datasets import _sodir_internal
from kglite_datasets.sodir.wrapper import PACKAGED_BLUEPRINT, _build_graph

PACKAGED = json.loads(PACKAGED_BLUEPRINT.read_text())


def _spec(name: str, *, sub_nodes=(), junctions=(), fk_edges=()) -> dict:
    """A packaged node spec trimmed to the named children."""
    spec = json.loads(json.dumps(PACKAGED["nodes"][name]))
    spec["sub_nodes"] = {k: v for k, v in spec.get("sub_nodes", {}).items() if k in sub_nodes}
    conns = spec.setdefault("connections", {})
    conns["junction_edges"] = {k: v for k, v in conns.get("junction_edges", {}).items() if k in junctions}
    conns["fk_edges"] = {k: v for k, v in conns.get("fk_edges", {}).items() if k in fk_edges}
    return spec


def _build(tmp_path: Path, csvs: dict[str, str], nodes: dict[str, dict]):
    """Write ``csvs`` as cached datasets, refresh, and build ``nodes``."""
    csv_dir = tmp_path / "csv"
    csv_dir.mkdir(exist_ok=True)
    for stem, text in csvs.items():
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
                "datasets": {s: {**entry, "csv_path": f"csv/{s}.csv"} for s in csvs},
            }
        )
    )
    blueprint = {"settings": PACKAGED["settings"], "nodes": nodes}
    report = _sodir_internal.refresh(
        str(tmp_path),
        json.dumps(blueprint),
        index_cooldown_days=14,
        dataset_cooldown_days=30,
        concurrency=1,
        enhance_discovery_play=False,
    )
    assert not report["unfetchable"], report["unfetchable"]
    return report, _build_graph(tmp_path, blueprint, "memory", None, False)


def _rows(graph, query: str, **kwargs) -> list[dict]:
    return graph.cypher(query, **kwargs).to_list()


# ── well status timeline ────────────────────────────────────────────────

WELLBORE_HEADER = (
    "wlbNpdidWellbore,wlbWellboreName,wlbStatus,wlbDrillPerApprovedPermitDate,wlbEntryPreDrillDate,"
    "wlbCompPreDrillDate,wlbEntryDate,wlbCompletionDate,wlbDateReclass,wlbPluggedDate,"
    "wlbPluggedAbandonDate,wlbDateUpdated\n"
)
WELLBORES = WELLBORE_HEADER + (
    # Drilled, completed, plugged and abandoned (epoch-ms, as FactMaps ships).
    "1,15/9-F-12,P&A,,,,1181779200000,1188172800000,,,1476921600000,2026-09-02\n"
    # Producing since completion: the current status takes over the
    # COMPLETED period, Sodir gives no date for the change.
    "2,PROD-1,PRODUCING,,,,2010-01-01,2010-03-01,,,,2026-01-01\n"
    # Plugged with no date: the current status starts at the last event.
    "3,SUSP-1,SUSPENDED,,,,2011-01-01,2011-02-01,,2015-05-05,,2026-01-01\n"
    # Junked: an outcome, not a lifecycle stage.
    "4,JUNK-1,JUNKED,,,,2012-01-01,2012-01-20,,,,2026-01-01\n"
    # Reclassified: the reclass date carries the RE-CLASS status.
    "5,RECL-1,RE-CLASS TO DEV,,,,2013-01-01,2013-02-01,2013-06-01,,,2026-01-01\n"
    # Never drilled, no dated event at all.
    "6,NEVER-1,WILL NEVER BE DRILLED,,,,,,,,,2026-01-01\n"
)


def _well_nodes() -> dict:
    return {"Wellbore": _spec("Wellbore", junctions=("HAS_STATUS",)), "WellStatus": _spec("WellStatus")}


def _status(graph, name: str, **kwargs) -> list:
    query = (
        "MATCH (w:Wellbore {title: $name})-[r:HAS_STATUS]->(s:WellStatus) "
        "RETURN s.title AS status, r.basis AS basis ORDER BY r.validFrom"
    )
    return [(r["status"], r["basis"]) for r in _rows(graph, query, params={"name": name}, **kwargs)]


def test_well_status_timeline_reads_as_of(tmp_path: Path) -> None:
    _, graph = _build(tmp_path, {"wellbore": WELLBORES}, _well_nodes())
    assert _status(graph, "15/9-F-12", valid_at="2007-07-15") == [("DRILLING", "reported")]
    assert _status(graph, "15/9-F-12", valid_at="2012-01-01") == [("COMPLETED", "reported")]
    assert _status(graph, "15/9-F-12") == [("P&A", "reported")]
    assert len(_status(graph, "15/9-F-12", valid_at="all")) == 3
    assert _status(graph, "15/9-F-12", valid_at="2007-06-01") == []


def test_current_status_closes_the_timeline(tmp_path: Path) -> None:
    _, graph = _build(tmp_path, {"wellbore": WELLBORES}, _well_nodes())
    # COMPLETED is replaced by the current status from the completion date.
    assert _status(graph, "PROD-1", valid_at="all") == [("DRILLING", "reported"), ("PRODUCING", "inferred-start")]
    # Otherwise the current status starts on the last dated event.
    assert _status(graph, "SUSP-1") == [("SUSPENDED", "inferred-start")]
    assert _status(graph, "SUSP-1", valid_at="2015-06-01") == [("SUSPENDED", "inferred-start")]
    assert _status(graph, "SUSP-1", valid_at="2015-05-04") == [("COMPLETED", "reported")]
    assert _status(graph, "RECL-1", valid_at="2014-01-01") == [("RE-CLASS TO DEV", "reported")]


def test_outcomes_are_a_property_not_a_status(tmp_path: Path) -> None:
    _, graph = _build(tmp_path, {"wellbore": WELLBORES}, _well_nodes())
    outcomes = _rows(graph, "MATCH (w:Wellbore) WHERE w.wlbOutcome IS NOT NULL RETURN w.title AS w, w.wlbOutcome AS o")
    assert outcomes == [{"w": "JUNK-1", "o": "JUNKED"}]
    assert _status(graph, "JUNK-1") == [("COMPLETED", "reported")]
    statuses = {r["s"] for r in _rows(graph, "MATCH (s:WellStatus) RETURN s.title AS s")}
    assert "JUNKED" not in statuses and "BLOWOUT" not in statuses
    order = _rows(graph, "MATCH (s:WellStatus {title: 'P&A'}) RETURN s.phase_order AS o")
    assert order == [{"o": 15}]
    # A well with no dated event has no status period.
    assert _status(graph, "NEVER-1", valid_at="all") == []
