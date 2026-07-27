//! `kglite_datasets._disk_graph` — the committed-disk-graph probe, exposed.
//!
//! Thin delegates to `kglite_datasets::disk_graph`, which owns the knowledge
//! of what kglite writes when it publishes a disk graph. Exposed to Python
//! rather than reimplemented there because this repo has already demonstrated
//! what happens otherwise: the same question had three different answers in
//! three files (SEC's Rust layout probed for a filename kglite never writes,
//! Sodir's Rust layout was right, the Wikidata Python wrapper was right
//! again but separately), and only the wrong one was load-bearing.

use pyo3::prelude::*;

use kglite_datasets::disk_graph;
use std::path::Path;

/// The file whose mtime dates the committed graph in `graph_dir`, or `None`
/// if the directory holds no committed graph.
#[pyfunction]
fn commit_marker(graph_dir: String) -> PyResult<Option<String>> {
    Ok(disk_graph::commit_marker(Path::new(&graph_dir)).map(|p| p.to_string_lossy().into_owned()))
}

/// True if `graph_dir` holds a committed kglite disk graph. A probe, not a
/// validation — the caller must still degrade to a rebuild if the load fails.
#[pyfunction]
fn exists(graph_dir: String) -> PyResult<bool> {
    Ok(disk_graph::exists(Path::new(&graph_dir)))
}

pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(py, "_disk_graph")?;
    m.add_function(wrap_pyfunction!(commit_marker, &m)?)?;
    m.add_function(wrap_pyfunction!(exists, &m)?)?;
    parent.add_submodule(&m)?;
    let sys = py.import("sys")?;
    sys.getattr("modules")?
        .set_item("kglite_datasets._disk_graph", m)?;
    Ok(())
}
