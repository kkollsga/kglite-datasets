"""GATE #3 — frozen golden for the *built graph* (the kglite boundary).

GATE #1 (``test_parity_golden.py``) freezes the CSVs our Rust loaders emit.
That is where this crate's own code ends — but it is not where the user's
graph ends. Everything after the CSV boundary (blueprint -> ``from_blueprint``
-> ``.kgl`` -> ``load``) is kglite's, and until this file existed the offline
suite asserted **nothing** about it: a kglite upgrade could silently change
node counts, edge counts, or the round-trip and every test still passed.

So this gate freezes the shape of the graph the user actually receives, and
exercises the create-then-reopen path the SEC wrapper depends on
(``_build_graph`` -> ``save`` -> ``_load_cached_graph``).

Verified identical across kglite 0.13.0, 0.14.5 and 0.15.0 (2026-07-27) — the
digest is engine-version-stable by construction: it covers node/edge topology,
not serialization bytes. The ``.kgl`` file *size* deliberately is **not**
asserted; it legitimately changed between 0.13 and 0.14 (153457 -> 149087
bytes) with identical graph content.

Offline: no network.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path

import pytest

from kglite_datasets import _sec_internal
from kglite_datasets.tests.synth_sec import write_synth_raw

kglite = pytest.importorskip("kglite", reason="kglite engine not installed")

GOLDEN = Path(__file__).resolve().parents[2] / "tests" / "goldens" / "sec-graph-build.sha256"


def _fingerprint(g) -> str:
    """Deterministic rendering of a built graph's topology.

    Sorted client-side: ``ORDER BY`` on a list-valued expression (``labels(n)``)
    is not a defined ordering, so engine row order is not a stable key.
    """

    def rows(query: str) -> list:
        out = g.cypher(query).to_list()
        return sorted(out, key=lambda r: json.dumps(r, sort_keys=True, default=str))

    parts = [
        rows("MATCH (n) RETURN labels(n) AS l, count(*) AS c"),
        rows("MATCH ()-[r]->() RETURN type(r) AS t, count(*) AS c"),
        g.cypher("MATCH (n) RETURN n.id AS id, labels(n) AS l ORDER BY id LIMIT 25").to_list(),
    ]
    return json.dumps(parts, sort_keys=True, default=str)


def _digest(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def _build_memory_graph(tmp_path: Path):
    """Reproduce ``sec/wrapper.py::_build_graph`` mode='memory'."""
    from kglite import from_blueprint

    from kglite_datasets.sec.wrapper import _blueprint_with_root, _load_blueprint

    wd = write_synth_raw(tmp_path)
    _sec_internal.extract_all_py(str(wd), force=True)
    compiled = wd / "_sec_compiled_blueprint.json"
    compiled.write_text(json.dumps(_blueprint_with_root(_load_blueprint(), wd)))
    try:
        return from_blueprint(str(compiled), verbose=False, save=False)
    finally:
        compiled.unlink(missing_ok=True)


def test_graph_build_golden(tmp_path: Path) -> None:
    """The built graph's topology matches the frozen digest.

    A drift here means the graph our users get changed — either our blueprint /
    CSVs moved (GATE #1 would usually catch that first) or the kglite engine's
    build semantics changed under us. Investigate before re-freezing;
    ``UPDATE_GOLDEN=1`` recaptures, and only belongs in the same commit as the
    deliberate change that caused the drift.
    """
    g = _build_memory_graph(tmp_path)
    got = _digest(_fingerprint(g))

    if os.environ.get("UPDATE_GOLDEN") or not GOLDEN.exists():
        GOLDEN.write_text(got + "\n")
        pytest.skip(f"froze golden {GOLDEN.name} = {got}")

    want = GOLDEN.read_text().strip()
    assert got == want, (
        f"built-graph golden drifted ({GOLDEN.name}). The graph produced from an "
        f"unchanged input changed shape — check the kglite version and the "
        f"blueprint before re-freezing with UPDATE_GOLDEN=1."
    )


def test_save_reload_round_trip(tmp_path: Path) -> None:
    """``.kgl`` save -> ``load`` returns an identical graph.

    This is the SEC wrapper's cache-hit contract (``_build_graph`` writes
    ``sec.kgl``; a later ``SEC.open()`` returns ``_load_cached_graph``'s reopen).
    Note the reopen deliberately uses ``kglite.load(path)`` with no ``storage=``
    kwarg: a ``.kgl`` checkpoint records no storage mode, and since kglite 0.15
    passing a ``storage=`` that disagrees with the loaded backend raises
    ``kglite.ArgumentError`` instead of being silently ignored.
    """
    g = _build_memory_graph(tmp_path)
    before = _fingerprint(g)

    kgl = tmp_path / "graph" / "sec.kgl"
    kgl.parent.mkdir(parents=True, exist_ok=True)
    g.save(str(kgl))

    reloaded = kglite.load(str(kgl))
    assert _fingerprint(reloaded) == before, "graph changed across save -> load round trip"
