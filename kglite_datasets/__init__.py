"""kglite-datasets — fetch-build-cache dataset loaders for kglite.

Opinionated builders for well-known public datasets. Each loader wraps the
fetch + maintenance + build cycle behind a single entry point so applications
can treat a public dataset as a typed Python value:

    from kglite_datasets.sec import SEC
    from kglite_datasets import sodir, wikidata

Loaders:
    sec      - SEC EDGAR filings (pure-Rust loader, no pandas).
    sodir    - Norwegian Continental Shelf petroleum data.
    wikidata - Wikimedia Foundation's `latest-truthy` RDF dumps.

The Rust fetch/parse/extract stage ships in the bundled native extension; the
graph build reuses the kglite engine (`from kglite import KnowledgeGraph`).
"""

from __future__ import annotations

import importlib
from importlib import metadata as _metadata
from typing import TYPE_CHECKING

# Import the native extension eagerly so it registers the
# `kglite_datasets._{sec,sodir,wikidata}_internal` submodules the wrappers use.
# The extension is only the Rust loader surface (no pandas/pyarrow), so this is
# cheap and safe at package import.
from . import kglite_datasets as _ext  # noqa: F401

try:
    __version__ = _metadata.version("kglite-datasets")
except _metadata.PackageNotFoundError:  # pragma: no cover - source checkout
    __version__ = getattr(_ext, "__version__", "0.1.0")

# The three loader wrappers load LAZILY (PEP 562). Importing `sec` must not drag
# in `sodir`/`wikidata` and their optional pandas/pyarrow stack — loading
# pyarrow after the native extension can crash the dynamic linker on macOS (the
# blast radius this crate was extracted to contain). Access a submodule by name
# (`kglite_datasets.sodir`) to import it on first use.
__all__ = ["sec", "sodir", "wikidata", "__version__"]

if TYPE_CHECKING:
    from . import sec, sodir, wikidata  # noqa: F401


def __getattr__(name: str) -> object:
    if name in ("sec", "sodir", "wikidata"):
        return importlib.import_module(f"{__name__}.{name}")
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted([*__all__, "kglite_datasets"])
