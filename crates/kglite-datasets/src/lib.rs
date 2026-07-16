//! # kglite-datasets
//!
//! Standalone extraction of kglite's domain dataset loaders — SEC EDGAR,
//! Sodir (Norwegian Offshore Directorate), and Wikidata. Each loader
//! **fetches** a public registry and **emits** it to disk (CSV / blueprint
//! JSON / dump) ready to be built into a kglite knowledge graph.
//!
//! The loaders are deliberately **engine-free**: the CSV→graph and
//! N-Triples→graph *build* is done by the caller via kglite's
//! `from_blueprint` / `load_ntriples`. This crate owns only the fetch +
//! parse + emit stage; kglite owns the graph engine (imported as a library
//! at the composition seam — the PyO3 wrapper and the parity/bench tests).
//!
//! Extracted from `kglite::datasets` (kglite ≤ 0.13.x). Public surface
//! mirrors `kglite::api::datasets`; see `dev-docs/plans/dataset-loader-extraction.md`.
//!
//! ## Public API / migration parity
//!
//! The crate-root loader modules **are** the public API — they re-export the
//! same items kglite's `api::datasets::<loader>` facade exposed, so a Rust
//! embedder migrating off kglite maps one-for-one:
//!
//! | kglite (pre-removal)                  | kglite-datasets            |
//! |---------------------------------------|----------------------------|
//! | `kglite::api::datasets::sec::X`       | `kglite_datasets::sec::X`      |
//! | `kglite::api::datasets::sodir::X`     | `kglite_datasets::sodir::X`    |
//! | `kglite::api::datasets::wikidata::X`  | `kglite_datasets::wikidata::X` |
//!
//! There is deliberately **no separate `api` facade module** — the loader
//! modules already expose the stable surface, so a second re-export layer would
//! have no distinct consumer (boundary principle: don't add a layer without
//! one). The only addition over the old facade is [`sec::run_all_at`] (the
//! injectable-clock entry point).

// Shared synchronous HTTP client (ureq + rate gate + retry).
pub mod http;

// Per-loader modules, feature-gated (mirrors kglite's datasets/mod.rs):
#[cfg(feature = "sec")]
pub mod sec;
#[cfg(feature = "sodir")]
pub mod sodir;
#[cfg(feature = "wikidata")]
pub mod wikidata;
