//! Reported and conservatively generated discovery-volume observations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::sodir::error::{Result, SodirError};
use crate::sodir::geojson_wkt::epoch_ms_to_iso_date;

const OUTPUT: &str = "_derived_discovery_volume.csv";
const COMPONENTS: &[(&str, &str, &str)] = &[
    ("recoverable_oil", "dscRecoverableOil", "fldRecoverableOil"),
    ("recoverable_gas", "dscRecoverableGas", "fldRecoverableGas"),
    ("recoverable_ngl", "dscRecoverableNGL", "fldRecoverableNGL"),
    (
        "recoverable_condensate",
        "dscRecoverableCondensate",
        "fldRecoverableCondensate",
    ),
    ("recoverable_oe", "dscRecoverableOe", "fldRecoverableOE"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeReport {
    pub field_primary: usize,
    pub discovery_secondary: usize,
    pub unresolved: usize,
}

type Row = BTreeMap<String, String>;

fn cell<'a>(row: &'a Row, column: &str) -> Option<&'a str> {
    row.get(column)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

#[derive(Clone)]
struct Snapshot {
    date: String,
    values: [Option<f64>; 5],
    identity: String,
    json: String,
    conflict: bool,
}

/// Rebuild the common discovery-volume projection.
pub fn apply(csv_dir: &Path) -> Result<VolumeReport> {
    apply_source_precedence(csv_dir)
}

fn apply_source_precedence(csv_dir: &Path) -> Result<VolumeReport> {
    let discoveries_path = csv_dir.join("discovery.csv");
    if !discoveries_path.is_file() {
        return Ok(VolumeReport::default());
    }
    let discoveries = read_rows(&discoveries_path)?;
    let reserve_rows = read_optional(&csv_dir.join("discovery_reserves.csv"))?;
    let field_rows = read_optional(&csv_dir.join("field_reserves.csv"))?;
    let redirects = redirect_map(&discoveries);
    let discovery_ids: BTreeSet<String> = discoveries
        .iter()
        .filter_map(|row| cell(row, "dscNpdidDiscovery").map(str::to_string))
        .collect();
    let current_fields = lookup(&discoveries, "dscNpdidDiscovery", "fldNpdidField");
    let snapshots = load_snapshots(&field_rows);
    let mut field_members: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, field) in &current_fields {
        field_members
            .entry(field.clone())
            .or_default()
            .push(id.clone());
    }
    for ids in field_members.values_mut() {
        ids.sort();
        ids.dedup();
    }

    let mut output = Vec::new();
    let mut report = VolumeReport::default();
    let mut secondary_roots = BTreeSet::new();
    for id in &discovery_ids {
        let field = current_fields.get(id).map(String::as_str).unwrap_or("");
        let latest = snapshots
            .get(field)
            .and_then(|items| items.last())
            .filter(|snapshot| !snapshot.conflict && snapshot.values.iter().any(Option::is_some));
        if let Some(latest) = latest {
            let covered = &field_members[field];
            let key = format!("field:{field}:{}:original_recoverable", latest.date);
            let mut row = volume_row(
                &format!("field-primary-{}", digest(&format!("{key}|{id}"))),
                id,
                id,
                field,
                true,
                true,
                "field_reserves_primary",
                "field_total",
                "latest_original_recoverable_field_snapshot",
                &latest.date,
                "",
                &latest.values,
                redirects.get(id).map(String::as_str).unwrap_or(""),
                &latest.identity,
                &latest.json,
                "",
                "",
                1,
                false,
                "",
                "",
            );
            row[29] = "shared_field".into();
            row[30] = key;
            row[31] = serde_json::to_string(covered).expect("field members serialize");
            row[32] = covered.len().to_string();
            output.push(row);
        } else {
            secondary_roots.insert(
                resolve_reporting_root(id, &redirects, &discovery_ids)
                    .unwrap_or_else(|| id.clone()),
            );
        }
    }
    for root in secondary_roots {
        let covered: Vec<String> = discovery_ids
            .iter()
            .filter(|id| {
                resolve_reporting_root(id, &redirects, &discovery_ids).as_deref()
                    == Some(root.as_str())
                    && current_fields
                        .get(*id)
                        .and_then(|field| snapshots.get(field))
                        .and_then(|s| s.last())
                        .is_none()
            })
            .cloned()
            .collect();
        let source_rows: Vec<&Row> = reserve_rows
            .iter()
            .filter(|row| cell(row, "dscNpdidDiscovery") == Some(root.as_str()))
            .collect();
        let Some(latest_date) = source_rows
            .iter()
            .filter_map(|row| cell(row, "dscDateOffResEstDisplay").map(normalized_date))
            .max()
        else {
            continue;
        };
        let mut latest: Vec<&Row> = source_rows
            .into_iter()
            .filter(|row| {
                cell(row, "dscDateOffResEstDisplay")
                    .map(normalized_date)
                    .as_deref()
                    == Some(latest_date.as_str())
            })
            .collect();
        latest.sort_by_key(|row| row_json(row));
        latest.dedup_by_key(|row| row_json(row));
        let conflict =
            !conflict_keys(&latest.iter().map(|row| (*row).clone()).collect::<Vec<_>>()).is_empty();
        let mut values = [None; 5];
        for (index, value) in values.iter_mut().enumerate() {
            let present: Vec<f64> = latest
                .iter()
                .filter_map(|row| discovery_values(row)[index])
                .collect();
            if !present.is_empty() {
                *value = Some(present.iter().sum());
            }
        }
        let classes: BTreeSet<&str> = latest
            .iter()
            .filter_map(|row| cell(row, "dscReservesRC"))
            .collect();
        let source_json = serde_json::to_string(&latest.to_vec()).expect("source rows serialize");
        let identity = digest(&source_json);
        let key = format!("discovery:{root}:{latest_date}");
        for id in &covered {
            let mut row = volume_row(
                &format!("discovery-secondary-{}", digest(&format!("{key}|{id}"))),
                id,
                &root,
                "",
                false,
                !conflict,
                "discovery_reserves_secondary",
                "discovery_resources",
                "latest_structured_discovery_reserves",
                &latest_date,
                &serde_json::to_string(&classes).expect("classes serialize"),
                &values,
                redirects.get(id).map(String::as_str).unwrap_or(""),
                &identity,
                &source_json,
                "",
                "",
                latest.len(),
                conflict,
                if conflict {
                    "conflicting_latest_resource_class"
                } else {
                    ""
                },
                "",
            );
            row[29] = if covered.len() > 1 {
                "reporting_group"
            } else {
                "individual"
            }
            .into();
            row[30] = key.clone();
            row[31] = serde_json::to_string(&covered).expect("members serialize");
            row[32] = covered.len().to_string();
            output.push(row);
        }
    }
    output.sort_by(|a, b| a[0].cmp(&b[0]));
    report.field_primary = output
        .iter()
        .filter(|row| row[6] == "field_reserves_primary")
        .count();
    report.discovery_secondary = output
        .iter()
        .filter(|row| row[6] == "discovery_reserves_secondary")
        .count();
    write_rows(&csv_dir.join(OUTPUT), &output)?;
    Ok(report)
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

fn load_snapshots(rows: &[Row]) -> BTreeMap<String, Vec<Snapshot>> {
    let mut grouped: BTreeMap<(String, String), Vec<&Row>> = BTreeMap::new();
    for row in rows {
        let (Some(field), Some(date)) = (
            cell(row, "fldNpdidField"),
            cell(row, "fldDateOffResEstDisplay"),
        ) else {
            continue;
        };
        let date = normalized_date(date);
        if !valid_date(&date) {
            continue;
        }
        grouped
            .entry((field.to_string(), date))
            .or_default()
            .push(row);
    }
    let mut out: BTreeMap<String, Vec<Snapshot>> = BTreeMap::new();
    for ((field_id, date), same_date) in grouped {
        let mut value_shapes: BTreeSet<String> = BTreeSet::new();
        for row in &same_date {
            value_shapes.insert(values_key(&field_values(row)));
        }
        let conflict = value_shapes.len() > 1;
        let row = same_date
            .into_iter()
            .min_by_key(|row| digest(&row_json(row)))
            .unwrap();
        let json = row_json(row);
        out.entry(field_id.clone()).or_default().push(Snapshot {
            date,
            values: field_values(row),
            identity: digest(&json),
            json,
            conflict,
        });
    }
    for snapshots in out.values_mut() {
        snapshots.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then_with(|| a.identity.cmp(&b.identity))
        });
    }
    out
}

fn redirect_map(rows: &[Row]) -> BTreeMap<String, String> {
    rows.iter()
        .filter_map(|row| {
            let source = cell(row, "dscNpdidDiscovery")?;
            let target = cell(row, "dscNpdidResInclInDisc")?;
            Some((source.to_string(), target.to_string()))
        })
        .collect()
}

fn conflict_keys(rows: &[Row]) -> BTreeSet<String> {
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let Some(source) = cell(row, "dscNpdidDiscovery") else {
            continue;
        };
        let date = normalized_date(cell(row, "dscDateOffResEstDisplay").unwrap_or_default());
        let rc = cell(row, "dscReservesRC").unwrap_or_default();
        grouped
            .entry(format!("{source}|{date}|{rc}"))
            .or_default()
            .insert(values_key(&discovery_values(row)));
    }
    grouped
        .into_iter()
        .filter_map(|(key, values)| (values.len() > 1).then_some(key))
        .collect()
}

fn lookup(rows: &[Row], key: &str, value: &str) -> BTreeMap<String, String> {
    rows.iter()
        .filter_map(|row| Some((cell(row, key)?.to_string(), cell(row, value)?.to_string())))
        .collect()
}

fn discovery_values(row: &Row) -> [Option<f64>; 5] {
    std::array::from_fn(|index| parse_number(cell(row, COMPONENTS[index].1)))
}

fn field_values(row: &Row) -> [Option<f64>; 5] {
    std::array::from_fn(|index| parse_number(cell(row, COMPONENTS[index].2)))
}

fn parse_number(value: Option<&str>) -> Option<f64> {
    value?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn normalized_date(value: &str) -> String {
    value
        .parse::<i64>()
        .ok()
        .and_then(epoch_ms_to_iso_date)
        .unwrap_or_else(|| value.to_string())
}

fn valid_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
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

#[allow(clippy::too_many_arguments)]
fn volume_row(
    id: &str,
    discovery_id: &str,
    source_discovery_id: &str,
    field_id: &str,
    generated: bool,
    usable: bool,
    method: &str,
    coverage: &str,
    basis: &str,
    date: &str,
    rc: &str,
    values: &[Option<f64>; 5],
    redirect: &str,
    source_identity: &str,
    source_json: &str,
    before_identity: &str,
    before_json: &str,
    duplicate_count: usize,
    conflict: bool,
    reason: &str,
    signed_deltas: &str,
) -> Vec<String> {
    let mut row = vec![
        id.to_string(),
        discovery_id.to_string(),
        source_discovery_id.to_string(),
        field_id.to_string(),
        generated.to_string(),
        usable.to_string(),
        method.to_string(),
        coverage.to_string(),
        basis.to_string(),
        date.to_string(),
        rc.to_string(),
    ];
    row.extend(
        values
            .iter()
            .map(|value| value.map(|v| v.to_string()).unwrap_or_default()),
    );
    row.extend([
        redirect.to_string(),
        source_identity.to_string(),
        source_json.to_string(),
        before_identity.to_string(),
        before_json.to_string(),
        duplicate_count.to_string(),
        conflict.to_string(),
        reason.to_string(),
        signed_deltas.to_string(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "individual".to_string(),
        format!("discovery:{discovery_id}"),
        serde_json::to_string(&[discovery_id]).expect("discovery ID serializes"),
        "1".to_string(),
    ]);
    row
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

    #[test]
    fn latest_field_reserves_are_shared_primary_with_stable_keys() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "discovery.csv", "dscNpdidDiscovery,dscNpdidResInclInDisc,fldNpdidField\n44786,,4467574\n45651,44786,4467574\n44492,,43645\n44552,44534,46437\n44534,,46437\n28543124,,34833026\n");
        write(tmp.path(), "field_reserves.csv", "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableGas,fldRecoverableNGL,fldRecoverableCondensate,fldRecoverableOE\n4467574,2024-12-31,15.66,44.09,10.765,0,80.204\n4467574,2025-12-31,15.938,44.782,10.547,0,80.759\n43645,2025-12-31,73.329,27.819,2.722,0,106.32\n46437,2025-12-31,299.596,1473.82,21.585,1.52,1815.948\n34833026,2025-12-31,3.782,5.049,0.534,0,9.846\n");
        apply(tmp.path()).unwrap();
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        assert_eq!(rows.len(), 6);
        for row in &rows {
            assert_eq!(row["method"], "field_reserves_primary");
            assert_eq!(row["scope"], "shared_field");
            assert_eq!(row["coverage"], "field_total");
        }
        let gjoa: Vec<_> = rows
            .iter()
            .filter(|r| r["fldNpdidField"] == "4467574")
            .collect();
        assert_eq!(gjoa.len(), 2);
        assert!(gjoa.iter().all(|r| r["recoverable_oe"] == "80.759"));
        assert!(gjoa
            .iter()
            .all(|r| r["aggregation_key"] == "field:4467574:2025-12-31:original_recoverable"));
        assert!(gjoa
            .iter()
            .all(|r| r["covered_discovery_ids"] == "[\"44786\",\"45651\"]"));
        let expected = [
            ("34833026", "9.846"),
            ("43645", "106.32"),
            ("46437", "1815.948"),
        ];
        for (field, oe) in expected {
            assert!(rows
                .iter()
                .any(|r| r["fldNpdidField"] == field && r["recoverable_oe"] == oe));
        }
    }

    #[test]
    fn discovery_reserves_are_secondary_grouped_by_reporting_root_and_latest_date() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,dscNpdidResInclInDisc,fldNpdidField\n1,,\n2,1,\n3,1,\n4,,\n",
        );
        write(tmp.path(),"discovery_reserves.csv","OBJECTID,dscNpdidDiscovery,dscDateOffResEstDisplay,dscReservesRC,dscRecoverableOil,dscRecoverableGas,dscRecoverableOe\n1,1,2024-12-31,4F,9,9,18\n2,1,2025-12-31,4F,0,,2\n3,1,2025-12-31,7F,1,2,3\n4,4,2025-12-31,7F,0,0,0\n");
        apply(tmp.path()).unwrap();
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        let group: Vec<_> = rows
            .iter()
            .filter(|r| r["source_discovery_id"] == "1")
            .collect();
        assert_eq!(group.len(), 3);
        assert!(group
            .iter()
            .all(|r| r["aggregation_key"] == "discovery:1:2025-12-31"
                && r["scope"] == "reporting_group"));
        assert!(group.iter().all(|r| r["recoverable_oil"] == "1"
            && r["recoverable_gas"] == "2"
            && r["recoverable_oe"] == "5"));
        let zero = rows
            .iter()
            .find(|r| r["source_discovery_id"] == "4")
            .unwrap();
        assert_eq!(zero["recoverable_oil"], "0");
        assert_eq!(zero["recoverable_oe"], "0");
        assert_eq!(zero["scope"], "individual");
    }

    #[test]
    fn field_component_blanks_remain_missing_and_do_not_fall_back() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,fldNpdidField\n1,10\n",
        );
        write(tmp.path(),"field_reserves.csv","fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableGas,fldRecoverableOE\n10,2025-12-31,,0,5\n");
        write(
            tmp.path(),
            "discovery_reserves.csv",
            "dscNpdidDiscovery,dscDateOffResEstDisplay,dscRecoverableOil\n1,2025-12-31,99\n",
        );
        apply(tmp.path()).unwrap();
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["method"], "field_reserves_primary");
        assert!(rows[0]["recoverable_oil"].is_empty());
        assert_eq!(rows[0]["recoverable_gas"], "0");

        write(
            tmp.path(),
            "field_reserves.csv",
            "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableOE\n10,2025-12-31,1,5\n10,2025-12-31,2,5\n",
        );
        apply(tmp.path()).unwrap();
        assert!(read_rows(&tmp.path().join(OUTPUT)).unwrap().is_empty());
    }
}
