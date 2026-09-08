"""Offline kglite-boundary coverage for the Sodir wrapper."""

from __future__ import annotations

import gc
import json
from pathlib import Path
import shutil

from kglite_datasets import _sodir_internal
from kglite_datasets._cache import load_cached_graph
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


def test_discovery_play_membership_and_existing_joins(tmp_path: Path) -> None:
    csv_dir = tmp_path / "csv"
    csv_dir.mkdir()
    (csv_dir / "discovery.csv").write_text(
        "dscNpdidDiscovery,dscName,wlbNpdidWellbore,fldNpdidField\n10,NJU-style,100,500\n"
    )
    (csv_dir / "wellbore.csv").write_text(
        "wlbNpdidWellbore,wlbWellboreName,dscNpdidDiscovery,fldNpdidField,"
        "wlbAgeWithHc1,wlbAgeWithHc2,wlbAgeWithHc3,wkt_geometry\n"
        "100,SYNTH-1,10,500,EARLY JURASSIC,INDETERMINATE,,POINT (2 61)\n"
    )
    (csv_dir / "field.csv").write_text("fldNpdidField,fldName\n500,Synthetic field\n")
    (csv_dir / "play.csv").write_text(
        "plyNPDID,plyName,plyAge,wkt_geometry\n"
        '900,Synthetic play,Lower-Middle Jurassic,"POLYGON ((1 60, 3 60, 3 62, 1 62, 1 60))"\n'
    )
    now = "2026-09-08T10:00:00+00:00"
    entries = {}
    for stem in ("discovery", "wellbore", "field", "play"):
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
    nodes = {name: packaged["nodes"][name] for name in ("Field", "Wellbore", "Play", "Discovery")}
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
