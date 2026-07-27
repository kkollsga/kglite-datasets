//! What a *committed* kglite disk-mode graph looks like on the filesystem.
//!
//! Every loader that caches a disk graph has to answer the same question
//! before deciding "reopen" vs "rebuild": *has kglite finished publishing a
//! graph into this directory?* The answer is a property of kglite's on-disk
//! format, not of any one dataset, so it lives here once — `sec` and `sodir`
//! both call in, and if the engine's layout ever moves there is a single
//! place to follow it.
//!
//! kglite publishes a disk graph in one of two shapes:
//!
//! * **generation layout** — an immutable `generations/gen_…/` snapshot plus a
//!   root `CURRENT` pointer naming it. `save()` renames `CURRENT` into place
//!   last, so its presence is the atomic "this graph is complete" signal.
//! * **flat layout** — the older shape, with `disk_graph_meta.json` written
//!   directly at the graph root.
//!
//! Both are checked, newest first. A graph root that has neither — only the
//! bare `seg_*/` mmap files an *uncommitted* builder leaves behind — is not a
//! cache hit, and probing it as one would hand the caller a directory
//! `kglite.load()` refuses.
//!
//! Note what is deliberately *not* here: no `graph_manifest.json`. The SEC
//! layout probed for that filename until 2026-07 and kglite has never written
//! it, so the SEC disk cache never hit and every `mode="disk"` open silently
//! rebuilt from scratch.

use std::path::{Path, PathBuf};

/// Root-level pointer naming the live generation (generation layout).
pub const GENERATION_POINTER: &str = "CURRENT";

/// Root-level metadata written directly into the graph dir (flat layout).
pub const FLAT_META: &str = "disk_graph_meta.json";

/// The file whose existence — and mtime — marks `graph_dir` as a committed
/// kglite disk graph. Returns the generation pointer when present, else the
/// flat metadata path. Purely path arithmetic apart from one `is_file` probe;
/// the returned path is not guaranteed to exist (see [`exists`]).
pub fn commit_marker(graph_dir: &Path) -> PathBuf {
    let pointer = graph_dir.join(GENERATION_POINTER);
    if pointer.is_file() {
        pointer
    } else {
        graph_dir.join(FLAT_META)
    }
}

/// True when `graph_dir` holds a committed kglite disk graph.
///
/// This is a *probe*, not a validation: it says a publish completed, not that
/// the bytes are readable. Callers must still degrade gracefully when the
/// subsequent load fails — a corrupt or engine-incompatible cache should
/// rebuild, never propagate.
pub fn exists(graph_dir: &Path) -> bool {
    commit_marker(graph_dir).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn empty_dir_is_not_a_graph() {
        let d = tmp();
        assert!(!exists(d.path()));
        assert_eq!(commit_marker(d.path()), d.path().join(FLAT_META));
    }

    #[test]
    fn flat_layout_detected() {
        let d = tmp();
        fs::write(d.path().join(FLAT_META), b"{}").unwrap();
        assert!(exists(d.path()));
        assert_eq!(commit_marker(d.path()), d.path().join(FLAT_META));
    }

    #[test]
    fn generation_pointer_wins_over_flat_meta() {
        let d = tmp();
        fs::write(d.path().join(FLAT_META), b"{}").unwrap();
        fs::write(
            d.path().join(GENERATION_POINTER),
            b"gen_00000000000000000001\n",
        )
        .unwrap();
        assert!(exists(d.path()));
        assert_eq!(commit_marker(d.path()), d.path().join(GENERATION_POINTER));
    }

    /// The regression this module exists for: an *uncommitted* build leaves
    /// `seg_*/` mmap files behind but no marker, and a legacy
    /// `graph_manifest.json` is not a marker either — kglite never writes one.
    #[test]
    fn uncommitted_build_and_bogus_manifest_are_not_a_graph() {
        let d = tmp();
        fs::create_dir_all(d.path().join("seg_000")).unwrap();
        fs::write(d.path().join("seg_000/node_slots.bin"), b"\0\0").unwrap();
        fs::write(d.path().join(".kglite.lock"), b"0").unwrap();
        assert!(!exists(d.path()));

        fs::write(d.path().join("graph_manifest.json"), b"{}").unwrap();
        assert!(
            !exists(d.path()),
            "graph_manifest.json is not a kglite artifact and must never count as a cache hit"
        );
    }

    /// A directory (not a file) named `CURRENT` must not be mistaken for the
    /// pointer, and must not shadow a valid flat-layout marker.
    #[test]
    fn directory_named_current_is_ignored() {
        let d = tmp();
        fs::create_dir_all(d.path().join(GENERATION_POINTER)).unwrap();
        assert!(!exists(d.path()));
        fs::write(d.path().join(FLAT_META), b"{}").unwrap();
        assert!(exists(d.path()));
        assert_eq!(commit_marker(d.path()), d.path().join(FLAT_META));
    }
}
