"""GATE #5 — the built-graph cache: correct probe, safe fallback.

Two coupled defects lived here, and repairing either alone makes things worse:

1. The Rust ``graph_exists(Disk)`` probe looked for ``graph_manifest.json`` —
   a filename kglite has never written — so the disk cache never hit and every
   ``mode="disk"`` open silently rebuilt from scratch.
2. ``_build_graph(mode="disk")`` passed ``save=True`` to ``from_blueprint``,
   which only writes to the path named by the blueprint's ``output`` key. Our
   blueprint has none, so the call was a no-op: nothing was ever committed to
   the graph directory in the first place.

Fixing (1) on its own would have converted a silent cache miss into a hard
user-facing failure for any graph kglite cannot currently reopen. So the fix
also makes the cache path *degrade*: a cached graph that will not load is a
miss, and the caller rebuilds.

Everything here is offline — no network, no live SEC.
"""

from __future__ import annotations

from pathlib import Path
import warnings

import pytest

from kglite_datasets import _sec_internal
from kglite_datasets._cache import StaleGraphCacheWarning, load_cached_graph

kglite = pytest.importorskip("kglite", reason="kglite engine not installed")


def _disk_dir(workdir: Path) -> Path:
    return Path(_sec_internal.graph_dir(str(workdir), "disk"))


def _write_real_disk_graph(dest: Path) -> None:
    """Commit a real (tiny) kglite disk graph into ``dest``.

    Deliberately built with the engine's own API rather than through the SEC
    blueprint: the point is to pin what a *committed kglite disk graph* looks
    like on disk, independent of whether the SEC blueprint currently produces
    a reloadable one.
    """
    dest.mkdir(parents=True, exist_ok=True)
    g = kglite.KnowledgeGraph(storage="disk", path=str(dest))
    # Cypher CREATE rather than `add_nodes`: this crate is deliberately
    # pandas-free, and `add_nodes` takes a DataFrame.
    g.cypher("CREATE (:Thing {id: 1, name: 'a'}) CREATE (:Thing {id: 2, name: 'b'})")
    # `save()` is what publishes the root CURRENT pointer; without it the
    # directory holds only uncommitted segment files.
    g.save(str(dest))
    del g


# ── the probe ────────────────────────────────────────────────────────────


def test_disk_probe_sees_a_real_kglite_graph(tmp_path: Path) -> None:
    """A committed kglite disk graph must register as a cache hit.

    This is the half that was broken: the probe looked for a file kglite never
    writes, so this assertion failed for every real graph and disk-mode
    callers rebuilt on every single open.
    """
    d = _disk_dir(tmp_path)
    assert not _sec_internal.graph_exists(str(tmp_path), "disk")

    _write_real_disk_graph(d)
    assert (d / "CURRENT").is_file(), "kglite publishes a root CURRENT pointer"
    assert _sec_internal.graph_exists(str(tmp_path), "disk")


def test_disk_probe_ignores_an_uncommitted_build(tmp_path: Path) -> None:
    """Bare segment files with no commit marker are not a cache hit.

    An interrupted (or never-``save()``d) disk build leaves ``seg_*/`` mmap
    files behind. Treating those as a cache would hand the caller a directory
    ``kglite.load`` refuses.
    """
    d = _disk_dir(tmp_path)
    (d / "seg_000").mkdir(parents=True)
    (d / "seg_000" / "node_slots.bin").write_bytes(b"\0" * 16)
    (d / ".kglite.lock").write_text("0", encoding="utf-8")
    assert not _sec_internal.graph_exists(str(tmp_path), "disk")

    # The phantom filename the probe used to look for must never count.
    (d / "graph_manifest.json").write_text("{}", encoding="utf-8")
    assert not _sec_internal.graph_exists(str(tmp_path), "disk")


def test_memory_probe_tracks_the_kgl_file(tmp_path: Path) -> None:
    d = Path(_sec_internal.graph_dir(str(tmp_path), "memory"))
    d.mkdir(parents=True)
    assert not _sec_internal.graph_exists(str(tmp_path), "memory")
    (d / "sec.kgl").write_bytes(b"")
    assert _sec_internal.graph_exists(str(tmp_path), "memory")


# ── the fallback ─────────────────────────────────────────────────────────


def test_unloadable_cache_degrades_to_none(tmp_path: Path) -> None:
    """A directory that probes as a graph but will not load yields ``None``.

    ``None`` is the caller's signal to rebuild. Without this the corrected
    probe would surface engine load errors (e.g. the zero-row disk defect in
    CHANGELOG "Known issues") straight to the user, who would then have to
    delete the workdir by hand before their code could run again.
    """
    d = _disk_dir(tmp_path)
    d.mkdir(parents=True)
    # Probes as committed (a root CURRENT pointer) but names a generation that
    # does not exist — exactly the shape of a truncated or half-published dir.
    (d / "CURRENT").write_text("gen_00000000000000000001\n", encoding="utf-8")
    assert _sec_internal.graph_exists(str(tmp_path), "disk"), "probe must fire"

    with pytest.warns(StaleGraphCacheWarning):
        assert load_cached_graph(d, label="SEC") is None


def test_loadable_cache_is_returned(tmp_path: Path) -> None:
    """The fallback must not swallow a *working* cache."""
    d = _disk_dir(tmp_path)
    _write_real_disk_graph(d)
    g = load_cached_graph(d, label="SEC")
    assert g is not None
    assert g.cypher("MATCH (n:Thing) RETURN count(n) AS n").to_list()[0]["n"] == 2


def test_sec_wrapper_cache_helper_degrades(tmp_path: Path) -> None:
    """The wrapper's own cache step returns ``None`` for an unusable cache.

    ``SEC.open`` treats ``None`` as "no cache" and falls through to the fetch
    + build path, so this is the assertion that the wrapper rebuilds instead
    of raising. Exercised at the helper rather than through ``SEC.open``
    because everything past step 0 in ``open`` needs the network.
    """
    from kglite_datasets.sec.wrapper import _load_cached_graph

    empty, broken, good = (tmp_path / n for n in ("empty", "broken", "good"))

    assert _load_cached_graph(empty, "disk") is None, "no cache at all"

    d = _disk_dir(broken)
    d.mkdir(parents=True)
    (d / "CURRENT").write_text("gen_00000000000000000009\n", encoding="utf-8")
    with pytest.warns(StaleGraphCacheWarning):
        assert _load_cached_graph(broken, "disk") is None, "unloadable cache"

    _write_real_disk_graph(_disk_dir(good))
    assert _load_cached_graph(good, "disk") is not None, "usable cache"


# ── the two halves together ──────────────────────────────────────────────


def test_disk_build_commits_and_reopen_never_raises(tmp_path: Path) -> None:
    """Build SEC ``mode="disk"`` for real, then reopen it the way the wrapper
    does. Three things must hold regardless of the engine's state:

    1. The build **commits** — before the fix, ``_build_graph`` relied on
       ``from_blueprint(save=True)``, which only honours the blueprint's
       ``output`` key (we have none), so the graph dir was left with bare
       uncommitted segment files and nothing to cache.
    2. The probe then reports a cache hit.
    3. The reopen never raises. Today it returns ``None`` (the zero-row disk
       defect in CHANGELOG "Known issues" makes the directory unreadable) and
       the wrapper rebuilds; once the engine fix ships it returns the graph and
       the cache starts hitting — with no change needed here.

    Deliberately written to accept both arms, and to *verify* the good arm
    rather than wave it through: if a graph comes back, its node count must
    match what was built. That makes this the test that will notice the engine
    fix landing, instead of silently continuing to exercise the fallback.
    """
    import gc

    from kglite_datasets.sec.wrapper import _build_graph, _load_cached_graph
    from kglite_datasets.tests.synth_sec import write_synth_raw

    write_synth_raw(tmp_path)
    _sec_internal.extract_all_py(str(tmp_path), force=True)

    g = _build_graph(tmp_path, "disk", verbose=False)
    built_nodes = g.graph_info().get("node_count")
    assert built_nodes and built_nodes > 0
    del g
    gc.collect()

    d = _disk_dir(tmp_path)
    assert (d / "CURRENT").is_file(), "disk build must publish a commit marker"
    assert _sec_internal.graph_exists(str(tmp_path), "disk"), "probe must see the committed build"

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        reopened = _load_cached_graph(tmp_path, "disk")

    if reopened is None:
        assert any(issubclass(w.category, StaleGraphCacheWarning) for w in caught), (
            "a cache miss caused by an unreadable cache must be surfaced as a warning"
        )
    else:
        assert reopened.graph_info().get("node_count") == built_nodes
