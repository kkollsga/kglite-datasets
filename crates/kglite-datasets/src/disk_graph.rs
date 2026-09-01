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
//! * **generation layout** — an immutable `generations/gen_<20 digits>/`
//!   snapshot plus a root `CURRENT` pointer naming it. `CURRENT` is written
//!   **last**, by atomic rename, and only after the staged generation has been
//!   verified to hold both `metadata.json` and `disk_graph_meta.json` and been
//!   fsync'd. It is therefore a completion marker *by construction* — exactly
//!   the property a cache probe needs.
//! * **flat layout** — the older shape, with `disk_graph_meta.json` written
//!   directly at the graph root. Legacy graphs have no `CURRENT` at all, so
//!   the fallback branch stays.
//!
//! We still dereference the pointer rather than trusting its mere presence.
//! The atomic-rename guarantee covers what *kglite* does; it says nothing
//! about a directory a user has since half-deleted, restored from a partial
//! backup, or copied without following into `generations/`. Checking that the
//! named generation is really there costs one `stat` and turns a whole class
//! of "probes valid, refuses to load" into an ordinary cache miss.
//!
//! ## Three things that look like markers and are not
//!
//! Each would yield a plausible-but-wrong probe, so they are named here and
//! pinned by the tests below:
//!
//! 1. **`graph_manifest.json`** — never written by kglite; zero references in
//!    the engine source. The SEC layout probed for it until 2026-07, which is
//!    why the SEC disk cache never hit and every `mode="disk"` open silently
//!    rebuilt from scratch.
//! 2. **`seg_manifest.json`** — real, but it lives *inside* a generation
//!    directory. Probing for it would accept a half-written generation.
//! 3. **`.kglite.lock` and `seg_000/`** — both present in an *unsaved*
//!    `storage="disk"` workspace, alongside a `.working-<pid>-<n>/` staging
//!    dir. Neither is a validity signal; a probe keying on either would
//!    confidently report a graph that cannot be loaded. Since kglite 0.16.19
//!    the lock is also *released* at every publish (held for the dirty
//!    window only), so the file's presence says nothing in either direction:
//!    it sits in saved and unsaved directories alike.

use std::path::{Path, PathBuf};

/// Root-level pointer naming the live generation (generation layout).
pub const GENERATION_POINTER: &str = "CURRENT";

/// Root-level directory holding the immutable generation snapshots.
pub const GENERATIONS_DIR: &str = "generations";

/// Per-graph metadata: at the root in the flat layout, inside the generation
/// directory in the generation layout. Its presence is what makes a
/// generation complete.
pub const FLAT_META: &str = "disk_graph_meta.json";

/// The file whose mtime dates the committed graph in `graph_dir`, or `None`
/// when the directory holds no committed graph.
///
/// For the generation layout this is the `CURRENT` pointer itself — written
/// last, so its mtime is the publish time. Callers that age a cache (Sodir's
/// cooldown short-circuit) want exactly that.
pub fn commit_marker(graph_dir: &Path) -> Option<PathBuf> {
    let pointer = graph_dir.join(GENERATION_POINTER);
    if pointer.is_file() {
        let generation = std::fs::read_to_string(&pointer).ok()?;
        let generation = generation.trim();
        // Reject anything that is not a bare directory name. kglite refuses
        // path components in `CURRENT` when reading; a probe that did not
        // would happily follow `../..` out of the graph root.
        if generation.is_empty() || Path::new(generation).components().count() != 1 {
            return None;
        }
        let meta = graph_dir
            .join(GENERATIONS_DIR)
            .join(generation)
            .join(FLAT_META);
        return meta.is_file().then_some(pointer);
    }
    // Legacy flat layout — no CURRENT, metadata at the root. Deliberately not
    // reached when CURRENT exists: a stale root `disk_graph_meta.json` left
    // over from an older build must not rescue a broken generation pointer.
    let flat = graph_dir.join(FLAT_META);
    flat.is_file().then_some(flat)
}

/// True when `graph_dir` holds a committed kglite disk graph.
///
/// This is a *probe*, not a validation: it says a publish completed and the
/// generation it named is present, not that every byte inside is readable.
/// Callers must still degrade gracefully when the subsequent load fails — a
/// corrupt or engine-incompatible cache should rebuild, never propagate.
pub fn exists(graph_dir: &Path) -> bool {
    commit_marker(graph_dir).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const GEN: &str = "gen_00000000000000000001";

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// Write a directory shaped like a committed generation-layout graph.
    fn publish_generation(root: &Path, generation: &str) {
        let dir = root.join(GENERATIONS_DIR).join(generation);
        fs::create_dir_all(dir.join("seg_000")).unwrap();
        fs::write(dir.join(FLAT_META), b"{}").unwrap();
        fs::write(dir.join("metadata.json"), b"{}").unwrap();
        fs::write(dir.join("seg_manifest.json"), b"{}").unwrap();
        fs::write(root.join(GENERATION_POINTER), format!("{generation}\n")).unwrap();
    }

    #[test]
    fn empty_dir_is_not_a_graph() {
        let d = tmp();
        assert!(!exists(d.path()));
        assert_eq!(commit_marker(d.path()), None);
    }

    #[test]
    fn flat_layout_detected() {
        let d = tmp();
        fs::write(d.path().join(FLAT_META), b"{}").unwrap();
        assert_eq!(commit_marker(d.path()), Some(d.path().join(FLAT_META)));
    }

    #[test]
    fn generation_layout_detected_and_dates_from_the_pointer() {
        let d = tmp();
        publish_generation(d.path(), GEN);
        assert_eq!(
            commit_marker(d.path()),
            Some(d.path().join(GENERATION_POINTER)),
            "the pointer is written last, so its mtime is the publish time"
        );
    }

    /// The reason we dereference instead of trusting the pointer's presence.
    /// kglite's own writes are atomic, but a directory that has since been
    /// half-deleted or partially restored is not kglite's doing.
    #[test]
    fn pointer_to_a_missing_generation_is_not_a_graph() {
        let d = tmp();
        fs::write(d.path().join(GENERATION_POINTER), format!("{GEN}\n")).unwrap();
        assert!(!exists(d.path()));

        // A generation directory without its metadata is incomplete.
        fs::create_dir_all(d.path().join(GENERATIONS_DIR).join(GEN)).unwrap();
        assert!(!exists(d.path()));

        fs::write(
            d.path().join(GENERATIONS_DIR).join(GEN).join(FLAT_META),
            b"{}",
        )
        .unwrap();
        assert!(exists(d.path()));
    }

    /// A stale root `disk_graph_meta.json` must not rescue a broken pointer:
    /// once `CURRENT` exists the graph is generation-layout, full stop.
    #[test]
    fn flat_meta_does_not_rescue_a_broken_pointer() {
        let d = tmp();
        fs::write(d.path().join(FLAT_META), b"{}").unwrap();
        fs::write(d.path().join(GENERATION_POINTER), format!("{GEN}\n")).unwrap();
        assert!(!exists(d.path()));
    }

    /// `CURRENT` must never be able to point outside the graph root.
    #[test]
    fn pointer_with_path_components_is_rejected() {
        for bogus in ["../escape", "a/b", "/abs", ""] {
            let d = tmp();
            fs::write(d.path().join(GENERATION_POINTER), format!("{bogus}\n")).unwrap();
            assert!(!exists(d.path()), "CURRENT = {bogus:?} must be rejected");
        }
    }

    /// Trap 1: `graph_manifest.json` is not a kglite artifact at all.
    /// Trap 3: `.kglite.lock`, `seg_000/` and `.working-<pid>-<n>/` are what
    /// an *unsaved* disk workspace contains — none is a validity signal.
    #[test]
    fn unsaved_workspace_and_bogus_manifest_are_not_a_graph() {
        let d = tmp();
        fs::create_dir_all(d.path().join("seg_000")).unwrap();
        fs::create_dir_all(d.path().join(".working-1234-1").join("seg_000")).unwrap();
        for f in [
            "_pending_edges.bin",
            "in_offsets.bin",
            "node_slots.bin",
            "out_offsets.bin",
        ] {
            fs::write(d.path().join("seg_000").join(f), b"\0\0").unwrap();
        }
        fs::write(d.path().join(".kglite.lock"), b"0").unwrap();
        assert!(!exists(d.path()), "an unsaved workspace is not a cache hit");

        fs::write(d.path().join("graph_manifest.json"), b"{}").unwrap();
        assert!(
            !exists(d.path()),
            "graph_manifest.json is not a kglite artifact and must never count as a cache hit"
        );
    }

    /// Trap 2: `seg_manifest.json` is real but lives inside a generation, so
    /// it must not be mistaken for a root completion marker.
    #[test]
    fn seg_manifest_at_the_root_is_not_a_marker() {
        let d = tmp();
        fs::write(d.path().join("seg_manifest.json"), b"{}").unwrap();
        assert!(!exists(d.path()));
    }

    /// A *directory* named `CURRENT` is not the pointer, and must not shadow
    /// a valid flat-layout marker.
    #[test]
    fn directory_named_current_is_ignored() {
        let d = tmp();
        fs::create_dir_all(d.path().join(GENERATION_POINTER)).unwrap();
        assert!(!exists(d.path()));
        fs::write(d.path().join(FLAT_META), b"{}").unwrap();
        assert_eq!(commit_marker(d.path()), Some(d.path().join(FLAT_META)));
    }
}
