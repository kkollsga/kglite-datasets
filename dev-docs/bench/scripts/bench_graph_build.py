"""Engine-boundary bench: SEC blueprint -> `from_blueprint` -> `.kgl` -> `load`.

`benchmarks/bench_sec_extract.py` covers our own Rust extract, which links no
engine — it is the **unchanged-path control cell** for an engine bump. This
harness covers the half a kglite upgrade can actually move: the graph build,
the save, and the reopen.

A cold build is a *once-per-event* cost (CLAUDE.md "Performance protocol"), so
this reports the **mean of first events** across fresh instances, never `min`
— a warm-cache `min` is a run no user ever sees. Size is measured two
independent ways (node/edge counts and on-disk bytes); the routes diverge
exactly when the instrument is broken.

Offline: builds from the synthetic fixture, never a live fetch.
Writes heavy artifacts to `dev-docs/bench/out/`, numbers to stdout.

    .venv/bin/python dev-docs/bench/scripts/bench_graph_build.py [rounds]
"""

from __future__ import annotations

import json
import statistics
import sys
import tempfile
import time
from pathlib import Path

import kglite

from kglite_datasets.tests.test_graph_build_golden import _build_memory_graph

ROUNDS = int(sys.argv[1]) if len(sys.argv) > 1 else 7


def _one_event(out_dir: Path) -> dict:
    """One cold build+save+load in a fresh workdir."""
    with tempfile.TemporaryDirectory(dir=out_dir) as td:
        t0 = time.perf_counter()
        g = _build_memory_graph(Path(td))
        build_ms = (time.perf_counter() - t0) * 1000

        kgl = Path(td) / "sec.kgl"
        t0 = time.perf_counter()
        g.save(str(kgl))
        save_ms = (time.perf_counter() - t0) * 1000
        kgl_bytes = kgl.stat().st_size

        t0 = time.perf_counter()
        reloaded = kglite.load(str(kgl))
        load_ms = (time.perf_counter() - t0) * 1000

        nodes = reloaded.cypher("MATCH (n) RETURN count(*) AS c").to_list()[0]["c"]
        edges = reloaded.cypher("MATCH ()-[r]->() RETURN count(*) AS c").to_list()[0]["c"]

    return {
        "build_ms": build_ms,
        "save_ms": save_ms,
        "load_ms": load_ms,
        "kgl_bytes": kgl_bytes,
        "nodes": nodes,
        "edges": edges,
    }


def main() -> None:
    out_dir = Path(__file__).resolve().parents[1] / "out"
    out_dir.mkdir(parents=True, exist_ok=True)

    events = [_one_event(out_dir) for _ in range(ROUNDS)]

    print(f"SEC graph build (kglite {kglite.__version__}, {ROUNDS} fresh instances)")
    for metric in ("build_ms", "save_ms", "load_ms"):
        vals = [e[metric] for e in events]
        print(
            f"  {metric:<9}: mean {statistics.mean(vals):7.2f}  "
            f"median {statistics.median(vals):7.2f}  min {min(vals):7.2f}  max {max(vals):7.2f}"
        )

    sizes = {e["kgl_bytes"] for e in events}
    counts = {(e["nodes"], e["edges"]) for e in events}
    assert len(sizes) == 1, f"non-deterministic .kgl size: {sorted(sizes)}"
    assert len(counts) == 1, f"non-deterministic topology: {sorted(counts)}"
    (nodes, edges) = counts.pop()
    print(f"  topology : {nodes} nodes, {edges} edges  (route 1: counts)")
    print(f"  size     : {sizes.pop()} bytes on disk    (route 2: bytes)")

    print(json.dumps({"version": kglite.__version__, "events": events}))


if __name__ == "__main__":
    main()
