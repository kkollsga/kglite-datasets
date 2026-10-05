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
    # Network-free: every table the refresh wants is supplied here.
    assert not report["unfetchable"] and not report["fetched"], report
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


# ── yearly production profiles ──────────────────────────────────────────

PROFILES = (
    "prfPeriod,prfYear,prfMonth,prfInformationCarrier,prfInformationCarrierKind,prfNpdidInformationCarrier,"
    "prfPrdOilNetMillSm3,prfPrdOeNetMillSm3,prfInvestmentsMillNOK\n"
    "year,2000,0,EKOFISK,FIELD,1,0.0,0.0,356.0\n"
    "year,2001,0,EKOFISK,FIELD,1,0.5,0.6,990.0\n"
    "month,2001,1,EKOFISK,FIELD,1,0.2,0.25,0.0\n"
    "month,2001,2,EKOFISK,FIELD,1,0.3,0.35,0.0\n"
    "year,2001,0,D7,DISCOVERY,7,0.1,0.1,12.0\n"
    "month,2001,1,D7,DISCOVERY,7,0.1,0.1,0.0\n"
)


def test_yearly_profiles_carry_investments(tmp_path: Path) -> None:
    nodes = {
        "Field": _spec("Field", sub_nodes=("ProductionProfile", "ProductionProfileAnnual")),
        "Discovery": _spec("Discovery", sub_nodes=("DiscoveryProduction", "DiscoveryProductionAnnual")),
    }
    csvs = {"field": FIELDS, "discovery": DISCOVERIES, "profiles": PROFILES}
    _, graph = _build(tmp_path, csvs, nodes)
    annual = _rows(
        graph,
        "MATCH (:Field {title: 'EKOFISK'})<-[:OF_FIELD]-(a:ProductionProfileAnnual) "
        "RETURN ts_sum(a.investments) AS inv, ts_sum(a.prd_oe_net) AS oe, ts_count(a.investments) AS n",
    )
    assert annual == [{"inv": 1346.0, "oe": 0.6, "n": 2}]
    monthly = _rows(
        graph,
        "MATCH (:Field {title: 'EKOFISK'})<-[:OF_FIELD]-(p:ProductionProfile) "
        "RETURN ts_sum(p.prd_oe_net) AS oe, ts_count(p.prd_oe_net) AS n",
    )
    assert monthly == [{"oe": 0.6, "n": 2}]
    dsc = _rows(
        graph,
        "MATCH (:Discovery {title: 'D7'})<-[:OF_DISCOVERY]-(a:DiscoveryProductionAnnual) "
        "RETURN ts_sum(a.investments) AS inv",
    )
    assert dsc == [{"inv": 12.0}]
    # The yearly rows are loaded, not dropped as monthly aggregates.
    assert "prfMonth=0" not in json.dumps(graph.graph_info()["build"])


def test_monthly_profiles_have_no_investments_channel() -> None:
    for owner, sub in (("Field", "ProductionProfile"), ("Discovery", "DiscoveryProduction")):
        series = PACKAGED["nodes"][owner]["sub_nodes"][sub]["timeseries"]
        assert "investments" not in series["channels"], sub
        assert "investments" in PACKAGED["nodes"][owner]["sub_nodes"][sub + "Annual"]["timeseries"]["channels"]


# ── enrichment: formations and spatial containment ──────────────────────


def _square(x0: float, y0: float, size: float) -> str:
    x1, y1 = x0 + size, y0 + size
    return f'"POLYGON(({x0} {y0}, {x1} {y0}, {x1} {y1}, {x0} {y1}, {x0} {y0}))"'


STRUCTURAL = (
    "STRUCTID,KODE,LEVEL,NAME,wkt_geometry\n"
    # One element published in two parts under one id: merged.
    f"122,MOTO,3,Møre-Trøndelag Fault Complex,{_square(0, 0, 2)}\n"
    f"122,MOTO,3,Møre-Trøndelag Fault Complex,{_square(10, 0, 2)}\n"
    # Two different elements under one id: the platform keeps it.
    f"24,BJAR,2,Bjarmeland Platform,{_square(20, 0, 2)}\n"
    f"24,MJOL,3,Mjølnir Impact Crater,{_square(30, 0, 2)}\n"
    # 0 is no id: every element under it gets a new one.
    f"0,THOR,3,Thor Iversen Fault Complex,{_square(40, 0, 2)}\n"
    f"0,VKAR,3,Veslekari Dome,{_square(50, 0, 2)}\n"
)
ENRICH_WELLBORES = (
    "wlbNpdidWellbore,wlbWellboreName,wlbFormationWithHc1,wlbFormationWithHc2,wlbFormationWithHc3,wkt_geometry\n"
    '1,W-IN-PART-2,BRENT GP,TARBERT,,"POINT(11 1)"\n'
    '2,W-IN-CRATER,ULA,,,"POINT(31 1)"\n'
    '3,W-OUTSIDE,,,,"POINT(100 1)"\n'
)
ENRICH_DISCOVERIES = (
    "dscNpdidDiscovery,dscName,wlbNpdidWellbore,wlbName,wkt_geometry\n"
    f"7,D-INSIDE,1,W-IN-PART-2,{_square(10.5, 0.5, 0.5)}\n"
    # Straddles the element boundary: not enclosed.
    f"8,D-ACROSS,2,W-IN-CRATER,{_square(31.5, 0.5, 1.0)}\n"
)
STRAT_LITHO = (
    "lsuNpdidLithoStrat,lsuName,lsuLevel\n500,BRENT GP,GROUP\n501,TARBERT FM,FORMATION\n502,ULA FM,FORMATION\n"
)
PLAYS = f"plyNPDID,plyName,plyAge,wkt_geometry\n900,PLAY-A,Middle Jurassic,{_square(-1, -1, 15)}\n"
DISCOVERY_PLAY = (
    "dscNpdidDiscovery,plyNPDID,matched_hc_slots\n"
    # Only HC slot 1 matched the play's age.
    '7,900,"[1]"\n'
)


def _enriched(tmp_path: Path):
    nodes = {
        "StructuralElement": _spec("StructuralElement", junctions=("ENCLOSES",)),
        "Wellbore": _spec("Wellbore"),
        "Discovery": _spec("Discovery", junctions=("HC_IN_FORMATION", "IN_PLAY")),
        "Stratigraphy": _spec("Stratigraphy"),
        "Play": _spec("Play", junctions=("ENCLOSES", "PLAY_HAS_FORMATION")),
    }
    csvs = {
        "structural_elements": STRUCTURAL,
        "wellbore": ENRICH_WELLBORES,
        "discovery": ENRICH_DISCOVERIES,
        "strat_litho": STRAT_LITHO,
        "play": PLAYS,
    }
    # The discovery-play links come from the packaged enhancement; this
    # workdir supplies them directly.
    (tmp_path / "csv").mkdir()
    (tmp_path / "csv" / "_derived_discovery_play.csv").write_text(DISCOVERY_PLAY)
    return _build(tmp_path, csvs, nodes)


def test_structural_element_ids_are_unique(tmp_path: Path) -> None:
    report, graph = _enriched(tmp_path)
    rows = _rows(
        graph,
        "MATCH (s:StructuralElement) RETURN s.id AS id, s.KODE AS k, s.STRUCTID_SOURCE AS src ORDER BY k",
    )
    assert rows == [
        {"id": 24, "k": "BJAR", "src": 24},
        {"id": -3, "k": "MJOL", "src": 24},
        {"id": 122, "k": "MOTO", "src": 122},
        {"id": -1, "k": "THOR", "src": 0},
        {"id": -2, "k": "VKAR", "src": 0},
    ]
    pre = report["preprocess"]
    assert (pre["structural_elements_parts_merged"], pre["structural_elements_ids_reassigned"]) == (1, 3)


def test_encloses_by_spatial_containment(tmp_path: Path) -> None:
    _, graph = _enriched(tmp_path)
    enclosed = _rows(
        graph,
        "MATCH (s:StructuralElement)-[:ENCLOSES]->(x) "
        "RETURN s.KODE AS s, labels(x)[0] AS t, x.title AS x ORDER BY s, x",
    )
    assert enclosed == [
        {"s": "MJOL", "t": "Wellbore", "x": "W-IN-CRATER"},
        # The second part of the merged element holds both.
        {"s": "MOTO", "t": "Discovery", "x": "D-INSIDE"},
        {"s": "MOTO", "t": "Wellbore", "x": "W-IN-PART-2"},
    ]
    plays = _rows(graph, "MATCH (p:Play)-[:ENCLOSES]->(s:StructuralElement) RETURN s.KODE AS s")
    assert plays == [{"s": "MOTO"}]


def test_formation_links_follow_hc_formations(tmp_path: Path) -> None:
    _, graph = _enriched(tmp_path)
    hc = _rows(
        graph,
        "MATCH (d:Discovery)-[r:HC_IN_FORMATION]->(s:Stratigraphy) "
        "RETURN d.title AS d, s.title AS s, r.hc_rank AS rank ORDER BY d, rank",
    )
    # 'TARBERT' and 'ULA' name formations Sodir lists as 'TARBERT FM' / 'ULA FM'.
    assert hc == [
        {"d": "D-ACROSS", "s": "ULA FM", "rank": 1},
        {"d": "D-INSIDE", "s": "BRENT GP", "rank": 1},
        {"d": "D-INSIDE", "s": "TARBERT FM", "rank": 2},
    ]
    play = _rows(
        graph,
        "MATCH (p:Play)-[r:PLAY_HAS_FORMATION]->(s:Stratigraphy) RETURN s.title AS s, r.discovery_count AS n",
    )
    assert play == [{"s": "BRENT GP", "n": 1}]


# ── latest reserves snapshot on Field ───────────────────────────────────


def test_field_carries_latest_reserves_and_produced_oe(tmp_path: Path) -> None:
    nodes = {"Field": _spec("Field")}
    csvs = {"field": FIELDS, "field_reserves": FIELD_RESERVES, "profiles": PROFILES}
    _, graph = _build(tmp_path, csvs, nodes)
    rows = _rows(
        graph,
        "MATCH (f:Field) RETURN f.title AS f, f.fldRemainingOE AS oe, f.fldReservesVersion AS v, "
        "f.fldReservesDate AS d, f.fldProducedOE AS produced ORDER BY f",
    )
    assert [(r["f"], r["oe"], r["v"], str(r["d"]), r["produced"]) for r in rows] == [
        ("EKOFISK", 103.194, 2015, "2015-12-31", 0.6),
        # Two versions under one date: the later version is the latest.
        ("FRØY", 0.0, 2025, "2024-12-31", None),
    ]


# ── main-area casing ────────────────────────────────────────────────────


def test_main_areas_use_the_wellbore_casing(tmp_path: Path) -> None:
    nodes = {
        "Field": _spec("Field"),
        "Discovery": _spec("Discovery", sub_nodes=("DiscoveryPoly",)),
        "Licence": _spec("Licence", sub_nodes=("LicenceTask",)),
        "Block": _spec("Block"),
    }
    csvs = {
        "field": "fldNpdidField,fldName,fldMainArea\n1,EKOFISK,North sea\n",
        "discovery": "dscNpdidDiscovery,dscName,nmaName\n7,D7,Norwegian sea\n",
        "discovery_poly_hst": (
            "dscNpdidDiscovery,dscName,nmaName,dscDateValidFrom,dscDateValidTo\n7,D7,Norwegian sea,2000-01-01,\n"
        ),
        "licence": "prlNpdidLicence,prlName,prlMainArea\n100,PL100,Barents sea\n",
        "licence_task": "prlTaskID,prlNpdidLicence,prlTaskTypeEn,prlMainArea\n1,100,Drill,Barents sea\n",
        "block": "blcNpdidBlock,blcName,blcMainArea\n1,7/1,Barents Sea\n2,7/2,Barents sea\n",
    }
    _, graph = _build(tmp_path, csvs, nodes)
    areas = {
        query.split(":")[1].split(")")[0]: sorted({r["a"] for r in _rows(graph, query)})
        for query in (
            "MATCH (n:Field) RETURN n.fldMainArea AS a",
            "MATCH (n:Discovery) RETURN n.nmaName AS a",
            "MATCH (n:DiscoveryPoly) RETURN n.nmaName AS a",
            "MATCH (n:Licence) RETURN n.prlMainArea AS a",
            "MATCH (n:LicenceTask) RETURN n.prlMainArea AS a",
            "MATCH (n:Block) RETURN n.blcMainArea AS a",
        )
    }
    assert areas == {
        "Field": ["NORTH SEA"],
        "Discovery": ["NORWEGIAN SEA"],
        "DiscoveryPoly": ["NORWEGIAN SEA"],
        "Licence": ["BARENTS SEA"],
        "LicenceTask": ["BARENTS SEA"],
        "Block": ["BARENTS SEA"],
    }


# ── lifecycle windows (plain properties) ────────────────────────────────


def test_lifecycle_windows_are_plain_properties(tmp_path: Path) -> None:
    nodes = {
        "Wellbore": _spec("Wellbore"),
        "Licence": _spec("Licence"),
        "Field": _spec("Field"),
        "Discovery": _spec("Discovery"),
        "SeismicSurvey": _spec("SeismicSurvey"),
    }
    csvs = {
        "wellbore": WELLBORES,
        "licence": (
            "prlNpdidLicence,prlName,prlDateGranted,prlDateValidTo\n"
            "100,PL001,-136771200000,2009-12-31\n"
            "101,PL999,2020-01-01,\n"
        ),
        "field": "fldNpdidField,fldName,fldDiscoveryYear\n1,EKOFISK,1969\n2,NOHIST,1990\n",
        "field_licensee_hst": "fldNpdidField,cmpNpdidCompany,fldLicenseeFrom,fldLicenseeTo\n1,1,1971-01-01,\n",
        "field_activity_status_hst": (
            "fldNpdidField,fldStatus,fldStatusFromDate,fldStatusToDate\n"
            "1,PRODUCING,1971-06-09,\n"
            # Inverted by years: not a fact the field existed then.
            "1,SHUT DOWN,1950-01-01,1940-01-01\n"
        ),
        "discovery": (
            "dscNpdidDiscovery,dscName,dscDiscoveryYear,wlbNpdidWellbore\n"
            # Completion of the discovery well, in the discovery year.
            "7,D7,2007,1\n"
            # The well completed in another year: 1 January of the year.
            "8,D8,2011,1\n"
        ),
        "discovery_operator_hst": "dscNpdidDiscovery,cmpNpdidCompany,dscOperatorFrom,dscOperatorTo\n8,1,2010-05-05,\n",
        "seismic_acquisition": (
            "seaNpdidSurvey,seaName,seaDateStarting,seaPlanFromDate\n50,S-1,2001-03-01,2001-02-01\n51,S-2,,2002-02-01\n"
        ),
    }
    _, graph = _build(tmp_path, csvs, nodes)

    def window(label: str, name: str) -> tuple:
        rows = _rows(
            graph,
            f"MATCH (n:{label} {{title: $name}}) RETURN n.existsFrom AS f, n.existsTo AS t",
            params={"name": name},
        )
        return tuple(None if v is None else str(v) for v in (rows[0]["f"], rows[0]["t"]))

    assert window("Wellbore", "15/9-F-12") == ("2007-06-14", None)
    # Never active: an empty window on the register update date.
    assert window("Wellbore", "NEVER-1") == ("2026-01-01", "2026-01-01")
    # Granted 1965-09-01; Sodir's closed validTo becomes the next day.
    assert window("Licence", "PL001") == ("1965-09-01", "2010-01-01")
    assert window("Licence", "PL999") == ("2020-01-01", None)
    assert window("Field", "EKOFISK") == ("1971-01-01", None)
    assert window("Field", "NOHIST") == ("1990-01-01", None)
    assert window("Discovery", "D7") == ("2007-08-27", None)
    assert window("Discovery", "D8") == ("2010-05-05", None)
    assert window("SeismicSurvey", "S-1") == ("2001-02-01", None)
    assert window("SeismicSurvey", "S-2") == ("2002-02-01", None)
    # Plain properties: nothing is declared, so every node reads as current.
    declared = {r["name"] for r in _rows(graph, "CALL db.temporal.declarations() YIELD name RETURN name")}
    assert not declared & {"Wellbore", "Licence", "Field", "Discovery", "SeismicSurvey"}
    assert _rows(graph, "MATCH (n:Wellbore {title: 'NEVER-1'}) RETURN count(n) AS n") == [{"n": 1}]
