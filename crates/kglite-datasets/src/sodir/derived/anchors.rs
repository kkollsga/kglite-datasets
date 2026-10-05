//! Augmented copies of the anchor tables (`field.csv`, …) with columns the
//! packaged graph derives from other tables.
//!
//! Every copy upper-cases the main-area columns (see
//! [`super::upper_main_areas`]).
//!
//! `Field` carries its latest `FieldReserves` version (`fldRecoverable…`,
//! `fldRemaining…`, `fldInplace…`, `fldReservesDate`, `fldReservesVersion`)
//! and `fldProducedOE`, the sum of its monthly net oil-equivalent production
//! (set only when positive).

use std::collections::BTreeMap;
use std::path::Path;

use chrono::NaiveDate;

use super::{date, iso, DerivedReport, Table};
use crate::sodir::error::Result;

/// The `FieldReserves` columns copied onto `Field`, under the same names.
const FIELD_RESERVE_COLUMNS: &[&str] = &[
    "fldRecoverableOil",
    "fldRecoverableGas",
    "fldRecoverableNGL",
    "fldRecoverableCondensate",
    "fldRecoverableOE",
    "fldRemainingOil",
    "fldRemainingGas",
    "fldRemainingNGL",
    "fldRemainingCondensate",
    "fldRemainingOE",
    "fldInplaceOil",
    "fldInplaceAssLiquid",
    "fldInplaceAssGas",
    "fldInplaceFreeGas",
];
const FIELD_KEY: &str = "fldNpdidField";

fn read_source(csv_dir: &Path, stem: &str) -> Result<Option<Table>> {
    let path = csv_dir.join(format!("{stem}.csv"));
    if path.is_file() {
        Ok(Some(Table::read(&path)?))
    } else {
        Ok(None)
    }
}

/// A source table with its main areas upper-cased.
fn cased(csv_dir: &Path, stem: &str) -> Result<Table> {
    let mut table = Table::read(&csv_dir.join(format!("{stem}.csv")))?;
    super::upper_main_areas(&table.headers, &mut table.rows);
    Ok(table)
}

pub(super) fn licence(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    cased(csv_dir, "licence")
}

pub(super) fn licence_task(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    cased(csv_dir, "licence_task")
}

pub(super) fn discovery(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    cased(csv_dir, "discovery")
}

pub(super) fn block(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    cased(csv_dir, "block")
}

/// `field.csv` plus the latest-reserves snapshot and `fldProducedOE`.
pub(super) fn field(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    let mut fields = cased(csv_dir, "field")?;
    let key = fields.getter(FIELD_KEY);
    let ids: Vec<String> = fields.rows.iter().map(|r| key.get(r).to_string()).collect();

    let latest = match read_source(csv_dir, "field_reserves")? {
        Some(t) => latest_field_reserves(&t),
        None => BTreeMap::new(),
    };
    report.add(
        "field_reserves_snapshots",
        ids.iter().filter(|id| latest.contains_key(*id)).count(),
    );
    let mut columns: Vec<(&str, Vec<String>)> = FIELD_RESERVE_COLUMNS
        .iter()
        .map(|c| (*c, Vec::with_capacity(ids.len())))
        .collect();
    let mut dates = Vec::with_capacity(ids.len());
    let mut versions = Vec::with_capacity(ids.len());
    for id in &ids {
        let snapshot = latest.get(id);
        for (i, (_, values)) in columns.iter_mut().enumerate() {
            values.push(snapshot.map(|s| s.values[i].clone()).unwrap_or_default());
        }
        dates.push(iso(snapshot.map(|s| s.date)));
        versions.push(snapshot.map(|s| s.version.clone()).unwrap_or_default());
    }
    for (name, values) in columns {
        fields.set_column(name, values);
    }
    fields.set_column("fldReservesDate", dates);
    fields.set_column("fldReservesVersion", versions);

    let produced = match read_source(csv_dir, "profiles")? {
        Some(t) => produced_oe(&t),
        None => BTreeMap::new(),
    };
    let produced: Vec<String> = ids
        .iter()
        .map(|id| match produced.get(id) {
            // Sodir publishes six decimals; drop the summation noise.
            Some(v) if *v > 0.0 => ((v * 1e6).round() / 1e6).to_string(),
            _ => String::new(),
        })
        .collect();
    report.add(
        "field_produced_oe",
        produced.iter().filter(|v| !v.is_empty()).count(),
    );
    fields.set_column("fldProducedOE", produced);
    Ok(fields)
}

struct ReservesSnapshot {
    date: NaiveDate,
    version: String,
    values: Vec<String>,
}

/// Per field, the row with the latest date, ties broken by `fldVersion`.
fn latest_field_reserves(table: &Table) -> BTreeMap<String, ReservesSnapshot> {
    let key = table.getter(FIELD_KEY);
    let day = table.getter("fldDateOffResEstDisplay");
    let version = table.getter("fldVersion");
    let cols: Vec<_> = FIELD_RESERVE_COLUMNS
        .iter()
        .map(|c| table.getter(c))
        .collect();
    let mut best: BTreeMap<String, ((NaiveDate, i64), &Vec<String>)> = BTreeMap::new();
    for row in &table.rows {
        let Some(d) = date(day.get(row)) else {
            continue;
        };
        let rank = (
            d,
            version
                .get(row)
                .parse::<f64>()
                .map_or(i64::MIN, |v| v as i64),
        );
        let entry = best.entry(key.get(row).to_string()).or_insert((rank, row));
        if rank > entry.0 {
            *entry = (rank, row);
        }
    }
    best.into_iter()
        .map(|(id, ((d, _), row))| {
            let snapshot = ReservesSnapshot {
                date: d,
                version: version.get(row).to_string(),
                values: cols.iter().map(|c| c.get(row).to_string()).collect(),
            };
            (id, snapshot)
        })
        .collect()
}

/// Per field, the sum of its monthly `prfPrdOeNetMillSm3`.
fn produced_oe(profiles: &Table) -> BTreeMap<String, f64> {
    let period = profiles.getter("prfPeriod");
    let kind = profiles.getter("prfInformationCarrierKind");
    let carrier = profiles.getter("prfNpdidInformationCarrier");
    let oe = profiles.getter("prfPrdOeNetMillSm3");
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for row in &profiles.rows {
        if period.get(row) != "month" || kind.get(row) != "FIELD" {
            continue;
        }
        if let Ok(v) = oe.get(row).parse::<f64>() {
            *out.entry(carrier.get(row).to_string()).or_default() += v;
        }
    }
    out
}
