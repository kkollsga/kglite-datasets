//! PyO3 wrapper for kglite-datasets.
//!
//! Exposes the SEC / Sodir / Wikidata loaders' fetch + extract surface to
//! Python as `kglite_datasets.kglite_datasets` (imported via the package's
//! `__init__.py`), plus the `_sec_internal` / `_sodir_internal` /
//! `_wikidata_internal` submodules the Python wrappers call. Graph *building*
//! stays in Python — the wrappers call `from kglite import KnowledgeGraph` and
//! feed it the emitted CSVs / blueprint JSON / dump, so this extension does not
//! link the kglite engine.
//!
//! The three binding modules are ported from kglite-py (`src/{sec,sodir,
//! wikidata}.rs`) via the mechanical `kglite_core::api::datasets::` →
//! `kglite_datasets::` transform + the `kglite.` → `kglite_datasets.` module
//! namespace rename.

use pyo3::prelude::*;

mod sec;
mod sodir;
mod wikidata;

/// The native extension module. Renamed to `kglite_datasets.kglite_datasets`
/// by maturin's `module-name`; `__init__.py` does `from .kglite_datasets import *`.
#[pymodule]
fn kglite_datasets(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    // Register the per-loader internal submodules
    // (`kglite_datasets._{sec,sodir,wikidata}_internal`).
    sec::register(py, m)?;
    sodir::register(py, m)?;
    wikidata::register(py, m)?;
    Ok(())
}
