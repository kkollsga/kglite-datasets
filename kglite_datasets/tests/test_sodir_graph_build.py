"""Offline kglite-boundary coverage for the Sodir wrapper."""

from __future__ import annotations

import gc
from pathlib import Path
import shutil

from kglite_datasets._cache import load_cached_graph
from kglite_datasets.sodir.wrapper import _build_graph

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
