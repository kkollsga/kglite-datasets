//! Augmented copies of the anchor tables (`field.csv`, …) with columns the
//! packaged graph derives from other tables.
//!
//! Lifecycle windows `existsFrom` / `existsTo` are plain `date` properties,
//! not declared valid time: declaring them would hide a relinquished licence
//! and every relationship touching it from an undated query. `existsTo` is
//! the first day the entity no longer exists (half-open); empty is open.
//!
//! - Licence: granted (`prlDateGranted`) to the day after Sodir's closed
//!   `prlDateValidTo`; a window that would end before it starts is empty.
//! - Field: its earliest registered fact (status, licensee, operator, owner,
//!   included discovery), else 1 January of its discovery year.
//! - Discovery: the completion of its discovery wellbore when that falls in
//!   the discovery year, else 1 January of the year; never later than its
//!   first operator period or field inclusion.
//! - SeismicSurvey: acquisition start, or the planned start when earlier.
//! - Wellbore: see `status::wellbore`.
//!
//! History rows a valid-time copy would drop (inverted by more than a day)
//! are not facts and are ignored; so are dates before 1960, Sodir's
//! "unknown" sentinels.
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

use chrono::{Datelike, Duration, NaiveDate};

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

pub(super) fn licence(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    let mut licences = cased(csv_dir, "licence")?;
    let granted = licences.getter("prlDateGranted");
    let valid_to = licences.getter("prlDateValidTo");
    let mut from = Vec::with_capacity(licences.rows.len());
    let mut to = Vec::with_capacity(licences.rows.len());
    let mut emptied = 0;
    for row in &licences.rows {
        let f = fact_date(granted.get(row));
        let mut t = fact_date(valid_to.get(row)).map(|d| d + Duration::days(1));
        if let (Some(f), Some(end)) = (f, t) {
            if end < f {
                emptied += 1;
                t = Some(f);
            }
        }
        from.push(iso(f));
        to.push(iso(t));
    }
    report.add("licence_windows_emptied", emptied);
    licences.set_column("existsFrom", from);
    licences.set_column("existsTo", to);
    Ok(licences)
}

pub(super) fn licence_task(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    cased(csv_dir, "licence_task")
}

pub(super) fn discovery(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    let mut discoveries = cased(csv_dir, "discovery")?;
    const KEY: &str = "dscNpdidDiscovery";
    let completion: BTreeMap<String, NaiveDate> = match read_source(csv_dir, "wellbore")? {
        Some(wells) => {
            let (id, done) = (
                wells.getter("wlbNpdidWellbore"),
                wells.getter("wlbCompletionDate"),
            );
            wells
                .rows
                .iter()
                .filter_map(|r| Some((id.get(r).to_string(), fact_date(done.get(r))?)))
                .collect()
        }
        None => BTreeMap::new(),
    };
    let operator = earliest(
        csv_dir,
        "discovery_operator_hst",
        KEY,
        "dscOperatorFrom",
        "dscOperatorTo",
    )?;
    let inclusion = earliest(
        csv_dir,
        "field_discoveries_incl_hst",
        KEY,
        "fldDiscoveryInclFromDate",
        "fldDiscoveryInclToDate",
    )?;
    let (key, year, well) = (
        discoveries.getter(KEY),
        discoveries.getter("dscDiscoveryYear"),
        discoveries.getter("wlbNpdidWellbore"),
    );
    let from: Vec<String> = discoveries
        .rows
        .iter()
        .map(|row| {
            let jan1 = year_start(year.get(row));
            let base = match (completion.get(well.get(row)), jan1) {
                (Some(done), Some(j)) if done.year() == j.year() => Some(*done),
                _ => jan1,
            };
            let id = key.get(row);
            iso(
                [base, operator.get(id).copied(), inclusion.get(id).copied()]
                    .into_iter()
                    .flatten()
                    .min(),
            )
        })
        .collect();
    let open = vec![String::new(); from.len()];
    discoveries.set_column("existsFrom", from);
    discoveries.set_column("existsTo", open);
    Ok(discoveries)
}

/// `seismic_acquisition.csv` with its lifecycle window.
pub(super) fn seismic_acquisition(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    let mut surveys = cased(csv_dir, "seismic_acquisition")?;
    let (start, plan) = (
        surveys.getter("seaDateStarting"),
        surveys.getter("seaPlanFromDate"),
    );
    let from: Vec<String> = surveys
        .rows
        .iter()
        .map(|r| {
            iso([fact_date(start.get(r)), fact_date(plan.get(r))]
                .into_iter()
                .flatten()
                .min())
        })
        .collect();
    let open = vec![String::new(); from.len()];
    surveys.set_column("existsFrom", from);
    surveys.set_column("existsTo", open);
    Ok(surveys)
}

/// A date that can be a fact: parsed, and not one of Sodir's pre-1960
/// "unknown" sentinels.
fn fact_date(cell: &str) -> Option<NaiveDate> {
    date(cell).filter(|d| d.year() >= 1960)
}

fn year_start(cell: &str) -> Option<NaiveDate> {
    let year: i32 = cell.trim().parse::<f64>().ok()? as i32;
    NaiveDate::from_ymd_opt(year, 1, 1).filter(|d| d.year() >= 1960)
}

/// Per `key`, the earliest `from` of a history table, ignoring rows whose
/// closed interval is inverted by more than a day.
fn earliest(
    csv_dir: &Path,
    stem: &str,
    key: &str,
    from: &str,
    to: &str,
) -> Result<BTreeMap<String, NaiveDate>> {
    let Some(table) = read_source(csv_dir, stem)? else {
        return Ok(BTreeMap::new());
    };
    let (k, f, t) = (table.getter(key), table.getter(from), table.getter(to));
    let mut out: BTreeMap<String, NaiveDate> = BTreeMap::new();
    for row in &table.rows {
        let Some(start) = fact_date(f.get(row)) else {
            continue;
        };
        if date(t.get(row)).is_some_and(|end| end < start - Duration::days(1)) {
            continue;
        }
        let id = k.get(row);
        if id.is_empty() {
            continue;
        }
        let entry = out.entry(id.to_string()).or_insert(start);
        *entry = (*entry).min(start);
    }
    Ok(out)
}

pub(super) fn block(csv_dir: &Path, _: &mut DerivedReport) -> Result<Table> {
    cased(csv_dir, "block")
}

/// `field.csv` plus the latest-reserves snapshot and `fldProducedOE`.
pub(super) fn field(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    let mut fields = cased(csv_dir, "field")?;
    let first_fact = field_first_facts(csv_dir)?;
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

    let year = fields.getter("fldDiscoveryYear");
    let from: Vec<String> = fields
        .rows
        .iter()
        .zip(&ids)
        .map(|(row, id)| {
            iso(first_fact
                .get(id)
                .copied()
                .or_else(|| year_start(year.get(row))))
        })
        .collect();
    let open = vec![String::new(); from.len()];
    fields.set_column("existsFrom", from);
    fields.set_column("existsTo", open);
    Ok(fields)
}

/// Per field, the earliest start among its registered history.
fn field_first_facts(csv_dir: &Path) -> Result<BTreeMap<String, NaiveDate>> {
    let mut out: BTreeMap<String, NaiveDate> = BTreeMap::new();
    for (stem, from, to) in [
        (
            "field_activity_status_hst",
            "fldStatusFromDate",
            "fldStatusToDate",
        ),
        ("field_licensee_hst", "fldLicenseeFrom", "fldLicenseeTo"),
        ("field_operator_hst", "fldOperatorFrom", "fldOperatorTo"),
        (
            "field_owner_hst",
            "fldOwnershipFromDate",
            "fldOwnershipToDate",
        ),
        (
            "field_discoveries_incl_hst",
            "fldDiscoveryInclFromDate",
            "fldDiscoveryInclToDate",
        ),
    ] {
        for (id, d) in earliest(csv_dir, stem, FIELD_KEY, from, to)? {
            let entry = out.entry(id).or_insert(d);
            *entry = (*entry).min(d);
        }
    }
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(files: &[(&str, &str)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (name, body) in files {
            std::fs::write(tmp.path().join(name), body).unwrap();
        }
        tmp
    }

    #[test]
    fn licence_window_ends_the_day_after_and_never_inverts() {
        let tmp = dir(&[(
            "licence.csv",
            "prlNpdidLicence,prlDateGranted,prlDateValidTo,prlMainArea\n\
             1,2000-01-01,2009-12-31,North sea\n2,2010-01-01,2005-01-01,\n",
        )]);
        let mut report = DerivedReport::default();
        let t = licence(tmp.path(), &mut report).unwrap();
        let (f, to, area) = (
            t.getter("existsFrom"),
            t.getter("existsTo"),
            t.getter("prlMainArea"),
        );
        assert_eq!(
            (f.get(&t.rows[0]), to.get(&t.rows[0])),
            ("2000-01-01", "2010-01-01")
        );
        assert_eq!(
            (f.get(&t.rows[1]), to.get(&t.rows[1])),
            ("2010-01-01", "2010-01-01")
        );
        assert_eq!(area.get(&t.rows[0]), "NORTH SEA");
        assert_eq!(report.counts["licence_windows_emptied"], 1);
    }

    #[test]
    fn earliest_ignores_inverted_rows_and_sentinels() {
        let tmp = dir(&[(
            "h.csv",
            "k,f,t\n1,2001-01-01,\n1,1999-01-01,1990-01-01\n1,-2208988800000,\n2,2003-03-03,2003-03-02\n",
        )]);
        let e = earliest(tmp.path(), "h", "k", "f", "t").unwrap();
        assert_eq!(e["1"], NaiveDate::from_ymd_opt(2001, 1, 1).unwrap());
        // Inverted by exactly one day is a superseded version, still a fact.
        assert_eq!(e["2"], NaiveDate::from_ymd_opt(2003, 3, 3).unwrap());
        assert_eq!(year_start("1969"), NaiveDate::from_ymd_opt(1969, 1, 1));
        assert_eq!(year_start(""), None);
    }
}
