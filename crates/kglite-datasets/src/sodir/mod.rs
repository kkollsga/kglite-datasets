//! Pure-Rust Sodir FactMaps REST loader for kglite knowledge graphs.
//!
//! [transform] Extracted from `kglite::datasets::sodir` (kglite ≤ 0.13.x) via
//! the mechanical `crate::datasets::` → `crate::` rewrite; the module had zero
//! engine coupling. This module is engine-free (no PyO3); the Python bindings
//! live in the sibling `kglite-datasets-py` crate and the Python-facing API is
//! `kglite_datasets.sodir` (wired in Phase 5). The graph *build* uses kglite's
//! `from_blueprint` at the composition seam, not here.
//!
//! It mirrors the `kglite-sec` crate's layered architecture
//! (dependencies flow strictly one direction):
//!
//! ```text
//! lib (public API)
//!   ├── orchestrator  refresh + fetch_all — drives index/client/fetch  [A4]
//!   ├── blueprint     blueprint walk + deep-merge                       [A4]
//!   ├── preprocess    the 4 FK-derivation joins                         [A3]
//!   ├── index         sodir_index.json + two-tier cooldown              [A3]
//!   ├── fetch         paginate ArcGIS GeoJSON → CSV                     [A2]
//!   ├── client        ArcGIS REST client (rate limit + retry)           [A2]
//!   ├── geojson_wkt   GeoJSON → WKT, epoch-ms → ISO date                [A1]
//!   ├── layout        Workdir tiers                                     [A1]
//!   ├── catalog       ~150 dataset stem → (url, layer_id)               [A1]
//!   └── error         SodirError                                        [A1]
//! ```

pub mod blueprint;
pub mod catalog;
pub mod client;
pub mod error;
pub mod fetch;
pub mod geojson_wkt;
pub mod index;
pub mod layout;
pub mod orchestrator;
pub mod preprocess;

pub use blueprint::{datasets_used_by_blueprint, merge_blueprint_json};
pub use error::SodirError;
pub use layout::Workdir;
pub use orchestrator::{fetch_all, FetchAllReport};
