//! Reserves version chains: each published reserves version is valid from its
//! as-of date until the entity's next version (`existsFrom` / `existsTo`,
//! half-open), so an as-of query reads the estimate in force on that day.
//!
//! All rows of one entity that share a version are valid together: the
//! resource classes of a discovery, the companies of a field. A company
//! missing from the field's next version is no longer in force. Two versions
//! under one date (FieldReserves publishes `fldVersion` 2024 and 2025 both as
//! of 2024-12-31 for some fields) are ordered by the version number; the
//! earlier one gets an empty window, valid on no day. A row without a date
//! keeps open bounds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::NaiveDate;

use super::{date, iso, DerivedReport, Table};
use crate::sodir::error::Result;

struct Chain {
    source: &'static str,
    entity: &'static str,
    date: &'static str,
    /// Orders two versions published under the same date.
    order: Option<&'static str>,
}

const FIELD_RESERVES: Chain = Chain {
    source: "field_reserves",
    entity: "fldNpdidField",
    date: "fldDateOffResEstDisplay",
    order: Some("fldVersion"),
};
const DISCOVERY_RESERVES: Chain = Chain {
    source: "discovery_reserves",
    entity: "dscNpdidDiscovery",
    date: "dscDateOffResEstDisplay",
    order: None,
};
const FIELD_RESERVES_COMPANY: Chain = Chain {
    source: "field_reserves_company",
    entity: "fldNpdidField",
    date: "cmpDateOffResEstDisplay",
    order: None,
};

pub(super) fn field_reserves(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    chain(csv_dir, &FIELD_RESERVES, report)
}

pub(super) fn discovery_reserves(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    chain(csv_dir, &DISCOVERY_RESERVES, report)
}

pub(super) fn field_reserves_company(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    chain(csv_dir, &FIELD_RESERVES_COMPANY, report)
}

/// A version: its date, then the order column as a number (text sorts after).
type Version = (NaiveDate, Option<i64>, String);

fn chain(csv_dir: &Path, spec: &Chain, report: &mut DerivedReport) -> Result<Table> {
    let mut table = Table::read(&csv_dir.join(format!("{}.csv", spec.source)))?;
    let (from, to) = windows(&table, spec);
    report.add(
        &format!("{}_superseded_same_day", spec.source),
        from.iter()
            .zip(&to)
            .filter(|(f, t)| f.is_some() && f == t)
            .count(),
    );
    table.set_column("existsFrom", from.into_iter().map(iso).collect());
    table.set_column("existsTo", to.into_iter().map(iso).collect());
    Ok(table)
}

/// Each row's `[from, to)` window.
fn windows(table: &Table, spec: &Chain) -> (Vec<Option<NaiveDate>>, Vec<Option<NaiveDate>>) {
    let entity = table.getter(spec.entity);
    let day = table.getter(spec.date);
    let order = spec.order.map(|c| table.getter(c));
    let version = |row: &[String]| -> Option<Version> {
        let d = date(day.get(row))?;
        let o = order.map(|c| c.get(row)).unwrap_or_default();
        Some((d, o.parse::<f64>().ok().map(|f| f as i64), o.to_string()))
    };
    let mut versions: BTreeMap<&str, BTreeSet<Version>> = BTreeMap::new();
    for row in &table.rows {
        if let Some(v) = version(row) {
            versions.entry(entity.get(row)).or_default().insert(v);
        }
    }
    let mut from = Vec::with_capacity(table.rows.len());
    let mut to = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        let Some(v) = version(row) else {
            from.push(None);
            to.push(None);
            continue;
        };
        let next = versions[entity.get(row)]
            .range((std::ops::Bound::Excluded(&v), std::ops::Bound::Unbounded))
            .next()
            .map(|n| n.0);
        from.push(Some(v.0));
        to.push(next);
    }
    (from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(spec: &Chain, csv: &str) -> Vec<(String, String)> {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(format!("{}.csv", spec.source)), csv).unwrap();
        let out = chain(tmp.path(), spec, &mut DerivedReport::default()).unwrap();
        let (f, t) = (out.getter("existsFrom"), out.getter("existsTo"));
        out.rows
            .iter()
            .map(|r| (f.get(r).to_string(), t.get(r).to_string()))
            .collect()
    }

    #[test]
    fn each_version_runs_until_the_next() {
        let rows = run(
            &FIELD_RESERVES,
            "fldNpdidField,fldDateOffResEstDisplay,fldVersion\n\
             1,1419984000000,2014\n1,1388448000000,2013\n1,1451520000000,2015\n\
             2,1735603200000,2025\n2,1735603200000,2024\n3,,2020\n",
        );
        assert_eq!(
            rows,
            [
                ("2014-12-31".into(), "2015-12-31".into()),
                ("2013-12-31".into(), "2014-12-31".into()),
                ("2015-12-31".into(), String::new()),
                ("2024-12-31".into(), String::new()),
                ("2024-12-31".into(), "2024-12-31".into()),
                (String::new(), String::new()),
            ]
        );
    }

    #[test]
    fn rows_of_one_version_share_its_window() {
        let rows = run(
            &FIELD_RESERVES_COMPANY,
            "fldNpdidField,cmpNpdidCompany,cmpDateOffResEstDisplay\n\
             1,1,2014-12-31\n1,2,2014-12-31\n1,1,2015-12-31\n",
        );
        assert_eq!(rows[0], rows[1]);
        assert_eq!(rows[1], ("2014-12-31".into(), "2015-12-31".into()));
        assert_eq!(rows[2], ("2015-12-31".into(), String::new()));
    }
}
