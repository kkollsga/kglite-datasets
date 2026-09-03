"""GATE — every key in a shipped blueprint is a key the engine reads.

kglite's blueprint parser drops any key it does not recognise. Until 0.16.22
it dropped them in silence: a misspelled ``"lables"`` cost every label it
carried, the build reported success, and the graph the author described was
not the graph they got. 0.16.22 turned that into a build-report warning — but
a warning on stderr during a user's fetch is not a gate, and our blueprints
ship inside the wheel, so a typo in one is a defect in a released artifact.

This test is that gate. It is deliberately spelled as a data check rather than
a build, because the failure it guards has nothing to do with the CSVs: a
blueprint is wrong the moment it is written, not the moment it is loaded.

The accepted key sets below mirror ``crates/kglite/src/graph/blueprint/
schema.rs`` (``ACCEPTED_BLUEPRINT_KEYS`` and friends) in the kglite version at
the declared floor. Drift is one-directional and safe: a key kglite *adds*
cannot make this test wrong until we start using it, at which point the test
fails loudly and this list gets the new name.

Offline: no network, no graph build.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

PACKAGE_ROOT = Path(__file__).resolve().parents[1]

ACCEPTED_BLUEPRINT_KEYS = frozenset({"settings", "nodes", "compute", "ontology"})
ACCEPTED_SETTINGS_KEYS = frozenset({"input_root", "root", "output_path", "output_file", "output", "auto_purge"})
ACCEPTED_NODE_KEYS = frozenset(
    {
        "csv",
        "pk",
        "title",
        "parent",
        "parent_fk",
        "properties",
        "labels",
        "skipped",
        "filter",
        "connections",
        "sub_nodes",
        "timeseries",
    }
)
ACCEPTED_FK_EDGE_KEYS = frozenset({"target", "fk", "properties", "property_types", "rename"})
ACCEPTED_JUNCTION_EDGE_KEYS = frozenset(
    {
        "csv",
        "source_fk",
        "target",
        "target_fk",
        "target_type_column",
        "properties",
        "property_types",
        "rename",
    }
)

SHIPPED_BLUEPRINTS = [
    PACKAGE_ROOT / "sec" / "blueprint.json",
    PACKAGE_ROOT / "sodir" / "blueprint.json",
]


def _unknown(where: str, spec: dict, accepted: frozenset[str]) -> list[str]:
    return [f"{where}: unknown key {k!r}" for k in spec if k not in accepted]


def _walk_node(where: str, spec: dict) -> list[str]:
    bad = _unknown(where, spec, ACCEPTED_NODE_KEYS)
    connections = spec.get("connections") or {}
    for edge_type, fk in (connections.get("fk_edges") or {}).items():
        bad += _unknown(f"{where} fk_edge {edge_type!r}", fk, ACCEPTED_FK_EDGE_KEYS)
    for edge_type, junc in (connections.get("junction_edges") or {}).items():
        bad += _unknown(f"{where} junction_edge {edge_type!r}", junc, ACCEPTED_JUNCTION_EDGE_KEYS)
    for sub_type, sub in (spec.get("sub_nodes") or {}).items():
        bad += _walk_node(f"{where} sub_node {sub_type!r}", sub)
    return bad


@pytest.mark.parametrize("path", SHIPPED_BLUEPRINTS, ids=lambda p: p.parent.name)
def test_shipped_blueprint_declares_only_keys_the_engine_reads(path: Path) -> None:
    blueprint = json.loads(path.read_text(encoding="utf-8"))

    bad = _unknown(path.parent.name, blueprint, ACCEPTED_BLUEPRINT_KEYS)
    bad += _unknown(f"{path.parent.name} settings", blueprint.get("settings") or {}, ACCEPTED_SETTINGS_KEYS)
    for node_type, spec in (blueprint.get("nodes") or {}).items():
        bad += _walk_node(f"{path.parent.name} node {node_type!r}", spec)

    assert not bad, "kglite ignores these keys, so whatever they declare is not in the built graph:\n  " + "\n  ".join(
        bad
    )


def test_the_gate_can_fail() -> None:
    """A typo must be caught wherever it is written, not only at the top."""
    typo = {
        "nodes": {
            "Field": {
                "csv": "csv/field.csv",
                "lables": ["Asset"],
                "connections": {"fk_edges": {"IN_BLOCK": {"target": "Block", "fk": "b", "note": "x"}}},
                "sub_nodes": {"FieldReserves": {"csv": "csv/r.csv", "spatial_only": True}},
            }
        }
    }
    found = _walk_node("node 'Field'", typo["nodes"]["Field"])
    assert len(found) == 3, found
    assert any("'lables'" in f for f in found)
    assert any("fk_edge 'IN_BLOCK'" in f and "'note'" in f for f in found)
    assert any("sub_node 'FieldReserves'" in f and "'spatial_only'" in f for f in found)
