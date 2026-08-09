"""GATE #5 — the built-graph cache: correct probe, safe fallback.

Two coupled defects lived here, and repairing either alone makes things worse:

1. The Rust ``graph_exists(Disk)`` probe looked for ``graph_manifest.json`` —
   a filename kglite has never written — so the disk cache never hit and every
   ``mode="disk"`` open silently rebuilt from scratch.
2. Before kglite 0.15.2, ``_build_graph(mode="disk")`` passed ``save=True`` to
   ``from_blueprint``, but the engine ignored the supplied disk ``path`` and
   committed nothing. The current floor fixes that contract; this suite now
   pins it directly so the workaround cannot return unnoticed.

Fixing (1) on its own would have converted a silent cache miss into a hard
user-facing failure for old or damaged graphs. So the fix also makes the cache
path *degrade*: a cached graph that will not load is a miss, and the caller
rebuilds.

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


def _damage_live_generation(dest: Path) -> None:
    """Corrupt a byte-level file *inside* the published generation.

    Leaves the completion protocol intact — `CURRENT` still names a generation
    that still holds `disk_graph_meta.json` — so the directory legitimately
    probes as a cache hit and only fails when the engine reads it. That is the
    real shape of the case this fallback exists for: directories already on
    disk, written by a version of the engine with a defect, or since damaged.
    Faking it with a dangling pointer would *not* reproduce it, because the
    probe correctly rejects that before any load is attempted.
    """
    generation = (dest / "CURRENT").read_text(encoding="utf-8").strip()
    (dest / "generations" / generation / "interner.bin.zst").write_bytes(b"\0" * 8)


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


def test_disk_probe_dereferences_the_pointer(tmp_path: Path) -> None:
    """``CURRENT`` naming a generation that is not there is not a cache hit.

    kglite writes ``CURRENT`` last, by atomic rename, only after the staged
    generation is verified and fsync'd — so *kglite* never leaves a dangling
    pointer. Users do: a half-deleted directory, a partial backup restore, a
    copy that did not follow into ``generations/``. Trusting the pointer's
    presence would report those as cache hits and then fail on load; one extra
    stat turns them into an ordinary miss.
    """
    d = _disk_dir(tmp_path)
    d.mkdir(parents=True)
    generation = "gen_00000000000000000001"
    (d / "CURRENT").write_text(f"{generation}\n", encoding="utf-8")
    assert not _sec_internal.graph_exists(str(tmp_path), "disk"), "dangling pointer"

    gen_dir = d / "generations" / generation
    gen_dir.mkdir(parents=True)
    assert not _sec_internal.graph_exists(str(tmp_path), "disk"), "generation without metadata"

    (gen_dir / "disk_graph_meta.json").write_text("{}", encoding="utf-8")
    assert _sec_internal.graph_exists(str(tmp_path), "disk")


def test_memory_probe_tracks_the_kgl_file(tmp_path: Path) -> None:
    d = Path(_sec_internal.graph_dir(str(tmp_path), "memory"))
    d.mkdir(parents=True)
    assert not _sec_internal.graph_exists(str(tmp_path), "memory")
    (d / "sec.kgl").write_bytes(b"")
    assert _sec_internal.graph_exists(str(tmp_path), "memory")


# ── the fallback ─────────────────────────────────────────────────────────


def test_unloadable_cache_degrades_to_none(tmp_path: Path) -> None:
    """A directory that probes as a graph but will not load yields ``None``.

    ``None`` is the caller's signal to rebuild. Without it the corrected probe
    would surface engine load errors straight to the user, who would then have
    to delete the workdir by hand before their code could run again — and
    old engine versions could write caches that passed the commit probe but
    could not be reopened. The current floor repairs and recovers the known
    zero-row/unknown-label case, but arbitrary truncation or corruption is
    still invisible to a probe.
    """
    d = _disk_dir(tmp_path)
    _write_real_disk_graph(d)
    _damage_live_generation(d)
    assert _sec_internal.graph_exists(str(tmp_path), "disk"), "probe must still fire"

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

    empty, dangling, broken, good = (tmp_path / n for n in ("empty", "dangling", "broken", "good"))

    assert _load_cached_graph(empty, "disk") is None, "no cache at all"

    # Rejected by the probe, before any load is attempted — so no warning.
    d = _disk_dir(dangling)
    d.mkdir(parents=True)
    (d / "CURRENT").write_text("gen_00000000000000000009\n", encoding="utf-8")
    assert _load_cached_graph(dangling, "disk") is None, "dangling pointer"

    # Passes the probe, fails the load — the warning path.
    d = _disk_dir(broken)
    _write_real_disk_graph(d)
    _damage_live_generation(d)
    with pytest.warns(StaleGraphCacheWarning):
        assert _load_cached_graph(broken, "disk") is None, "unloadable cache"

    _write_real_disk_graph(_disk_dir(good))
    assert _load_cached_graph(good, "disk") is not None, "usable cache"


# ── the two halves together ──────────────────────────────────────────────


def test_disk_build_commits_and_reopens(tmp_path: Path) -> None:
    """Build SEC ``mode="disk"`` for real, then reopen it the way the wrapper
    does. Three things pin the current engine floor:

    1. ``from_blueprint(save=True, storage="disk", path=...)`` commits even
       though the blueprint has no ``output`` key (fixed in kglite 0.15.2).
    2. The probe then reports a cache hit.
    3. The zero-row node types in the SEC blueprint survive reopen (fixed in
       kglite 0.15.1) with the same node count.
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

    assert reopened is not None, "the declared kglite floor must reopen an SEC disk graph containing empty node types"
    assert not caught, f"a healthy floor-version cache emitted warnings: {[str(w.message) for w in caught]}"
    assert reopened.graph_info().get("node_count") == built_nodes


def test_mapped_build_reopens_mapped(tmp_path: Path) -> None:
    """Build SEC ``mode="mapped"`` for real, then reopen it the way the wrapper
    does, and pin that the reopened graph is *still mapped*.

    The wrapper has always built and saved this mode with ``storage="mapped"``,
    but before kglite 0.15.8 a saved ``.kgl`` recorded no storage mode, so
    ``_load_cached_graph`` handed back a **memory** graph — every call after the
    one that built the workdir quietly ignored the mode the caller asked for.
    Nothing caught it because the reopened graph is otherwise identical, so the
    regression is invisible to a topology digest; only ``storage_mode`` shows
    it. That is why this asserts the mode and not just the node count.
    """
    import gc

    from kglite_datasets.sec.wrapper import _build_graph, _load_cached_graph
    from kglite_datasets.tests.synth_sec import write_synth_raw

    write_synth_raw(tmp_path)
    _sec_internal.extract_all_py(str(tmp_path), force=True)

    g = _build_graph(tmp_path, "mapped", verbose=False)
    built_nodes = g.graph_info().get("node_count")
    assert built_nodes and built_nodes > 0
    del g
    gc.collect()

    assert _sec_internal.graph_exists(str(tmp_path), "mapped"), "probe must see the saved build"

    reopened = _load_cached_graph(tmp_path, "mapped")
    assert reopened is not None, "the declared kglite floor must reopen a saved mapped SEC graph"
    assert reopened.graph_info().get("node_count") == built_nodes
    assert reopened.graph_info().get("storage_mode") == "mapped", (
        "a mapped-saved cache must reopen mapped; `None` means the engine predates the "
        "recorded storage mode (the key is absent before kglite 0.15.8, which reopened "
        "such a graph as memory), and 'memory' means it recorded the mode but dropped it"
    )
