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


# ── facility and pipeline status ────────────────────────────────────────

FACILITIES = (
    "fclNpdidFacility,fclName,fclPhase,fclStartupDate,fclDateShutdown,fclDateRemoved\n"
    "10,IN-SERVICE,IN SERVICE,2000-01-01,,\n"
    "11,GONE,REMOVED,1980-01-01,1999-01-01,2005-06-01\n"
    # A startup date on a facility not yet in service is a plan.
    "12,PLANNED,FUTURE,2030-01-01,,\n"
    # Sodir's 1900-01-01 'unknown' sentinel is no date.
    "13,STOPPED,SHUT DOWN,1990-01-01,-2208988800000,\n"
)
PIPELINES = (
    "pplNpdidPipeline,pplName,pplCurrentPhase,pplCurrentPhaseFromDate\n"
    "20,PIPE-1,IN SERVICE,1995-10-01\n"
    "21,PIPE-2,DECOMMISSIONED,\n"
)


def _facility_nodes() -> dict:
    return {
        "Facility": _spec("Facility", junctions=("HAS_STATUS",)),
        "Pipeline": _spec("Pipeline", junctions=("HAS_STATUS",)),
        "FacilityStatus": _spec("FacilityStatus"),
    }


def _fstatus(graph, label: str, name: str, **kwargs) -> list:
    query = (
        f"MATCH (f:{label} {{title: $name}})-[r:HAS_STATUS]->(s:FacilityStatus) "
        "RETURN s.title AS status, r.basis AS basis ORDER BY r.validFrom"
    )
    return [(r["status"], r["basis"]) for r in _rows(graph, query, params={"name": name}, **kwargs)]


def test_facility_status_follows_startup_shutdown_and_removal(tmp_path: Path) -> None:
    _, graph = _build(tmp_path, {"facility": FACILITIES, "pipeline": PIPELINES}, _facility_nodes())
    assert _fstatus(graph, "Facility", "IN-SERVICE") == [("IN SERVICE", "reported")]
    assert _fstatus(graph, "Facility", "GONE", valid_at="1990-01-01") == [("IN SERVICE", "reported")]
    assert _fstatus(graph, "Facility", "GONE", valid_at="2000-01-01") == [("SHUT DOWN", "reported")]
    assert _fstatus(graph, "Facility", "GONE") == [("REMOVED", "reported")]
    assert _fstatus(graph, "Facility", "PLANNED", valid_at="all") == []
    assert _fstatus(graph, "Facility", "STOPPED") == [("SHUT DOWN", "inferred-start")]
    dates = _rows(graph, "MATCH (f:Facility {title: 'GONE'}) RETURN f.fclDateShutdown AS s, f.fclDateRemoved AS r")
    assert [(str(d["s"]), str(d["r"])) for d in dates] == [("1999-01-01", "2005-06-01")]


def test_pipeline_status_is_its_current_phase(tmp_path: Path) -> None:
    _, graph = _build(tmp_path, {"facility": FACILITIES, "pipeline": PIPELINES}, _facility_nodes())
    assert _fstatus(graph, "Pipeline", "PIPE-1") == [("IN SERVICE", "reported")]
    assert _fstatus(graph, "Pipeline", "PIPE-1", valid_at="1995-09-30") == []
    assert _fstatus(graph, "Pipeline", "PIPE-2", valid_at="all") == []
    names = [r["s"] for r in _rows(graph, "MATCH (s:FacilityStatus) RETURN s.title AS s ORDER BY s")]
    assert names == ["IN SERVICE", "REMOVED", "SHUT DOWN"]


def test_facility_cache_without_lifecycle_columns_still_builds(tmp_path: Path) -> None:
    older = "fclNpdidFacility,fclName,fclPhase,fclStartupDate\n10,IN-SERVICE,IN SERVICE,2000-01-01\n"
    _, graph = _build(tmp_path, {"facility": older, "pipeline": PIPELINES}, _facility_nodes())
    assert _fstatus(graph, "Facility", "IN-SERVICE") == [("IN SERVICE", "reported")]


# ── reserves version chains ─────────────────────────────────────────────

FIELDS = "fldNpdidField,fldName\n1,EKOFISK\n2,FRØY\n"
FIELD_RESERVES = (
    "fldNpdidField,fldDateOffResEstDisplay,fldVersion,fldRemainingOE\n"
    "1,1388448000000,2013,140.289\n"
    "1,1419984000000,2014,122.353\n"
    "1,1451520000000,2015,103.194\n"
    # Two versions published under one date: the later version wins.
    "2,1735603200000,2024,1.0\n"
    "2,1735603200000,2025,0.0\n"
)
FIELD_RESERVES_COMPANY = (
    "fldNpdidField,fldName,cmpNpdidCompany,cmpLongName,cmpDateOffResEstDisplay,cmpRemainingOE\n"
    "1,EKOFISK,1,Alpha,2014-12-31,60\n"
    "1,EKOFISK,2,Beta,2014-12-31,40\n"
    "1,EKOFISK,1,Alpha,2015-12-31,100\n"
)
DISCOVERIES = "dscNpdidDiscovery,dscName\n7,D7\n8,D8\n"
DISCOVERY_RESERVES = (
    "dscNpdidDiscovery,dscDateOffResEstDisplay,dscReservesRC,dscRecoverableOe\n"
    # Two resource classes of one version.
    "7,2025-12-31,4F,8.4\n"
    "7,2025-12-31,5F,1.6\n"
    "8,2010-12-31,7F,3.0\n"
    "8,2012-12-31,5F,2.0\n"
)


def _reserves_graph(tmp_path: Path):
    nodes = {
        "Field": _spec("Field", sub_nodes=("FieldReserves", "FieldReservesCompany")),
        "Discovery": _spec("Discovery", sub_nodes=("DiscoveryReserves",)),
    }
    csvs = {
        "field": FIELDS,
        "field_reserves": FIELD_RESERVES,
        "field_reserves_company": FIELD_RESERVES_COMPANY,
        "discovery": DISCOVERIES,
        "discovery_reserves": DISCOVERY_RESERVES,
    }
    return _build(tmp_path, csvs, nodes)[1]


def test_field_reserves_read_the_version_in_force(tmp_path: Path) -> None:
    graph = _reserves_graph(tmp_path)
    query = (
        "MATCH (f:Field {title: $f})<-[:OF_FIELD]-(r:FieldReserves) "
        "RETURN r.fldVersion AS v, r.fldRemainingOE AS oe ORDER BY v"
    )
    assert _rows(graph, query, params={"f": "EKOFISK"}, valid_at="2015-06-30") == [{"v": 2014, "oe": 122.353}]
    assert _rows(graph, query, params={"f": "EKOFISK"}) == [{"v": 2015, "oe": 103.194}]
    assert _rows(graph, query, params={"f": "EKOFISK"}, valid_at="2013-06-30") == []
    assert len(_rows(graph, query, params={"f": "EKOFISK"}, valid_at="all")) == 3
    assert _rows(graph, query, params={"f": "FRØY"}) == [{"v": 2025, "oe": 0.0}]
    window = _rows(
        graph,
        "MATCH (r:FieldReserves {fldVersion: 2014}) RETURN r.existsFrom AS f, r.existsTo AS t",
        valid_at="all",
    )
    assert [(str(w["f"]), str(w["t"])) for w in window] == [("2014-12-31", "2015-12-31")]


def test_company_and_discovery_reserves_chain_per_version(tmp_path: Path) -> None:
    graph = _reserves_graph(tmp_path)
    companies = "MATCH (r:FieldReservesCompany) RETURN r.cmpLongName AS c, r.cmpRemainingOE AS oe ORDER BY c"
    assert _rows(graph, companies, valid_at="2015-06-30") == [{"c": "Alpha", "oe": 60.0}, {"c": "Beta", "oe": 40.0}]
    # Beta is not in the 2015 version: it is no longer in force.
    assert _rows(graph, companies) == [{"c": "Alpha", "oe": 100.0}]
    discovery = (
        "MATCH (d:Discovery {title: $d})<-[:OF_DISCOVERY]-(r:DiscoveryReserves) "
        "RETURN r.dscReservesRC AS rc ORDER BY rc"
    )
    assert _rows(graph, discovery, params={"d": "D7"}) == [{"rc": "4F"}, {"rc": "5F"}]
    assert _rows(graph, discovery, params={"d": "D8"}, valid_at="2011-06-30") == [{"rc": "7F"}]
    assert _rows(graph, discovery, params={"d": "D8"}) == [{"rc": "5F"}]
