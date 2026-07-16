//! Offline CSV goldens — the Rust crate's output-boundary oracle.
//!
//! The loaders' Rust surface ends at **emitted CSVs**; graph-building from a
//! blueprint is a Python-layer op (`kglite.from_blueprint`, goldened in the
//! Python suite). So the Rust golden digests the canonical rendering of the
//! CSV set a loader produces, given a synthetic input — catching any
//! extraction regression in the parse/shape/preprocess logic without a
//! network round-trip or the engine.
//!
//! Freezing: the digest is anchored to the copied (== upstream) logic's output
//! on a hand-authored input. Regenerate ONLY for a deliberate behavior change:
//! `UPDATE_GOLDEN=1 cargo test -p kglite-datasets --test csv_golden`, then
//! review + commit the changed `.sha256`. A digest change with no code change
//! is a regression, not a re-baseline.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Workspace root (two up from this crate's manifest dir).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}

/// Deterministic rendering of every `*.csv` under `dir`: files sorted by name,
/// each header row verbatim, data rows **sorted** (order-insensitive), fields
/// tab-joined under `## <file>` section headers. Two dirs render equal iff
/// they hold the same CSVs with the same header + same row multiset.
fn canonical_csv_dir(dir: &Path) -> String {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read csv dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    files.sort();

    let mut out = String::new();
    for f in &files {
        out.push_str(&format!(
            "## {}\n",
            f.file_name().unwrap().to_string_lossy()
        ));
        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .from_path(f)
            .expect("open csv");
        let headers: Vec<String> = rdr
            .headers()
            .expect("headers")
            .iter()
            .map(String::from)
            .collect();
        out.push_str(&format!("H\t{}\n", headers.join("\t")));
        let mut rows: Vec<String> = rdr
            .records()
            .map(|r| r.expect("row").iter().collect::<Vec<_>>().join("\t"))
            .collect();
        rows.sort();
        for row in rows {
            out.push_str(&format!("R\t{row}\n"));
        }
    }
    out
}

fn digest(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}

/// Copy every file in `src` into a fresh tempdir (preprocess mutates in place).
fn stage_fixture(src: &Path) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    for entry in std::fs::read_dir(src).expect("read fixture") {
        let p = entry.unwrap().path();
        if p.is_file() {
            std::fs::copy(&p, tmp.path().join(p.file_name().unwrap())).expect("copy fixture");
        }
    }
    tmp
}

/// Assert the digest of `csv_dir` matches the frozen golden, or (re)write it
/// under `UPDATE_GOLDEN=1`.
fn assert_or_update_golden(golden_name: &str, csv_dir: &Path) {
    let rendered = canonical_csv_dir(csv_dir);
    let got = digest(&rendered);
    let golden_path = workspace_root().join("tests/goldens").join(golden_name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() || !golden_path.exists() {
        std::fs::write(&golden_path, format!("{got}\n")).expect("write golden");
        eprintln!("froze golden {golden_name} = {got}");
        return;
    }
    let want = std::fs::read_to_string(&golden_path)
        .expect("read golden")
        .trim()
        .to_string();
    assert_eq!(
        got, want,
        "\nCSV golden {golden_name} drifted.\n--- rendered ---\n{rendered}\n\
         A digest change means the emitted CSVs changed. If intentional, rerun \
         with UPDATE_GOLDEN=1 and commit the new .sha256 in the same change.",
    );
}

#[cfg(feature = "sodir")]
#[test]
fn sodir_preprocess_csv_golden() {
    use kglite_datasets::sodir::preprocess;

    let fixture = workspace_root().join("tests/fixtures/sodir/csv-in");
    let staged = stage_fixture(&fixture);

    // Run the real FK-derivation joins (pure CSV→CSV, offline). Exercises the
    // sequential-PK + child-propagation path (petreg_licence) and the
    // name→NPDID self-join (strat_chrono).
    let report = preprocess::apply(staged.path()).expect("preprocess applies");

    // Sanity: the joins actually fired (guards against a fixture that silently
    // no-ops, which would make the golden meaningless).
    assert_eq!(
        report.petreg_licence_pk,
        Some(0),
        "all licensee rows should map to a ptl_id (0 unmapped)"
    );
    assert_eq!(
        report.chrono_parent_fk,
        Some(1),
        "the root chrono row has an empty parent → 1 unmapped"
    );

    assert_or_update_golden("sodir-csv.sha256", staged.path());
}
