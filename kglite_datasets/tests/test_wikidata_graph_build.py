"""Offline kglite-boundary coverage for the Wikidata wrapper."""

from __future__ import annotations

import gc
from pathlib import Path

from kglite_datasets import wikidata
from kglite_datasets._cache import load_cached_graph

FIXTURE = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "wikidata" / "tiny.nt"


def _douglas_adams_title(graph) -> list[dict]:
    return graph.cypher("MATCH (n {nid: 'Q42'}) RETURN n.title AS title").to_list()


def test_memory_build_loads_ntriples() -> None:
    graph = wikidata._build_memory_graph(FIXTURE, ("en",), None, False)

    assert _douglas_adams_title(graph) == [{"title": "Douglas Adams"}]
    assert graph.cypher("MATCH ()-[r:P27]->() RETURN count(r) AS n").to_list() == [{"n": 1}]


def test_disk_build_saves_and_reopens(tmp_path: Path) -> None:
    graph = wikidata._build_graph(tmp_path, FIXTURE, None, ("en",), None, False)
    before = _douglas_adams_title(graph)
    graph_dir = tmp_path / "graph"

    assert (graph_dir / "CURRENT").is_file()
    assert (graph_dir / wikidata.SOURCE_META_FILENAME).is_file()

    del graph
    gc.collect()
    reopened = load_cached_graph(graph_dir, label="Wikidata")

    assert reopened is not None
    assert _douglas_adams_title(reopened) == before
