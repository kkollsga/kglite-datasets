//! One non-duplicating volume projection for every Sodir discovery.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::sodir::error::{Result, SodirError};
use crate::sodir::geojson_wkt::epoch_ms_to_iso_date;

const OUTPUT: &str = "_derived_discovery_volume.csv";
const COMPONENTS: &[(&str, &str)] = &[
    ("dscRecoverableOil", "fldRecoverableOil"),
    ("dscRecoverableGas", "fldRecoverableGas"),
    ("dscRecoverableNGL", "fldRecoverableNGL"),
    ("dscRecoverableCondensate", "fldRecoverableCondensate"),
    ("dscRecoverableOe", "fldRecoverableOE"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeReport {
    pub discovery: usize,
    pub field_fallback: usize,
    pub null: usize,
}

type Row = BTreeMap<String, String>;

#[derive(Clone)]
struct Discovery {
    id: String,
    root: String,
    field: Option<String>,
    chronology: Chronology,
    redirect: Option<String>,
}

#[derive(Clone, Eq, PartialEq)]
struct Chronology {
    date: Option<String>,
    basis: &'static str,
}

#[derive(Clone)]
struct Snapshot {
    date: String,
    values: [Option<f64>; 5],
    identity: String,
    json: String,
    resource_class: String,
    duplicate_count: usize,
}

#[derive(Clone, Copy)]
enum VolumeSource {
    Discovery,
    FieldFallback,
    Null,
}

struct DiscoverySet {
    rows: Vec<Discovery>,
    ids: BTreeSet<String>,
    redirects: BTreeMap<String, String>,
}

/// Rebuild one `DiscoveryVolume` row per discovery.
///
/// A latest structured discovery snapshot wins. Otherwise the latest field
/// snapshot is assigned only to the field's earliest discovery. Redirected
/// discoveries never copy the reporting root's values, and every remaining
/// discovery receives an explicit row with null components.
pub fn apply(csv_dir: &Path) -> Result<VolumeReport> {
    let discovery_path = csv_dir.join("discovery.csv");
    if !discovery_path.is_file() {
        return Ok(VolumeReport::default());
    }

    let discovery_rows = read_rows(&discovery_path)?;
    let reserve_rows = read_optional(&csv_dir.join("discovery_reserves.csv"))?;
    let field_rows = read_optional(&csv_dir.join("field_reserves.csv"))?;
    let discoveries = load_discoveries(csv_dir, &discovery_rows)?;
    let discovery_snapshots =
        discovery_snapshots(&reserve_rows, &discoveries.redirects, &discoveries.ids);
    let field_snapshots = field_snapshots(&field_rows);
    let earliest = earliest_by_field(&discoveries.rows);
    let roots_with_discovery_volume: BTreeSet<&str> = discovery_snapshots
        .keys()
        .filter(|root| discoveries.ids.contains(*root))
        .map(String::as_str)
        .collect();
    let (output, report) = project_volumes(
        &discoveries.rows,
        &discovery_snapshots,
        &field_snapshots,
        &earliest,
        &roots_with_discovery_volume,
    );

    write_rows(&csv_dir.join(OUTPUT), &output)?;
    Ok(report)
}

fn load_discoveries(csv_dir: &Path, discovery_rows: &[Row]) -> Result<DiscoverySet> {
    let membership_rows = read_optional(&csv_dir.join("field_discoveries_incl_hst.csv"))?;
    let wellbore_rows = read_optional(&csv_dir.join("wellbore.csv"))?;
    let ids: BTreeSet<String> = discovery_rows
        .iter()
        .filter_map(|row| cell(row, "dscNpdidDiscovery").map(str::to_string))
        .collect();
    let redirects = redirect_map(discovery_rows);
    let well_completion = lookup(&wellbore_rows, "wlbNpdidWellbore", "wlbCompletionDate");
    let inclusion_dates = inclusion_dates(&membership_rows);
    let mut discoveries: Vec<Discovery> = discovery_rows
        .iter()
        .filter_map(|row| {
            let id = cell(row, "dscNpdidDiscovery")?.to_string();
            Some(Discovery {
                root: resolve_reporting_root(&id, &redirects, &ids).unwrap_or_else(|| id.clone()),
                field: cell(row, "fldNpdidField").map(str::to_string),
                chronology: chronology(row, &well_completion, &inclusion_dates),
                redirect: redirects.get(&id).cloned(),
                id,
            })
        })
        .collect();
    discoveries.sort_by(|left, right| id_cmp(&left.id, &right.id));
    discoveries.dedup_by(|left, right| left.id == right.id);
    Ok(DiscoverySet {
        rows: discoveries,
        ids,
        redirects,
    })
}

fn project_volumes(
    discoveries: &[Discovery],
    discovery_snapshots: &BTreeMap<String, Snapshot>,
    field_snapshots: &BTreeMap<String, Snapshot>,
    earliest: &BTreeMap<String, String>,
    roots_with_discovery_volume: &BTreeSet<&str>,
) -> (Vec<Vec<String>>, VolumeReport) {
    let mut output = Vec::with_capacity(discoveries.len());
    let mut report = VolumeReport::default();
    for discovery in discoveries {
        let (row, source) = project_discovery(
            discovery,
            discovery_snapshots,
            field_snapshots,
            earliest,
            roots_with_discovery_volume,
        );
        match source {
            VolumeSource::Discovery => report.discovery += 1,
            VolumeSource::FieldFallback => report.field_fallback += 1,
            VolumeSource::Null => report.null += 1,
        }
        output.push(row);
    }
    (output, report)
}

fn project_discovery(
    discovery: &Discovery,
    discovery_snapshots: &BTreeMap<String, Snapshot>,
    field_snapshots: &BTreeMap<String, Snapshot>,
    earliest: &BTreeMap<String, String>,
    roots_with_discovery_volume: &BTreeSet<&str>,
) -> (Vec<String>, VolumeSource) {
    if discovery.id == discovery.root {
        if let Some(snapshot) = discovery_snapshots.get(&discovery.root) {
            return (
                snapshot_row(
                    discovery,
                    snapshot,
                    false,
                    "discovery_reserves",
                    "discovery",
                    "latest_structured_discovery_reserves",
                    discovery.chronology.date.as_deref().unwrap_or(""),
                ),
                VolumeSource::Discovery,
            );
        }
    }

    let is_earliest = discovery.field.as_ref().is_some_and(|field| {
        earliest.get(field).map(String::as_str) == Some(discovery.id.as_str())
    });
    let root_has_volume = roots_with_discovery_volume.contains(discovery.root.as_str());
    if is_earliest && !root_has_volume {
        if let Some(snapshot) = discovery
            .field
            .as_ref()
            .and_then(|field| field_snapshots.get(field))
        {
            return (
                snapshot_row(
                    discovery,
                    snapshot,
                    true,
                    "field_reserves_fallback",
                    "field",
                    &format!("earliest_field_discovery_by_{}", discovery.chronology.basis),
                    discovery.chronology.date.as_deref().unwrap_or(""),
                ),
                VolumeSource::FieldFallback,
            );
        }
    }

    let reason = if discovery.id != discovery.root && root_has_volume {
        "included_in_reporting_discovery"
    } else if discovery.field.is_some() && !is_earliest {
        "later_field_discovery"
    } else if is_earliest && root_has_volume {
        "field_total_unused_discovery_volume_exists"
    } else {
        "no_structured_volume"
    };
    (null_row(discovery, reason), VolumeSource::Null)
}

fn chronology(
    discovery: &Row,
    well_completion: &BTreeMap<String, String>,
    inclusion_dates: &BTreeMap<(String, String), String>,
) -> Chronology {
    if let Some(date) = cell(discovery, "dscDateFromInclInField").and_then(normalized_valid_date) {
        return Chronology {
            date: Some(date),
            basis: "discovery_field_inclusion",
        };
    }
    if let (Some(field), Some(id)) = (
        cell(discovery, "fldNpdidField"),
        cell(discovery, "dscNpdidDiscovery"),
    ) {
        if let Some(date) = inclusion_dates.get(&(field.to_string(), id.to_string())) {
            return Chronology {
                date: Some(date.clone()),
                basis: "field_inclusion_history",
            };
        }
    }
    if let Some(date) = cell(discovery, "wlbNpdidWellbore")
        .and_then(|well| well_completion.get(well))
        .and_then(|date| normalized_valid_date(date))
    {
        return Chronology {
            date: Some(date),
            basis: "designated_well_completion",
        };
    }
    if let Some(year) = cell(discovery, "dscDiscoveryYear")
        .filter(|year| year.len() == 4 && year.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Chronology {
            date: Some(format!("{year}-01-01")),
            basis: "discovery_year",
        };
    }
    Chronology {
        date: None,
        basis: "discovery_id",
    }
}

fn inclusion_dates(rows: &[Row]) -> BTreeMap<(String, String), String> {
    let mut result = BTreeMap::new();
    for row in rows {
        let (Some(field), Some(discovery), Some(date)) = (
            cell(row, "fldNpdidField"),
            cell(row, "dscNpdidDiscovery"),
            cell(row, "fldDiscoveryInclFromDate").and_then(normalized_valid_date),
        ) else {
            continue;
        };
        result
            .entry((field.to_string(), discovery.to_string()))
            .and_modify(|current: &mut String| {
                if date < *current {
                    *current = date.clone();
                }
            })
            .or_insert(date);
    }
    result
}

fn earliest_by_field(discoveries: &[Discovery]) -> BTreeMap<String, String> {
    let mut result: BTreeMap<String, &Discovery> = BTreeMap::new();
    for discovery in discoveries {
        let Some(field) = &discovery.field else {
            continue;
        };
        result
            .entry(field.clone())
            .and_modify(|current| {
                if chronology_cmp(discovery, current).is_lt() {
                    *current = discovery;
                }
            })
            .or_insert(discovery);
    }
    result
        .into_iter()
        .map(|(field, discovery)| (field, discovery.id.clone()))
        .collect()
}

fn chronology_cmp(left: &Discovery, right: &Discovery) -> Ordering {
    match (&left.chronology.date, &right.chronology.date) {
        (Some(left_date), Some(right_date)) => left_date
            .cmp(right_date)
            .then_with(|| id_cmp(&left.id, &right.id)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => id_cmp(&left.id, &right.id),
    }
}

fn id_cmp(left: &str, right: &str) -> Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn discovery_snapshots(
    rows: &[Row],
    redirects: &BTreeMap<String, String>,
    ids: &BTreeSet<String>,
) -> BTreeMap<String, Snapshot> {
    let mut grouped: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        let Some(source) = cell(row, "dscNpdidDiscovery") else {
            continue;
        };
        let Some(root) = resolve_reporting_root(source, redirects, ids) else {
            continue;
        };
        grouped.entry(root).or_default().push(row);
    }
    grouped
        .into_iter()
        .filter_map(|(root, rows)| {
            let direct: Vec<&Row> = rows
                .iter()
                .copied()
                .filter(|row| cell(row, "dscNpdidDiscovery") == Some(root.as_str()))
                .collect();
            let selected = if direct.is_empty() { &rows } else { &direct };
            latest_snapshot(selected, "dscDateOffResEstDisplay", false)
                .map(|snapshot| (root, snapshot))
        })
        .collect()
}

fn field_snapshots(rows: &[Row]) -> BTreeMap<String, Snapshot> {
    grouped_snapshots(rows, "fldNpdidField", "fldDateOffResEstDisplay", true)
}

fn grouped_snapshots(
    rows: &[Row],
    key_column: &str,
    date_column: &str,
    field: bool,
) -> BTreeMap<String, Snapshot> {
    let mut grouped: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        if let Some(key) = cell(row, key_column) {
            grouped.entry(key.to_string()).or_default().push(row);
        }
    }
    grouped
        .into_iter()
        .filter_map(|(key, rows)| {
            latest_snapshot(&rows, date_column, field).map(|snapshot| (key, snapshot))
        })
        .collect()
}

fn latest_snapshot(rows: &[&Row], date_column: &str, field: bool) -> Option<Snapshot> {
    let latest_date = rows
        .iter()
        .filter_map(|row| cell(row, date_column).and_then(normalized_valid_date))
        .max()?;
    let mut latest: Vec<&Row> = rows
        .iter()
        .copied()
        .filter(|row| {
            cell(row, date_column)
                .and_then(normalized_valid_date)
                .as_deref()
                == Some(latest_date.as_str())
        })
        .collect();
    latest.sort_by_key(|row| row_json(row));
    let source_json = serde_json::to_string(&latest).expect("source rows serialize");
    let mut semantic: BTreeMap<String, &Row> = BTreeMap::new();
    for row in &latest {
        let resource_class = if field {
            ""
        } else {
            cell(row, "dscReservesRC").unwrap_or_default()
        };
        semantic
            .entry(format!(
                "{resource_class}|{}",
                values_key(&values(row, field))
            ))
            .or_insert(row);
    }
    let selected: Vec<&Row> = semantic.into_values().collect();
    if has_conflict(&selected, field) {
        return None;
    }

    let mut components = [None; 5];
    for (index, value) in components.iter_mut().enumerate() {
        let present: Vec<f64> = selected
            .iter()
            .filter_map(|row| values(row, field)[index])
            .collect();
        if !present.is_empty() {
            *value = Some(present.iter().sum());
        }
    }
    if components.iter().all(Option::is_none) {
        return None;
    }
    let resource_classes: BTreeSet<&str> = selected
        .iter()
        .filter_map(|row| cell(row, "dscReservesRC"))
        .collect();
    Some(Snapshot {
        date: latest_date,
        values: components,
        identity: digest(&source_json),
        json: source_json,
        resource_class: serde_json::to_string(&resource_classes)
            .expect("resource classes serialize"),
        duplicate_count: latest.len() - selected.len(),
    })
}

fn has_conflict(rows: &[&Row], field: bool) -> bool {
    let mut by_class: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        by_class
            .entry(cell(row, "dscReservesRC").unwrap_or_default())
            .or_default()
            .insert(values_key(&values(row, field)));
    }
    by_class.values().any(|shapes| shapes.len() > 1)
}

fn values(row: &Row, field: bool) -> [Option<f64>; 5] {
    std::array::from_fn(|index| {
        let column = if field {
            COMPONENTS[index].1
        } else {
            COMPONENTS[index].0
        };
        parse_number(cell(row, column))
    })
}

fn snapshot_row(
    discovery: &Discovery,
    snapshot: &Snapshot,
    generated: bool,
    method: &str,
    coverage: &str,
    basis: &str,
    selection_date: &str,
) -> Vec<String> {
    let aggregation_key = if method == "discovery_reserves" {
        format!("discovery:{}:{}", discovery.root, snapshot.date)
    } else {
        format!(
            "field:{}:{}",
            discovery.field.as_deref().unwrap_or_default(),
            snapshot.date
        )
    };
    volume_row(
        &format!(
            "{method}-{}",
            digest(&format!("{aggregation_key}|{}", discovery.id))
        ),
        discovery,
        generated,
        true,
        method,
        coverage,
        basis,
        &snapshot.date,
        &snapshot.resource_class,
        &snapshot.values,
        &snapshot.identity,
        &snapshot.json,
        snapshot.duplicate_count,
        "",
        selection_date,
        &aggregation_key,
    )
}

fn null_row(discovery: &Discovery, reason: &str) -> Vec<String> {
    volume_row(
        &format!("no-volume-{}", digest(&discovery.id)),
        discovery,
        false,
        false,
        "no_volume",
        "none",
        "no_volume_assigned",
        "",
        "",
        &[None; 5],
        "",
        "",
        0,
        reason,
        discovery.chronology.date.as_deref().unwrap_or(""),
        &format!("discovery:{}", discovery.id),
    )
}

#[allow(clippy::too_many_arguments)]
fn volume_row(
    id: &str,
    discovery: &Discovery,
    generated: bool,
    usable: bool,
    method: &str,
    coverage: &str,
    basis: &str,
    estimate_date: &str,
    resource_class: &str,
    values: &[Option<f64>; 5],
    source_identity: &str,
    source_json: &str,
    duplicate_count: usize,
    reason: &str,
    selection_date: &str,
    aggregation_key: &str,
) -> Vec<String> {
    let mut row = vec![
        id.to_string(),
        discovery.id.clone(),
        discovery.root.clone(),
        discovery.field.clone().unwrap_or_default(),
        generated.to_string(),
        usable.to_string(),
        method.to_string(),
        coverage.to_string(),
        basis.to_string(),
        estimate_date.to_string(),
        resource_class.to_string(),
    ];
    row.extend(
        values
            .iter()
            .map(|value| value.map(|v| v.to_string()).unwrap_or_default()),
    );
    row.extend([
        discovery.redirect.clone().unwrap_or_default(),
        source_identity.to_string(),
        source_json.to_string(),
        String::new(),
        String::new(),
        duplicate_count.to_string(),
        "false".to_string(),
        reason.to_string(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "individual".to_string(),
        aggregation_key.to_string(),
        serde_json::to_string(&[&discovery.id]).expect("discovery ID serializes"),
        "1".to_string(),
        selection_date.to_string(),
        discovery.chronology.basis.to_string(),
    ]);
    row
}

fn redirect_map(rows: &[Row]) -> BTreeMap<String, String> {
    rows.iter()
        .filter_map(|row| {
            Some((
                cell(row, "dscNpdidDiscovery")?.to_string(),
                cell(row, "dscNpdidResInclInDisc")?.to_string(),
            ))
        })
        .collect()
}

fn resolve_reporting_root(
    id: &str,
    redirects: &BTreeMap<String, String>,
    ids: &BTreeSet<String>,
) -> Option<String> {
    let mut current = id;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current) {
            return None;
        }
        match redirects.get(current).map(String::as_str) {
            Some(next) if next != current && ids.contains(next) => current = next,
            Some(next) if next != current => return None,
            _ => return Some(current.to_string()),
        }
    }
}

fn lookup(rows: &[Row], key: &str, value: &str) -> BTreeMap<String, String> {
    rows.iter()
        .filter_map(|row| Some((cell(row, key)?.to_string(), cell(row, value)?.to_string())))
        .collect()
}

fn cell<'a>(row: &'a Row, column: &str) -> Option<&'a str> {
    row.get(column)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn parse_number(value: Option<&str>) -> Option<f64> {
    value?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn normalized_valid_date(value: &str) -> Option<String> {
    let normalized = value
        .parse::<i64>()
        .ok()
        .and_then(epoch_ms_to_iso_date)
        .unwrap_or_else(|| value.to_string());
    chrono::NaiveDate::parse_from_str(&normalized, "%Y-%m-%d")
        .is_ok()
        .then_some(normalized)
}

fn values_key(values: &[Option<f64>; 5]) -> String {
    values
        .iter()
        .map(|value| value.map(|v| format!("{v:.12}")).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("|")
}

fn row_json(row: &Row) -> String {
    serde_json::to_string(row).expect("CSV row serializes")
}

fn digest(value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(value.as_bytes());
    format!("{:x}", hash.finalize())
}

fn read_optional(path: &Path) -> Result<Vec<Row>> {
    if path.is_file() {
        read_rows(path)
    } else {
        Ok(Vec::new())
    }
}

fn read_rows(path: &Path) -> Result<Vec<Row>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|error| SodirError::Csv(format!("open {}: {error}", path.display())))?;
    let headers = reader
        .headers()
        .map_err(|error| SodirError::Csv(format!("headers {}: {error}", path.display())))?
        .clone();
    reader
        .records()
        .map(|record| {
            record
                .map(|record| {
                    headers
                        .iter()
                        .zip(record.iter())
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect()
                })
                .map_err(|error| SodirError::Csv(format!("row {}: {error}", path.display())))
        })
        .collect()
}

fn write_rows(path: &Path, rows: &[Vec<String>]) -> Result<()> {
    let tmp = path.with_extension("csv.tmp");
    let mut writer = csv::Writer::from_path(&tmp)
        .map_err(|error| SodirError::Csv(format!("open {}: {error}", tmp.display())))?;
    writer
        .write_record([
            "volume_id",
            "dscNpdidDiscovery",
            "source_discovery_id",
            "fldNpdidField",
            "generated",
            "usable",
            "method",
            "coverage",
            "basis",
            "estimate_date",
            "resource_class",
            "recoverable_oil",
            "recoverable_gas",
            "recoverable_ngl",
            "recoverable_condensate",
            "recoverable_oe",
            "raw_redirect_id",
            "source_record_identity",
            "source_record_json",
            "source_before_identity",
            "source_before_json",
            "source_duplicate_count",
            "conflict",
            "unresolved_reason",
            "signed_deltas_json",
            "generation_parameters_json",
            "generation_source_url",
            "generation_source_publication_year",
            "generation_source_accessed_on",
            "scope",
            "aggregation_key",
            "covered_discovery_ids",
            "covered_discovery_count",
            "chronology_date",
            "chronology_basis",
        ])
        .map_err(|error| SodirError::Csv(format!("header {}: {error}", tmp.display())))?;
    for row in rows {
        writer
            .write_record(row)
            .map_err(|error| SodirError::Csv(format!("row {}: {error}", tmp.display())))?;
    }
    writer.flush()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    fn rows(dir: &Path) -> Vec<Row> {
        read_rows(&dir.join(OUTPUT)).unwrap()
    }

    #[test]
    fn discovery_volume_is_primary_and_finite_zero_is_present() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,fldNpdidField,wlbNpdidWellbore\n1,10,101\n",
        );
        write(
            tmp.path(),
            "wellbore.csv",
            "wlbNpdidWellbore,wlbCompletionDate\n101,2000-01-01\n",
        );
        write(tmp.path(), "discovery_reserves.csv", "dscNpdidDiscovery,dscDateOffResEstDisplay,dscReservesRC,dscRecoverableOil,dscRecoverableGas,dscRecoverableOe\n1,2025-12-31,4F,0,2,2\n");
        write(
            tmp.path(),
            "field_reserves.csv",
            "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil\n10,2025-12-31,99\n",
        );

        let report = apply(tmp.path()).unwrap();
        let rows = rows(tmp.path());
        assert_eq!(
            report,
            VolumeReport {
                discovery: 1,
                field_fallback: 0,
                null: 0
            }
        );
        assert_eq!(rows[0]["method"], "discovery_reserves");
        assert_eq!(rows[0]["recoverable_oil"], "0");
        assert_eq!(rows[0]["recoverable_gas"], "2");
    }

    #[test]
    fn field_fallback_belongs_only_to_earliest_and_later_is_null() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,fldNpdidField,wlbNpdidWellbore\n20,10,102\n10,10,101\n",
        );
        write(
            tmp.path(),
            "wellbore.csv",
            "wlbNpdidWellbore,wlbCompletionDate\n102,2005-01-01\n101,2000-01-01\n",
        );
        write(tmp.path(), "field_reserves.csv", "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableGas\n10,1767139200000,7,0\n");

        let report = apply(tmp.path()).unwrap();
        let rows = rows(tmp.path());
        assert_eq!(
            report,
            VolumeReport {
                discovery: 0,
                field_fallback: 1,
                null: 1
            }
        );
        assert_eq!(
            rows.iter()
                .map(|row| row["dscNpdidDiscovery"].as_str())
                .collect::<Vec<_>>(),
            ["10", "20"]
        );
        assert_eq!(rows[0]["method"], "field_reserves_fallback");
        assert_eq!(rows[0]["estimate_date"], "2025-12-31");
        assert_eq!(rows[0]["chronology_date"], "2000-01-01");
        assert_eq!(rows[0]["chronology_basis"], "designated_well_completion");
        assert_eq!(rows[1]["method"], "no_volume");
        assert_eq!(rows[1]["unresolved_reason"], "later_field_discovery");
        assert!(rows[1]["recoverable_oil"].is_empty());
        assert!(rows[1]["recoverable_gas"].is_empty());
    }

    #[test]
    fn direct_volume_on_earliest_does_not_shift_field_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,fldNpdidField,wlbNpdidWellbore\n1,10,101\n2,10,102\n",
        );
        write(
            tmp.path(),
            "wellbore.csv",
            "wlbNpdidWellbore,wlbCompletionDate\n101,2000-01-01\n102,2001-01-01\n",
        );
        write(
            tmp.path(),
            "discovery_reserves.csv",
            "dscNpdidDiscovery,dscDateOffResEstDisplay,dscRecoverableOil\n1,2025-12-31,3\n",
        );
        write(
            tmp.path(),
            "field_reserves.csv",
            "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil\n10,2025-12-31,9\n",
        );

        let report = apply(tmp.path()).unwrap();
        let rows = rows(tmp.path());
        assert_eq!(
            report,
            VolumeReport {
                discovery: 1,
                field_fallback: 0,
                null: 1
            }
        );
        assert_eq!(rows[0]["method"], "discovery_reserves");
        assert_eq!(rows[1]["method"], "no_volume");
        assert_eq!(rows[1]["unresolved_reason"], "later_field_discovery");
    }

    #[test]
    fn duplicate_field_snapshots_with_different_object_ids_are_not_summed() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,fldNpdidField\n1,10\n",
        );
        write(
            tmp.path(),
            "field_reserves.csv",
            "OBJECTID,fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableOE\n1,10,2025-12-31,4,5\n2,10,2025-12-31,4,5\n",
        );

        apply(tmp.path()).unwrap();
        let rows = rows(tmp.path());
        assert_eq!(rows[0]["recoverable_oil"], "4");
        assert_eq!(rows[0]["recoverable_oe"], "5");
        assert_eq!(rows[0]["source_duplicate_count"], "1");
        assert!(rows[0]["source_record_json"].contains("\"OBJECTID\":\"1\""));
        assert!(rows[0]["source_record_json"].contains("\"OBJECTID\":\"2\""));
    }

    #[test]
    fn redirect_values_are_attached_once_to_terminal_root() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,dscNpdidResInclInDisc,fldNpdidField\n2,1,10\n1,,10\n4,3,20\n3,,20\n",
        );
        write(
            tmp.path(),
            "discovery_reserves.csv",
            "dscNpdidDiscovery,dscDateOffResEstDisplay,dscRecoverableOe\n2,2025-12-31,0\n1,2025-12-31,8\n4,2025-12-31,6\n",
        );

        apply(tmp.path()).unwrap();
        let rows = rows(tmp.path());
        assert_eq!(rows[0]["method"], "discovery_reserves");
        assert_eq!(rows[0]["recoverable_oe"], "8");
        assert_eq!(rows[1]["method"], "no_volume");
        assert!(rows[1]["recoverable_oe"].is_empty());
        assert_eq!(
            rows[1]["unresolved_reason"],
            "included_in_reporting_discovery"
        );
        assert_eq!(rows[2]["method"], "discovery_reserves");
        assert_eq!(rows[2]["recoverable_oe"], "6");
        assert_eq!(rows[3]["method"], "no_volume");
        assert!(rows[3]["recoverable_oe"].is_empty());
    }

    #[test]
    fn chronology_fallback_and_ties_are_deterministic() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "discovery.csv", "dscNpdidDiscovery,dscDateFromInclInField,dscDiscoveryYear,fldNpdidField\n20,2000-01-01,1990,10\n10,2000-01-01,1995,10\n30,,1980,20\n40,,1970,20\n");
        write(tmp.path(), "field_reserves.csv", "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil\n10,2025-12-31,1\n20,2025-12-31,2\n");

        apply(tmp.path()).unwrap();
        let rows = rows(tmp.path());
        assert_eq!(
            rows.iter()
                .map(|row| row["dscNpdidDiscovery"].as_str())
                .collect::<Vec<_>>(),
            ["10", "20", "30", "40"]
        );
        assert_eq!(rows[0]["method"], "field_reserves_fallback");
        assert!(rows[1]["recoverable_oil"].is_empty());
        assert!(rows[2]["recoverable_oil"].is_empty());
        assert_eq!(rows[3]["method"], "field_reserves_fallback");
        assert_eq!(
            rows[3]["basis"],
            "earliest_field_discovery_by_discovery_year"
        );
    }

    #[test]
    fn earlier_inclusion_wins_when_its_well_was_completed_later() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,dscDateFromInclInField,fldNpdidField,wlbNpdidWellbore\n1,2001-01-01,10,101\n2,2002-01-01,10,102\n",
        );
        write(
            tmp.path(),
            "wellbore.csv",
            "wlbNpdidWellbore,wlbCompletionDate\n101,2010-01-01\n102,2000-01-01\n",
        );
        write(
            tmp.path(),
            "field_reserves.csv",
            "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil\n10,2025-12-31,9\n",
        );

        apply(tmp.path()).unwrap();
        let rows = rows(tmp.path());
        assert_eq!(rows[0]["method"], "field_reserves_fallback");
        assert_eq!(rows[0]["chronology_date"], "2001-01-01");
        assert_eq!(rows[0]["chronology_basis"], "discovery_field_inclusion");
        assert_eq!(rows[1]["method"], "no_volume");
        assert!(rows[1]["recoverable_oil"].is_empty());
    }
}
