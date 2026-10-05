//! Valid-time hygiene for Sodir's closed interval tables.
//!
//! Sodir writes history as `closed` periods: `…To` is the last valid day and
//! the next version starts the day after. kglite refuses an inverted closed
//! interval, so one row whose `to` lies more than a day before its `from`
//! would fail the whole build. Such rows are data errors (a 2026 operator row
//! runs 2025-03-14 → 1974-06-12). This step writes a copy of each table the
//! blueprint reads as `_derived_temporal_<stem>.csv` without them, and logs
//! every removed row to `_derived_temporal_rejects.csv`.
//!
//! Two shapes are kept on purpose:
//!
//! - A row inverted by exactly one day (`to == from − 1`) is Sodir's spelling
//!   of "superseded the day it was registered". The blueprint declares
//!   `empty_when: "to_before_from"`, so kglite stores it as an empty interval,
//!   valid on no day.
//! - A zero-length row (`from == to`) is a complete one-day version. Removing
//!   the zero-length rows that share their day with another row (the rule
//!   first proposed for this step) left the entity with no licensee at all on
//!   that day: on the April and October 2026 snapshots every such day already
//!   sums to 100 %, and the rule would have emptied 503 licence, field, TUF
//!   and business-arrangement days.
//!
//! The copies upper-case the main-area columns, as every derived copy does
//! (`crate::sodir::derived::upper_main_areas`). The source CSVs are never
//! modified. Bounds are compared as kglite reads
//! them: `YYYY-MM-DD` (a time after it is ignored), eight digits as
//! `YYYYMMDD`, or epoch milliseconds of nine digits or more, negative before
//! 1970; anything else is treated as missing, which kglite reads as an open
//! bound.

use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate};

use crate::sodir::error::Result;
use crate::sodir::preprocess::{read_csv, write_csv};

/// Filename prefix of the filtered copies (the stem follows it).
pub const PREFIX: &str = "_derived_temporal_";
/// The audit log of every removed row, rewritten on each run.
pub const REJECTS: &str = "_derived_temporal_rejects.csv";

/// One Sodir table with a closed validity interval.
struct Table {
    stem: &'static str,
    from: &'static str,
    to: &'static str,
    /// Rows sharing this column's value form one entity's history.
    entity: &'static str,
}

const TABLES: &[Table] = &[
    t(
        "field_licensee_hst",
        "fldLicenseeFrom",
        "fldLicenseeTo",
        "fldNpdidField",
    ),
    t(
        "field_operator_hst",
        "fldOperatorFrom",
        "fldOperatorTo",
        "fldNpdidField",
    ),
    t(
        "field_discoveries_incl_hst",
        "fldDiscoveryInclFromDate",
        "fldDiscoveryInclToDate",
        "fldNpdidField",
    ),
    t(
        "field_activity_status_hst",
        "fldStatusFromDate",
        "fldStatusToDate",
        "fldNpdidField",
    ),
    t(
        "field_owner_hst",
        "fldOwnershipFromDate",
        "fldOwnershipToDate",
        "fldNpdidField",
    ),
    t(
        "discovery_operator_hst",
        "dscOperatorFrom",
        "dscOperatorTo",
        "dscNpdidDiscovery",
    ),
    t(
        "discovery_poly_hst",
        "dscDateValidFrom",
        "dscDateValidTo",
        "dscNpdidDiscovery",
    ),
    t(
        "licence_licensee_hst",
        "prlLicenseeDateValidFrom",
        "prlLicenseeDateValidTo",
        "prlNpdidLicence",
    ),
    t(
        "licence_operator_hst",
        "prlOperDateValidFrom",
        "prlOperDateValidTo",
        "prlNpdidLicence",
    ),
    t(
        "licence_phase_hst",
        "prlDatePhaseValidFrom",
        "prlDatePhaseValidTo",
        "prlNpdidLicence",
    ),
    t(
        "licence_area_poly_hst",
        "prlAreaPolyDateValidFrom",
        "prlAreaPolyDateValidTo",
        "prlNpdidLicence",
    ),
    t("tuf", "tufDateValidFrom", "tufDateValidTo", "tufNpdidTuf"),
    t(
        "tuf_operator_hst",
        "tufOperDateValidFrom",
        "tufOperDateValidTo",
        "tufNpdidTuf",
    ),
    t(
        "tuf_owner_hst",
        "tufOwnerDateValidFrom",
        "tufOwnerDateValidTo",
        "tufNpdidTuf",
    ),
    t(
        "business_arrangement_area",
        "baaDateValidFrom",
        "baaDateValidTo",
        "baaNpdidBsnsArrArea",
    ),
    t(
        "business_arrangement_history",
        "baaAreaPolyDateValidFrom",
        "baaAreaPolyDateValidTo",
        "baaNpdidBsnsArrArea",
    ),
    t(
        "business_arrangement_licensee_hst",
        "baaLicenseeDateValidFrom",
        "baaLicenseeDateValidTo",
        "baaNpdidBsnsArrArea",
    ),
    t(
        "business_arrangement_operator",
        "baaOperatorDateValidFrom",
        "baaOperatorDateValidTo",
        "baaNpdidBsnsArrArea",
    ),
    t(
        "afex_area",
        "afxDateValidFrom",
        "afxDateValidTo",
        "afxNpdidAfexArea",
    ),
    t(
        "afex_area_history",
        "afxAreaPolyDateValidFrom",
        "afxAreaPolyDateValidTo",
        "afxNpdidAfexArea",
    ),
    t(
        "petreg_licence",
        "ptlDateValidFrom",
        "ptlDateValidTo",
        "ptlPetregLicenceID",
    ),
    t(
        "seismic_acquisition_fishery",
        "seaFisheryExpertFromDate",
        "seaFisheryExpertToDate",
        "seaNpdidSurvey",
    ),
];

const fn t(
    stem: &'static str,
    from: &'static str,
    to: &'static str,
    entity: &'static str,
) -> Table {
    Table {
        stem,
        from,
        to,
        entity,
    }
}

/// Geometry columns are left out of a reject's `record` — a polygon would
/// make the log unreadable and is recoverable from the source row.
const GEOMETRY_COLUMNS: &[&str] = &["wkt_geometry", "_geometry", "SHAPE"];

const REJECTS_HEADER: &[&str] = &["table", "row", "reason", "entity", "from", "to", "record"];

/// Why a row was left out of the filtered copy.
const INVERTED: &str = "inverted";

/// Counts over every table processed in one run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemporalReport {
    /// Filtered copies written.
    pub tables: usize,
    /// Rows written to the copies.
    pub kept: usize,
    /// Kept rows whose `to` is the day before `from` (empty intervals).
    pub empty_kept: usize,
    /// Rows inverted by more than one day, removed.
    pub inverted_dropped: usize,
}

/// The source stem behind a filtered copy's stem, for the tables this step
/// knows. `_derived_temporal_licence_licensee_hst` gives
/// `licence_licensee_hst`; any other stem gives `None`.
pub fn source_for_derived(stem: &str) -> Option<&'static str> {
    let source = stem.strip_prefix(PREFIX)?;
    TABLES.iter().find(|t| t.stem == source).map(|t| t.stem)
}

/// Write the filtered copy of each `targets` source stem whose CSV exists in
/// `csv_dir`, and rewrite the rejects log. A target whose source is missing
/// gets no copy (a stale one is removed). Nothing is touched when `targets`
/// is empty.
pub fn apply(csv_dir: &Path, targets: &[&str]) -> Result<TemporalReport> {
    let mut report = TemporalReport::default();
    if targets.is_empty() {
        return Ok(report);
    }
    let mut rejects: Vec<Vec<String>> = Vec::new();
    for table in TABLES.iter().filter(|t| targets.contains(&t.stem)) {
        let source = csv_dir.join(format!("{}.csv", table.stem));
        let derived = csv_dir.join(format!("{PREFIX}{}.csv", table.stem));
        if !source.is_file() {
            if derived.is_file() {
                std::fs::remove_file(&derived)?;
            }
            continue;
        }
        let (headers, rows) = read_csv(&source)?;
        let mut kept = filter_table(table, &headers, &rows, &mut report, &mut rejects);
        crate::sodir::derived::upper_main_areas(&headers, &mut kept);
        write_csv(&derived, &headers, &kept)?;
        report.tables += 1;
        report.kept += kept.len();
    }
    let header: Vec<String> = REJECTS_HEADER.iter().map(|s| s.to_string()).collect();
    write_csv(&csv_dir.join(REJECTS), &header, &rejects)?;
    Ok(report)
}

/// The rows of one table that can be declared, in source order. Removed rows
/// are appended to `rejects`.
fn filter_table(
    table: &Table,
    headers: &[String],
    rows: &[Vec<String>],
    report: &mut TemporalReport,
    rejects: &mut Vec<Vec<String>>,
) -> Vec<Vec<String>> {
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (Some(fi), Some(ti)) = (col(table.from), col(table.to)) else {
        // A table without its bound columns declares nothing kglite could
        // refuse; copy it unchanged so the blueprint still finds it.
        return rows.to_vec();
    };
    let ei = col(table.entity);
    let mut kept = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        match (parse_date(&row[fi]), parse_date(&row[ti])) {
            (Some(f), Some(t)) if t < f - Duration::days(1) => {
                report.inverted_dropped += 1;
                rejects.push(vec![
                    table.stem.to_string(),
                    (i + 1).to_string(),
                    INVERTED.to_string(),
                    ei.map(|e| row[e].clone()).unwrap_or_default(),
                    row[fi].clone(),
                    row[ti].clone(),
                    record_json(headers, row),
                ]);
            }
            (Some(f), Some(t)) if t < f => {
                report.empty_kept += 1;
                kept.push(row.clone());
            }
            _ => kept.push(row.clone()),
        }
    }
    kept
}

/// A date as kglite reads a `date` column, or `None`.
pub(crate) fn parse_date(cell: &str) -> Option<NaiveDate> {
    let cell = cell.trim();
    if cell.len() >= 10 && cell.as_bytes()[4] == b'-' {
        return NaiveDate::parse_from_str(&cell[..10], "%Y-%m-%d").ok();
    }
    // A whole number, possibly written as a float ("1451520000000.0").
    let n: i64 = match cell.parse::<i64>() {
        Ok(n) => n,
        Err(_) => {
            let f = cell.parse::<f64>().ok().filter(|f| f.is_finite())?;
            f.trunc() as i64
        }
    };
    let day = if (10_000_000..100_000_000).contains(&n) {
        NaiveDate::parse_from_str(&n.to_string(), "%Y%m%d").ok()?
    } else if n.unsigned_abs() >= 100_000_000 {
        chrono::DateTime::from_timestamp_millis(n)?.date_naive()
    } else {
        return None;
    };
    (1..=9999).contains(&day.year()).then_some(day)
}

fn record_json(headers: &[String], row: &[String]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = headers
        .iter()
        .zip(row)
        .filter(|(h, _)| !GEOMETRY_COLUMNS.contains(&h.as_str()))
        .map(|(h, v)| (h.clone(), serde_json::Value::String(v.clone())))
        .collect();
    serde_json::Value::Object(map).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    fn read(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(name)).unwrap()
    }

    #[test]
    fn parses_iso_and_epoch_ms_like_kglite() {
        let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
        assert_eq!(parse_date("2015-12-31"), d("2015-12-31"));
        assert_eq!(parse_date("2015-12-31T10:00:00"), d("2015-12-31"));
        assert_eq!(parse_date("1451520000000"), d("2015-12-31"));
        assert_eq!(parse_date("1451520000000.0"), d("2015-12-31"));
        assert_eq!(parse_date("267840000000"), d("1978-06-28"));
        // Before 1970 FactMaps writes negative epoch milliseconds.
        assert_eq!(parse_date("-131846400000"), d("1965-10-28"));
        assert_eq!(parse_date("-2208988800000"), d("1900-01-01"));
        assert_eq!(parse_date("20151231"), d("2015-12-31"));
        // kglite stores these as NULL, an open bound.
        for junk in ["", "0", "0.0", "-86400000", "-86400000.0", "n/a"] {
            assert_eq!(parse_date(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn drops_wide_inversions_keeps_one_day_inversions_and_one_day_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        write(
            d,
            "licence_licensee_hst.csv",
            "prlNpdidLicence,cmpLongName,prlLicenseeDateValidFrom,prlLicenseeDateValidTo,prlLicenseeInterest\n\
             1,A,2000-01-01,2009-12-31,60\n\
             1,B,2000-01-01,2009-12-31,40\n\
             1,A,2010-01-01,2010-01-01,55\n\
             1,B,2010-01-01,2010-01-01,45\n\
             1,A,2010-01-02,,50\n\
             1,B,2010-01-02,,50\n\
             2,C,2022-03-31,2022-02-14,100\n\
             2,C,2022-03-31,2022-03-30,100\n",
        );
        let report = apply(d, &["licence_licensee_hst"]).unwrap();
        assert_eq!(
            report,
            TemporalReport {
                tables: 1,
                kept: 7,
                empty_kept: 1,
                inverted_dropped: 1,
            }
        );
        let copy = read(d, "_derived_temporal_licence_licensee_hst.csv");
        assert!(!copy.contains("2022-02-14"), "{copy}");
        // The one-day version (both licensees on 2010-01-01) and the
        // one-day inversion (kglite keeps it as empty) both stay.
        assert!(copy.contains("1,A,2010-01-01,2010-01-01,55"), "{copy}");
        assert!(copy.contains("1,B,2010-01-01,2010-01-01,45"), "{copy}");
        assert!(copy.contains("2022-03-31,2022-03-30"), "{copy}");

        let (h, r) = read_csv(&d.join(REJECTS)).unwrap();
        assert_eq!(h, REJECTS_HEADER);
        assert_eq!(r.len(), 1);
        assert_eq!(
            r[0][..6],
            [
                "licence_licensee_hst",
                "7",
                INVERTED,
                "2",
                "2022-03-31",
                "2022-02-14"
            ]
        );
        assert!(r[0][6].contains("\"prlLicenseeInterest\":\"100\""));
        // The source is untouched.
        assert_eq!(read(d, "licence_licensee_hst.csv").lines().count(), 9);
    }

    #[test]
    fn untargeted_and_missing_tables_write_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        write(
            d,
            "tuf.csv",
            "tufNpdidTuf,tufDateValidFrom,tufDateValidTo\n1,2000-01-01,\n",
        );
        assert_eq!(apply(d, &[]).unwrap(), TemporalReport::default());
        assert!(!d.join(REJECTS).exists());

        write(d, "_derived_temporal_field_owner_hst.csv", "stale\n");
        let report = apply(d, &["tuf", "field_owner_hst"]).unwrap();
        assert_eq!(report.tables, 1);
        assert!(!d.join("_derived_temporal_field_owner_hst.csv").exists());
        assert_eq!(read(d, "_derived_temporal_tuf.csv"), read(d, "tuf.csv"));
    }

    #[test]
    fn apply_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        write(
            d,
            "tuf_owner_hst.csv",
            "tufNpdidTuf,cmpLongName,tufOwnerDateValidFrom,tufOwnerDateValidTo,tufOwnerShare\n\
             9,A,2001-01-01,1990-01-01,50\n9,A,2001-01-01,,40\n9,B,2001-01-01,,60\n",
        );
        apply(d, &["tuf_owner_hst"]).unwrap();
        let first = (
            read(d, "_derived_temporal_tuf_owner_hst.csv"),
            read(d, REJECTS),
        );
        apply(d, &["tuf_owner_hst"]).unwrap();
        assert_eq!(
            first,
            (
                read(d, "_derived_temporal_tuf_owner_hst.csv"),
                read(d, REJECTS)
            )
        );
    }

    #[test]
    fn source_for_derived_knows_only_its_tables() {
        assert_eq!(
            source_for_derived("_derived_temporal_licence_licensee_hst"),
            Some("licence_licensee_hst")
        );
        assert_eq!(source_for_derived("licence_licensee_hst"), None);
        assert_eq!(source_for_derived("_derived_temporal_wellbore"), None);
        assert_eq!(source_for_derived("_derived_discovery_play"), None);
    }
}
