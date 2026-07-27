"""Shared cache-reopen policy for the loader wrappers.

Every loader caches a built graph and re-opens it on the next call instead of
rebuilding. The reopen has two steps — *probe* ("did a build finish here?")
then *load* — and the second step can fail for reasons the probe cannot see:
the cache was written by an engine whose on-disk format has since changed, the
directory was truncated by a killed process or a full disk, or the engine has a
bug that makes a particular graph unreadable after it was successfully written.

**A cache that will not load is a cache miss, not an error.** The graph is
reproducible from `raw/` by construction, so rebuilding always converges;
propagating the load error instead strands the user on a workdir they must
manually delete before their code runs again. This module is the one place
that decision is made, so all three loaders degrade the same way.

The counterpart on the Rust side is :mod:`kglite_datasets.disk_graph` (crate
module ``disk_graph``), which owns the probe.
"""

from __future__ import annotations

from pathlib import Path
from typing import Optional
import warnings

from kglite import KnowledgeGraph, load

from kglite_datasets import _disk_graph


def commit_marker(graph_dir: Path | str) -> Optional[Path]:
    """The file whose mtime dates the committed kglite disk graph in
    ``graph_dir``, or ``None`` when nothing is committed.

    A thin pass-through to the Rust ``disk_graph`` module — the single place
    that knows what kglite writes on publish. Python callers go through here
    instead of re-deriving it, which is how the SEC loader ended up probing
    for a filename the engine has never written.
    """
    marker = _disk_graph.commit_marker(str(graph_dir))
    return Path(marker) if marker is not None else None


def graph_exists(graph_dir: Path | str) -> bool:
    """True when ``graph_dir`` holds a committed kglite disk graph.

    A probe, not a validation — see :func:`load_cached_graph` for why the
    caller must still be able to fall back to a rebuild.
    """
    return bool(_disk_graph.exists(str(graph_dir)))


class StaleGraphCacheWarning(UserWarning):
    """A cached graph existed but could not be re-opened, so it is being
    rebuilt. Not an error — but it is worth surfacing, because a cache that
    consistently fails to reload means every "cached" open is silently paying
    the full build cost."""


def load_cached_graph(target: Path | str, *, label: str, verbose: bool = False) -> Optional[KnowledgeGraph]:
    """Re-open a cached graph, or return ``None`` so the caller rebuilds.

    ``target`` is whatever :func:`kglite.load` accepts for the mode in play —
    a ``.kgl`` file for memory/mapped, a graph directory for disk.

    Returns ``None`` (and warns :class:`StaleGraphCacheWarning`) when the load
    raises. Only ``Exception`` is caught: a ``KeyboardInterrupt`` during a slow
    mmap open must still interrupt, not be reinterpreted as a stale cache.
    """
    target = Path(target)
    try:
        g = load(str(target))
    except Exception as exc:  # noqa: BLE001 - deliberate: any load failure means "rebuild"
        warnings.warn(
            f"{label}: cached graph at {target} could not be re-opened ({type(exc).__name__}: {exc}). "
            f"Rebuilding from source. Delete the path to silence this permanently.",
            StaleGraphCacheWarning,
            stacklevel=2,
        )
        return None
    if verbose:
        print(f"  {label}: loaded cached graph at {target}.")
    return g
