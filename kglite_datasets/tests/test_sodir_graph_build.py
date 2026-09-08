"""Offline kglite-boundary coverage for the Sodir wrapper."""

from __future__ import annotations

import gc
import hashlib
import json
from pathlib import Path
import shutil

from kglite_datasets import _sodir_internal
from kglite_datasets._cache import load_cached_graph
from kglite_datasets.sodir import wrapper
from kglite_datasets.sodir.wrapper import (
    PACKAGED_BLUEPRINT,
    SOURCE_META_FILENAME,
    _blueprint_digest,
    _build_graph,
    _cached_blueprint_matches,
)

FIXTURE = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "sodir" / "csv-in" / "petreg_licence.csv"


def test_disk_blueprint_build_saves_and_reopens(tmp_path: Path) -> None:
    csv_dir = tmp_path / "csv"
    csv_dir.mkdir()
    shutil.copyfile(FIXTURE, csv_dir / FIXTURE.name)
    graph_dir = tmp_path / "graph"
    graph_dir.mkdir()
    blueprint = {
        "nodes": {
            "Licence": {
                "csv": f"csv/{FIXTURE.name}",
                "pk": "ptlPetregLicenceID",
                "title": "ptlName",
            }
        }
    }

    graph = _build_graph(tmp_path, blueprint, "disk", graph_dir, False)
    before = graph.cypher("MATCH (n:Licence) RETURN n.title AS title ORDER BY title").to_list()
    assert before == [{"title": "Licence 001"}, {"title": "Licence 002"}]
    assert (graph_dir / "CURRENT").is_file()

    del graph
    gc.collect()
    reopened = load_cached_graph(graph_dir, label="Sodir")

    assert reopened is not None
    assert reopened.cypher("MATCH (n:Licence) RETURN n.title AS title ORDER BY title").to_list() == before


def test_relative_workdir_is_not_resolved_twice(tmp_path: Path, monkeypatch) -> None:
    workdir = tmp_path / "relative"
    csv_dir = workdir / "csv"
    csv_dir.mkdir(parents=True)
    shutil.copyfile(FIXTURE, csv_dir / FIXTURE.name)
    monkeypatch.chdir(tmp_path)
    graph = _build_graph(
        Path("relative").resolve(),
        {
            "nodes": {
                "Licence": {
                    "csv": f"csv/{FIXTURE.name}",
                    "pk": "ptlPetregLicenceID",
                    "title": "ptlName",
                }
            }
        },
        "memory",
        None,
        False,
    )
    assert graph.cypher("MATCH (n:Licence) RETURN count(n) AS count").to_list() == [{"count": 2}]


def test_open_normalizes_relative_workdir_before_refresh_and_build(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.chdir(tmp_path)
    seen = []
    monkeypatch.setattr(wrapper, "_resolve_blueprint", lambda *_args: '{"nodes": {}}')
    monkeypatch.setattr(
        wrapper._sodir_internal,
        "refresh",
        lambda path, *_args, **_kwargs: seen.append(Path(path)) or {"fetched": []},
    )
    sentinel = object()
    monkeypatch.setattr(
        wrapper,
        "_build_graph",
        lambda path, *_args: seen.append(path) or sentinel,
    )
    assert wrapper.open("relative", verbose=False) is sentinel
    assert seen == [tmp_path / "relative", tmp_path / "relative"]


def test_press_release_markdown_volume_and_wellbore_link(tmp_path: Path) -> None:
    csv_dir = tmp_path / "csv"
    csv_dir.mkdir()
    url = "https://example.invalid/discovery-release"
    (csv_dir / "wellbore.csv").write_text(
        f"wlbNpdidWellbore,wlbWellboreName,dscNpdidDiscovery,wlbPressReleaseUrl\n10,TEST-1,20,{url}\n"
    )
    (csv_dir / "discovery.csv").write_text("dscNpdidDiscovery,dscName,fldNpdidField\n20,Test discovery,30\n")
    raw_dir = tmp_path / "press_releases" / "raw"
    raw_dir.mkdir(parents=True)
    release_id = hashlib.sha256(url.encode()).hexdigest()
    (raw_dir / f"{release_id}.source").write_text(
        "<html><body><main><article><h1>Test discovery</h1>"
        "<p>The size of the discovery is 3-5 million Sm3 of recoverable oil.</p>"
        "<p>The test flowed 1.2 million Sm3 gas per flow day.</p>"
        "</article></main></body></html>"
    )

    report = wrapper.fetch_press_releases(tmp_path, limit=1, verbose=False)
    assert report == {
        "uncertain_discoveries": 1,
        "eligible_releases": 1,
        "selected": 1,
        "fetched": 0,
        "cached": 1,
        "parsed": 1,
        "failed": 0,
        "documents": 1,
        "wellbore_links": 1,
        "volume_mentions": 1,
    }

    packaged = json.loads(PACKAGED_BLUEPRINT.read_text())
    blueprint = {
        "nodes": {
            "Wellbore": {
                "csv": "csv/wellbore.csv",
                "pk": "wlbNpdidWellbore",
                "title": "wlbWellboreName",
                "connections": {
                    "junction_edges": {
                        "HAS_PRESS_RELEASE": packaged["nodes"]["Wellbore"]["connections"]["junction_edges"][
                            "HAS_PRESS_RELEASE"
                        ]
                    }
                },
            },
            "PressRelease": packaged["nodes"]["PressRelease"],
        }
    }
    graph = _build_graph(tmp_path, blueprint, "memory", None, False)
    assert graph.cypher(
        "MATCH (w:Wellbore)-[:HAS_PRESS_RELEASE]->(p:PressRelease) "
        "MATCH (v:PressReleaseVolume)-[:OF_PRESS_RELEASE]->(p) "
        "RETURN w.title AS well, p.title AS release, v.minimum AS minimum, "
        "v.maximum AS maximum, v.unit AS unit, v.commodity AS commodity, "
        "v.scope AS scope, v.sourceText AS source"
    ).to_list() == [
        {
            "well": "TEST-1",
            "release": "Test discovery",
            "minimum": 3.0,
            "maximum": 5.0,
            "unit": "million_sm3",
            "commodity": "oil",
            "scope": "whole_discovery",
            "source": "The size of the discovery is 3-5 million Sm3 of recoverable oil.",
        }
    ]


def test_discovery_play_membership_and_existing_joins(tmp_path: Path) -> None:
    csv_dir = tmp_path / "csv"
    csv_dir.mkdir()
    (csv_dir / "discovery.csv").write_text(
        "dscNpdidDiscovery,dscName,wlbNpdidWellbore,fldNpdidField\n10,NJU-style,7988,500\n"
    )
    (csv_dir / "wellbore.csv").write_text(
        "wlbNpdidWellbore,wlbWellboreName,dscNpdidDiscovery,fldNpdidField,"
        "wlbAgeWithHc1,wlbAgeWithHc2,wlbAgeWithHc3,wkt_geometry,wlbPressReleaseUrl\n"
        "7988,SYNTH-1,10,500,EARLY JURASSIC,INDETERMINATE,,POINT (2 61),https://factpages.sodir.no/pbl/wellbore_press_releases/7988-36-7-4.pdf\n"
    )
    (csv_dir / "field.csv").write_text("fldNpdidField,fldName\n500,Synthetic field\n")
    (csv_dir / "field_discoveries_incl_hst.csv").write_text(
        "fldNpdidField,dscNpdidDiscovery,fldDiscoveryInclFromDate,fldDiscoveryInclToDate\n500,10,2000-01-01,\n"
    )
    (csv_dir / "field_reserves.csv").write_text(
        "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableGas,"
        "fldRecoverableNGL,fldRecoverableCondensate,fldRecoverableOE\n"
        "500,2025-12-31,10,20,,,30\n"
    )
    (csv_dir / "play.csv").write_text(
        "plyNPDID,plyName,plyAge,wkt_geometry\n"
        '900,Synthetic play,Lower-Middle Jurassic,"POLYGON ((1 60, 3 60, 3 62, 1 62, 1 60))"\n'
    )
    now = "2026-09-08T10:00:00+00:00"
    entries = {}
    for stem in (
        "discovery",
        "wellbore",
        "field",
        "play",
        "field_discoveries_incl_hst",
        "field_reserves",
    ):
        entries[stem] = {
            "kind": "user_supplied",
            "csv_path": f"csv/{stem}.csv",
            "row_count": 1,
            "fetched_at_iso": now,
            "count_checked_at_iso": now,
        }
    (tmp_path / "sodir_index.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "endpoint": "offline-test",
                "last_full_check_iso": now,
                "datasets": entries,
            }
        )
    )
    packaged = json.loads(PACKAGED_BLUEPRINT.read_text())
    nodes = {name: packaged["nodes"][name] for name in ("Field", "Wellbore", "Play", "Discovery", "DiscoveryVolume")}
    for node in nodes.values():
        node["sub_nodes"] = {}
    nodes["Field"]["connections"] = {"fk_edges": {}, "junction_edges": {}}
    nodes["Wellbore"]["connections"]["fk_edges"] = {
        name: edge
        for name, edge in nodes["Wellbore"]["connections"]["fk_edges"].items()
        if name in {"IN_DISCOVERY", "IN_FIELD"}
    }
    nodes["Wellbore"]["connections"]["junction_edges"] = {}
    nodes["Discovery"]["connections"]["fk_edges"] = {
        name: edge
        for name, edge in nodes["Discovery"]["connections"]["fk_edges"].items()
        if name in {"DISCOVERED_BY", "IN_FIELD"}
    }
    nodes["Discovery"]["connections"]["junction_edges"] = {
        "IN_PLAY": nodes["Discovery"]["connections"]["junction_edges"]["IN_PLAY"]
    }
    blueprint = {"nodes": nodes}
    report = _sodir_internal.refresh(
        str(tmp_path),
        json.dumps(blueprint),
        index_cooldown_days=14,
        dataset_cooldown_days=30,
        concurrency=1,
        enhance_discovery_play=True,
    )
    assert report["preprocess"]["discovery_play_links"] == 1
    graph = _build_graph(tmp_path, blueprint, "memory", None, False)
    assert graph.cypher(
        "MATCH (d:Discovery)-[r:IN_PLAY]->(p:Play) "
        "RETURN d.title AS discovery, p.title AS play, r.match_method AS method, "
        "r.age_compatibility AS age, r.distance_m AS metres, r.inferred AS inferred, "
        "r.matched_ages AS matched, r.matched_hc_slots AS slots"
    ).to_list() == [
        {
            "discovery": "NJU-style",
            "play": "Synthetic play",
            "method": "contains",
            "age": "full",
            "metres": 0.0,
            "inferred": True,
            "matched": ["Lower Jurassic"],
            "slots": [1],
        }
    ]
    assert graph.cypher(
        "MATCH (d:Discovery)-[:DISCOVERED_BY]->(w:Wellbore)-[:IN_FIELD]->(f:Field) "
        "RETURN w.title AS well, f.title AS field"
    ).to_list() == [{"well": "SYNTH-1", "field": "Synthetic field"}]
    assert graph.cypher(
        "MATCH (w:Wellbore) WHERE w.wlbNpdidWellbore = 7988 RETURN w.wlbPressReleaseUrl AS url"
    ).to_list() == [{"url": "https://factpages.sodir.no/pbl/wellbore_press_releases/7988-36-7-4.pdf"}]
    assert graph.cypher(
        "MATCH (v:DiscoveryVolume)-[:OF_DISCOVERY]->(d:Discovery) "
        "RETURN d.title AS discovery, v.generated AS generated, v.method AS method, "
        "v.recoverable_oil AS oil, v.recoverable_gas AS gas, v.recoverable_ngl AS ngl, "
        "v.scope AS scope, v.aggregation_key AS aggregation_key, "
        "v.covered_discovery_ids AS covered"
    ).to_list() == [
        {
            "discovery": "NJU-style",
            "generated": True,
            "method": "field_reserves_primary",
            "oil": 10.0,
            "gas": 20.0,
            "ngl": None,
            "scope": "shared_field",
            "aggregation_key": "field:500:2025-12-31:original_recoverable",
            "covered": ["10"],
        }
    ]


def test_replacement_blueprint_cannot_opt_in_by_filename_alone(tmp_path: Path) -> None:
    csv_dir = tmp_path / "csv"
    csv_dir.mkdir()
    caller_csv = csv_dir / "_derived_discovery_play.csv"
    original = "caller,owned\n1,value\n"
    caller_csv.write_text(original)
    replacement = {
        "nodes": {
            "CallerData": {
                "csv": "csv/_derived_discovery_play.csv",
                "pk": "caller",
                "title": "owned",
            }
        }
    }
    report = _sodir_internal.refresh(
        str(tmp_path),
        json.dumps(replacement),
        index_cooldown_days=14,
        dataset_cooldown_days=30,
        concurrency=1,
        enhance_discovery_play=False,
    )
    assert caller_csv.read_text() == original
    assert report["user_supplied"] == ["_derived_discovery_play"]


def test_disk_cache_requires_current_enhancement_version(tmp_path: Path) -> None:
    blueprint = '{"nodes": {}}'
    (tmp_path / SOURCE_META_FILENAME).write_text(
        json.dumps(
            {
                "blueprint_sha256": _blueprint_digest(blueprint),
                "enhancement_version": _sodir_internal.enhancement_version() - 1,
            }
        )
    )
    assert not _cached_blueprint_matches(tmp_path, blueprint)
    (tmp_path / SOURCE_META_FILENAME).write_text("[]")
    assert not _cached_blueprint_matches(tmp_path, blueprint)
