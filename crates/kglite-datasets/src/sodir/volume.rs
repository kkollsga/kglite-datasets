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
    pub reported: usize,
    pub singleton_copies: usize,
    pub inclusion_deltas: usize,
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
    field_id: String,
    date: String,
    values: [Option<f64>; 5],
    identity: String,
    json: String,
    conflict: bool,
}

#[derive(Clone)]
struct Membership {
    field_id: String,
    discovery_id: String,
    from: Option<String>,
    to: Option<String>,
}

/// Rebuild the common discovery-volume projection.
pub fn apply(csv_dir: &Path) -> Result<VolumeReport> {
    let discoveries_path = csv_dir.join("discovery.csv");
    let discovery_reserves_path = csv_dir.join("discovery_reserves.csv");
    let field_reserves_path = csv_dir.join("field_reserves.csv");
    let memberships_path = csv_dir.join("field_discoveries_incl_hst.csv");
    if !discoveries_path.is_file() {
        return Ok(VolumeReport::default());
    }

    let discoveries = read_rows(&discoveries_path)?;
    let reserve_rows = read_optional(&discovery_reserves_path)?;
    let field_rows = read_optional(&field_reserves_path)?;
    let membership_rows = read_optional(&memberships_path)?;
    let redirects = redirect_map(&discoveries);
    let discovery_ids: BTreeSet<String> = discoveries
        .iter()
        .filter_map(|row| cell(row, "dscNpdidDiscovery").map(str::to_string))
        .collect();
    let current_fields = lookup(&discoveries, "dscNpdidDiscovery", "fldNpdidField");
    let direct_reported: BTreeSet<String> = reserve_rows
        .iter()
        .filter_map(|row| cell(row, "dscNpdidDiscovery").map(str::to_string))
        .collect();

    let mut output = Vec::new();
    let mut report = VolumeReport::default();
    emit_reported(
        &reserve_rows,
        &redirects,
        &discovery_ids,
        &mut output,
        &mut report,
    );

    let memberships = load_memberships(&membership_rows);
    let snapshots = load_snapshots(&field_rows);
    emit_singletons(
        &memberships,
        &snapshots,
        &current_fields,
        &redirects,
        &direct_reported,
        &mut output,
        &mut report,
    );
    emit_inclusion_deltas(
        &memberships,
        &snapshots,
        &current_fields,
        &redirects,
        &direct_reported,
        &mut output,
        &mut report,
    );
    let curated = emit_troll_allocation(
        &memberships,
        &snapshots,
        &current_fields,
        &redirects,
        &direct_reported,
        &mut output,
    );
    if curated {
        output.retain(|row| {
            row[5] == "true"
                || !matches!(row[1].as_str(), "44552" | "44534")
                || row[6] == "troll_published_component_allocation"
        });
    }
    emit_published_estimates(
        &discoveries,
        &discovery_ids,
        &current_fields,
        &redirects,
        &mut output,
    );

    output.sort_by(|a, b| a[0].cmp(&b[0]));
    output.dedup_by(|left, right| left[0] == right[0]);
    report.reported = output
        .iter()
        .filter(|row| row[6] == "reported_observation")
        .count();
    report.singleton_copies = output
        .iter()
        .filter(|row| row[5] == "true" && row[6] == "singleton_field_copy")
        .count();
    report.inclusion_deltas = output
        .iter()
        .filter(|row| row[5] == "true" && row[6] == "single_entrant_positive_delta")
        .count();
    report.unresolved = output.iter().filter(|row| row[5] == "false").count();
    write_rows(&csv_dir.join(OUTPUT), &output)?;
    Ok(report)
}

fn emit_published_estimates(
    discoveries: &[Row],
    discovery_ids: &BTreeSet<String>,
    current_fields: &BTreeMap<String, String>,
    redirects: &BTreeMap<String, String>,
    output: &mut Vec<Vec<String>>,
) {
    use crate::sodir::estimates::{self, Unit};
    for discovery in discoveries {
        let (Some(id), Some(name)) = (
            cell(discovery, "dscNpdidDiscovery"),
            cell(discovery, "dscName"),
        ) else {
            continue;
        };
        let Ok(id_number) = id.parse::<u64>() else {
            continue;
        };
        if output.iter().any(|row| row[1] == id && row[5] == "true") {
            continue;
        }
        let Some(estimate) = estimates::newest_applicable_estimate(name, id_number) else {
            continue;
        };
        let redirect = redirects.get(id).map(String::as_str).unwrap_or("");
        if !redirect.is_empty()
            && (!discovery_ids.contains(redirect)
                || current_fields.get(id) != current_fields.get(redirect))
        {
            continue;
        }
        let values = [
            estimate_value(estimate.recoverable_oil, Unit::MillionSm3Oil),
            estimate_value(estimate.recoverable_gas, Unit::BillionSm3Gas),
            estimate_value(estimate.recoverable_ngl, Unit::MillionTonnesNgl),
            estimate_value(estimate.recoverable_condensate, Unit::MillionSm3Condensate),
            estimate_value(estimate.recoverable_oe, Unit::MillionSm3OilEquivalent),
        ];
        if values.iter().all(Option::is_none) {
            continue;
        }
        let source_json = serde_json::to_string(estimate).expect("published estimate serializes");
        let identity = digest(&source_json);
        let mut row = volume_row(
            &format!("published-estimate-{id}-{identity}"),
            id,
            id,
            current_fields.get(id).map(String::as_str).unwrap_or(""),
            true,
            true,
            "published_resource_range_midpoint",
            "published_whole_discovery_estimate",
            "newest_applicable_published_appraisal",
            estimate.estimate_date,
            "",
            &values,
            redirect,
            &identity,
            &source_json,
            "",
            "",
            1,
            false,
            "",
            "",
        );
        let len = row.len();
        row[len - 4] = source_json;
        row[len - 3] = estimate.source_url.into();
        row[len - 2] = estimate.estimate_date.get(..4).unwrap_or("").into();
        row[len - 1] = estimate.accessed_on.into();
        output.push(row);
    }
}

fn estimate_value(
    value: Option<crate::sodir::estimates::EstimateValue>,
    expected_unit: crate::sodir::estimates::Unit,
) -> Option<f64> {
    let value = value.filter(|value| value.unit == expected_unit)?;
    value
        .point
        .or_else(|| match (value.minimum, value.maximum) {
            (Some(minimum), Some(maximum))
                if minimum.is_finite() && maximum.is_finite() && minimum <= maximum =>
            {
                let source_places = decimal_places(minimum).max(decimal_places(maximum));
                let scale = 10_f64.powi((source_places + 1) as i32);
                Some((((minimum + maximum) / 2.0) * scale).round() / scale)
            }
            _ => None,
        })
}

fn decimal_places(value: f64) -> usize {
    value
        .to_string()
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len())
}

const TROLL_FIELD: &str = "46437";
const TROLL_EAST: &str = "44552";
const TROLL_WEST: &str = "44534";
const TROLL_SOURCE_URL: &str = "https://www.sodir.no/en/whats-new/publications/reports/resource-report/resource-report-2024/remaining-resources/";
const TROLL_ACCESSED_ON: &str = "2026-09-08";

fn emit_troll_allocation(
    memberships: &[Membership],
    snapshots: &BTreeMap<String, Vec<Snapshot>>,
    current_fields: &BTreeMap<String, String>,
    redirects: &BTreeMap<String, String>,
    direct_reported: &BTreeSet<String>,
    output: &mut Vec<Vec<String>>,
) -> bool {
    let history: Vec<_> = memberships
        .iter()
        .filter(|m| m.field_id == TROLL_FIELD)
        .collect();
    if !history
        .iter()
        .any(|m| matches!(m.discovery_id.as_str(), TROLL_EAST | TROLL_WEST))
    {
        return false;
    }
    let ids: BTreeSet<_> = history.iter().map(|m| m.discovery_id.as_str()).collect();
    let latest = snapshots.get(TROLL_FIELD).and_then(|items| items.last());
    let reason = if ids != BTreeSet::from([TROLL_EAST, TROLL_WEST]) {
        Some("unexpected_constituent_history")
    } else if ![TROLL_EAST, TROLL_WEST].iter().all(|id| {
        current_fields
            .get(*id)
            .is_some_and(|field| field == TROLL_FIELD)
    }) {
        Some("current_field_disagrees")
    } else if redirects
        .get(TROLL_EAST)
        .is_none_or(|target| target != TROLL_WEST)
        || redirects
            .get(TROLL_WEST)
            .is_some_and(|target| target != TROLL_WEST)
    {
        Some("unexpected_resource_redirect")
    } else if direct_reported.contains(TROLL_EAST) || direct_reported.contains(TROLL_WEST) {
        Some("reported_overlap")
    } else if history.iter().any(|membership| membership.from.is_none()) {
        Some("incomplete_membership_interval")
    } else if latest.is_none() {
        Some("missing_field_snapshot")
    } else if latest.unwrap().conflict {
        Some("conflicting_latest_snapshot")
    } else if latest.unwrap().date.as_str() < "2023-12-31" {
        Some("snapshot_predates_source_context")
    } else if ![TROLL_EAST, TROLL_WEST].iter().all(|id| {
        history
            .iter()
            .any(|m| m.discovery_id == *id && active_at(m, &latest.unwrap().date))
    }) {
        Some("membership_not_active_at_snapshot")
    } else if !latest
        .unwrap()
        .values
        .iter()
        .all(|value| value.is_some_and(|number| number >= 0.0))
    {
        Some("invalid_recoverable_values")
    } else {
        None
    };
    if let Some(reason) = reason {
        for discovery_id in [TROLL_EAST, TROLL_WEST] {
            output.push(unresolved_row(
                discovery_id,
                TROLL_FIELD,
                "troll_published_component_allocation",
                reason,
            ));
        }
        return true;
    }
    let latest = latest.unwrap();
    let source = latest.values.map(Option::unwrap);
    let east = [
        Some(0.0),
        Some(source[1] * 2.0 / 3.0),
        Some(source[2] * 2.0 / 3.0),
        Some(source[3] * 2.0 / 3.0),
        Some((source[4] - source[0]) * 2.0 / 3.0),
    ];
    let west = std::array::from_fn(|index| Some(source[index] - east[index].unwrap()));
    let parameters = serde_json::json!({
        "east_non_oil_ratio": 2.0 / 3.0, "expected_discoveries": [44552, 44534],
        "assumptions": ["about two-thirds of recoverable gas assigned to Troll East", "all recoverable oil assigned to Troll West", "NGL and condensate follow the gas ratio", "non-oil OE follows the gas ratio", "published ratio carried forward to the latest qualifying field snapshot"]
    }).to_string();
    for (discovery_id, values, redirect) in [(TROLL_EAST, east, TROLL_WEST), (TROLL_WEST, west, "")]
    {
        let mut row = volume_row(
            &format!(
                "troll-allocation-{}",
                digest(&format!("{discovery_id}|{}", latest.identity))
            ),
            discovery_id,
            discovery_id,
            TROLL_FIELD,
            true,
            true,
            "troll_published_component_allocation",
            "component_allocation_estimate",
            "latest_original_recoverable",
            &latest.date,
            "",
            &values,
            redirect,
            &latest.identity,
            &latest.json,
            "",
            "",
            1,
            false,
            "",
            "",
        );
        let len = row.len();
        row[len - 4] = parameters.clone();
        row[len - 3] = TROLL_SOURCE_URL.into();
        row[len - 2] = "2024".into();
        row[len - 1] = TROLL_ACCESSED_ON.into();
        output.push(row);
    }
    true
}

fn emit_reported(
    rows: &[Row],
    redirects: &BTreeMap<String, String>,
    discovery_ids: &BTreeSet<String>,
    output: &mut Vec<Vec<String>>,
    report: &mut VolumeReport,
) {
    let mut groups: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        let Some(source_id) = cell(row, "dscNpdidDiscovery") else {
            continue;
        };
        let date = normalized_date(cell(row, "dscDateOffResEstDisplay").unwrap_or_default());
        let rc = cell(row, "dscReservesRC").unwrap_or_default();
        let values = discovery_values(row);
        let key = format!("{source_id}|{date}|{rc}|{}", values_key(&values));
        groups.entry(key).or_default().push(row);
    }
    let conflict_keys = conflict_keys(rows);
    for (key, duplicates) in groups {
        let row = duplicates
            .iter()
            .copied()
            .min_by_key(|row| digest(&row_json(row)))
            .expect("reported group is non-empty");
        let source_id = cell(row, "dscNpdidDiscovery").unwrap_or_default();
        let date = normalized_date(cell(row, "dscDateOffResEstDisplay").unwrap_or_default());
        let rc = cell(row, "dscReservesRC").unwrap_or_default();
        let values = discovery_values(row);
        let source_json = row_json(row);
        let identity = digest(&source_json);
        let conflict_key = format!("{source_id}|{date}|{rc}");
        let reporting_parent = redirects.get(source_id).filter(|target| {
            target.as_str() != source_id && discovery_ids.contains(target.as_str())
        });
        let reported_with_parent =
            reporting_parent.is_some() && values.iter().all(|value| *value == Some(0.0));
        output.push(volume_row(
            &format!("reported-{}", digest(&key)),
            source_id,
            source_id,
            "",
            false,
            !reported_with_parent,
            "reported_observation",
            "reported",
            "discovery_reserves",
            &date,
            rc,
            &values,
            redirects.get(source_id).map(String::as_str).unwrap_or(""),
            &identity,
            &source_json,
            "",
            "",
            duplicates.len(),
            conflict_keys.contains(&conflict_key),
            if reported_with_parent {
                "resources_reported_with_parent"
            } else {
                ""
            },
            "",
        ));
        report.reported += 1;
    }
}

fn emit_singletons(
    memberships: &[Membership],
    snapshots: &BTreeMap<String, Vec<Snapshot>>,
    current_fields: &BTreeMap<String, String>,
    redirects: &BTreeMap<String, String>,
    direct_reported: &BTreeSet<String>,
    output: &mut Vec<Vec<String>>,
    report: &mut VolumeReport,
) {
    let by_field = memberships_by_field(memberships);
    for (field_id, history) in by_field {
        let ids: BTreeSet<&str> = history.iter().map(|m| m.discovery_id.as_str()).collect();
        if ids.len() > 1 {
            for discovery_id in ids {
                output.push(unresolved_row(
                    discovery_id,
                    field_id,
                    "singleton_field_copy",
                    "multiple_historical_constituents",
                ));
                report.unresolved += 1;
            }
            continue;
        }
        let Some(discovery_id) = ids.iter().next().copied() else {
            continue;
        };
        let reason = singleton_reason(
            field_id,
            discovery_id,
            &history,
            snapshots.get(field_id),
            current_fields,
            redirects,
            direct_reported,
        );
        if let Some(reason) = reason {
            output.push(unresolved_row(
                discovery_id,
                field_id,
                "singleton_field_copy",
                reason,
            ));
            report.unresolved += 1;
            continue;
        }
        let latest = snapshots[field_id].last().unwrap();
        output.push(volume_row(
            &format!(
                "singleton-{}",
                digest(&format!(
                    "{discovery_id}|{}|{}",
                    latest.field_id, latest.identity
                ))
            ),
            discovery_id,
            discovery_id,
            field_id,
            true,
            true,
            "singleton_field_copy",
            "complete_field_history",
            "latest_original_recoverable_field_snapshot",
            &latest.date,
            "",
            &latest.values,
            "",
            &latest.identity,
            &latest.json,
            "",
            "",
            1,
            latest.conflict,
            "",
            "",
        ));
        report.singleton_copies += 1;
    }
}

fn singleton_reason<'a>(
    field_id: &str,
    discovery_id: &str,
    history: &[&Membership],
    snapshots: Option<&Vec<Snapshot>>,
    current_fields: &BTreeMap<String, String>,
    redirects: &BTreeMap<String, String>,
    direct_reported: &BTreeSet<String>,
) -> Option<&'a str> {
    if current_fields.get(discovery_id).map(String::as_str) != Some(field_id) {
        return Some("current_field_disagrees");
    }
    if redirects
        .get(discovery_id)
        .is_some_and(|target| target != discovery_id)
    {
        return Some("resource_redirect");
    }
    if direct_reported.contains(discovery_id) {
        return Some("reported_overlap");
    }
    if history.iter().any(|membership| membership.from.is_none()) {
        return Some("incomplete_membership_interval");
    }
    let Some(snapshots) = snapshots.filter(|items| !items.is_empty()) else {
        return Some("missing_field_snapshot");
    };
    let latest = snapshots.last().unwrap();
    if latest.conflict {
        return Some("conflicting_latest_snapshot");
    }
    if !history
        .iter()
        .any(|membership| active_at(membership, &latest.date))
    {
        return Some("membership_not_active_at_snapshot");
    }
    if latest.values.iter().all(Option::is_none) {
        return Some("missing_recoverable_values");
    }
    None
}

fn emit_inclusion_deltas(
    memberships: &[Membership],
    snapshots: &BTreeMap<String, Vec<Snapshot>>,
    current_fields: &BTreeMap<String, String>,
    redirects: &BTreeMap<String, String>,
    direct_reported: &BTreeSet<String>,
    output: &mut Vec<Vec<String>>,
    report: &mut VolumeReport,
) {
    for (field_id, field_snapshots) in snapshots {
        let history: Vec<&Membership> = memberships
            .iter()
            .filter(|m| &m.field_id == field_id)
            .collect();
        for pair in field_snapshots.windows(2) {
            let before = &pair[0];
            let after = &pair[1];
            let entrants: Vec<&&Membership> = history
                .iter()
                .filter(|membership| {
                    membership
                        .from
                        .as_ref()
                        .is_some_and(|date| date > &before.date && date <= &after.date)
                })
                .collect();
            let leavers: Vec<&&Membership> = history
                .iter()
                .filter(|membership| {
                    membership
                        .to
                        .as_ref()
                        .is_some_and(|date| date > &before.date && date <= &after.date)
                })
                .collect();
            if entrants.len() > 1 {
                for entrant in &entrants {
                    output.push(unresolved_row_with_evidence(
                        &entrant.discovery_id,
                        field_id,
                        "single_entrant_positive_delta",
                        "multiple_entrants",
                        after,
                        before,
                        "",
                    ));
                    report.unresolved += 1;
                }
                continue;
            }
            if !leavers.is_empty() {
                for membership in entrants.iter().chain(leavers.iter()) {
                    output.push(unresolved_row_with_evidence(
                        &membership.discovery_id,
                        field_id,
                        "single_entrant_positive_delta",
                        "membership_leaver_in_window",
                        after,
                        before,
                        "",
                    ));
                    report.unresolved += 1;
                }
                continue;
            }
            if entrants.is_empty() {
                continue;
            }
            let discovery_id = &entrants[0].discovery_id;
            let mut reason = None;
            if before.conflict || after.conflict {
                reason = Some("conflicting_snapshot");
            } else if current_fields.get(discovery_id) != Some(field_id) {
                reason = Some("current_field_disagrees");
            } else if redirects
                .get(discovery_id)
                .is_some_and(|target| target != discovery_id)
            {
                reason = Some("resource_redirect");
            } else if direct_reported.contains(discovery_id) {
                reason = Some("reported_overlap");
            }
            let (deltas, signed, delta_reason) = deltas(before, after);
            reason = reason.or(delta_reason);
            if let Some(reason) = reason {
                output.push(unresolved_row_with_evidence(
                    discovery_id,
                    field_id,
                    "single_entrant_positive_delta",
                    reason,
                    after,
                    before,
                    &signed,
                ));
                report.unresolved += 1;
                continue;
            }
            output.push(volume_row(
                &format!(
                    "delta-{}",
                    digest(&format!(
                        "{discovery_id}|{}|{}",
                        before.identity, after.identity
                    ))
                ),
                discovery_id,
                discovery_id,
                field_id,
                true,
                true,
                "single_entrant_positive_delta",
                "inclusion_window_change",
                "consecutive_original_recoverable_field_snapshots",
                &after.date,
                "",
                &deltas,
                "",
                &after.identity,
                &after.json,
                &before.identity,
                &before.json,
                1,
                false,
                "",
                &signed,
            ));
            report.inclusion_deltas += 1;
        }
    }
}

fn deltas(before: &Snapshot, after: &Snapshot) -> ([Option<f64>; 5], String, Option<&'static str>) {
    let mut values = [None; 5];
    let mut signed = BTreeMap::new();
    let mut comparable = 0;
    let mut negative = false;
    for (index, (name, _, _)) in COMPONENTS.iter().enumerate() {
        if let (Some(left), Some(right)) = (before.values[index], after.values[index]) {
            let delta = right - left;
            signed.insert(*name, delta);
            comparable += 1;
            negative |= delta < 0.0;
            values[index] = Some(delta);
        }
    }
    let reason = if comparable == 0 {
        Some("no_comparable_components")
    } else if negative {
        Some("negative_component_delta")
    } else {
        None
    };
    (
        values,
        serde_json::to_string(&signed).expect("signed delta serializes"),
        reason,
    )
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
            field_id,
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

fn load_memberships(rows: &[Row]) -> Vec<Membership> {
    let mut out: Vec<_> = rows
        .iter()
        .filter_map(|row| {
            Some(Membership {
                field_id: cell(row, "fldNpdidField")?.to_string(),
                discovery_id: cell(row, "dscNpdidDiscovery")?.to_string(),
                from: cell(row, "fldDiscoveryInclFromDate")
                    .map(normalized_date)
                    .filter(|date| valid_date(date)),
                to: cell(row, "fldDiscoveryInclToDate")
                    .map(normalized_date)
                    .filter(|date| valid_date(date)),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        (&a.field_id, &a.discovery_id, &a.from, &a.to).cmp(&(
            &b.field_id,
            &b.discovery_id,
            &b.from,
            &b.to,
        ))
    });
    out.dedup_by(|a, b| {
        a.field_id == b.field_id
            && a.discovery_id == b.discovery_id
            && a.from == b.from
            && a.to == b.to
    });
    out
}

fn memberships_by_field(memberships: &[Membership]) -> BTreeMap<&str, Vec<&Membership>> {
    let mut out: BTreeMap<&str, Vec<&Membership>> = BTreeMap::new();
    for membership in memberships {
        out.entry(&membership.field_id)
            .or_default()
            .push(membership);
    }
    out
}

fn active_at(membership: &Membership, date: &str) -> bool {
    membership.from.as_deref().is_some_and(|from| from <= date)
        && membership.to.as_deref().is_none_or(|to| date < to)
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
    ]);
    row
}

fn unresolved_row(discovery_id: &str, field_id: &str, method: &str, reason: &str) -> Vec<String> {
    volume_row(
        &format!(
            "unresolved-{}",
            digest(&format!("{discovery_id}|{field_id}|{method}|{reason}"))
        ),
        discovery_id,
        discovery_id,
        field_id,
        true,
        false,
        method,
        "unresolved",
        "",
        "",
        "",
        &[None; 5],
        "",
        "",
        "",
        "",
        "",
        0,
        false,
        reason,
        "",
    )
}

fn unresolved_row_with_evidence(
    discovery_id: &str,
    field_id: &str,
    method: &str,
    reason: &str,
    after: &Snapshot,
    before: &Snapshot,
    signed: &str,
) -> Vec<String> {
    volume_row(
        &format!(
            "unresolved-{}",
            digest(&format!(
                "{discovery_id}|{}|{}|{reason}",
                before.identity, after.identity
            ))
        ),
        discovery_id,
        discovery_id,
        field_id,
        true,
        false,
        method,
        "unresolved",
        "consecutive_original_recoverable_field_snapshots",
        &after.date,
        "",
        &[None; 5],
        "",
        &after.identity,
        &after.json,
        &before.identity,
        &before.json,
        1,
        after.conflict || before.conflict,
        reason,
        signed,
    )
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
    fn reported_rows_dedupe_exactly_and_preserve_rc_redirect_and_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,dscNpdidResInclInDisc,fldNpdidField\n1,2,10\n2,,10\n",
        );
        write(
            tmp.path(),
            "discovery_reserves.csv",
            "OBJECTID,dscNpdidDiscovery,dscDateOffResEstDisplay,dscReservesRC,dscRecoverableOil,dscRecoverableGas,dscRecoverableNGL,dscRecoverableCondensate,dscRecoverableOe\n\
             9,1,1767139200000,4F,10,,,,\n8,1,1767139200000,4F,10,,,,\n7,1,1767139200000,4F,11,,,,\n6,1,1767139200000,5F,0,0,0,0,0\n",
        );
        let report = apply(tmp.path()).unwrap();
        assert_eq!(report.reported, 3);
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        assert!(rows.iter().all(|row| row["dscNpdidDiscovery"] == "1"));
        assert!(rows.iter().all(|row| row["raw_redirect_id"] == "2"));
        let zero = rows
            .iter()
            .find(|row| row["resource_class"] == "5F")
            .unwrap();
        assert_eq!(zero["usable"], "false");
        assert_eq!(zero["unresolved_reason"], "resources_reported_with_parent");
        assert_eq!(zero["recoverable_oil"], "0");
        assert!(rows
            .iter()
            .filter(|row| row["resource_class"] == "4F")
            .all(|row| row["conflict"] == "true"));
        assert!(rows.iter().any(|row| row["source_duplicate_count"] == "2"));
        assert!(rows
            .iter()
            .filter(|row| row["resource_class"] == "4F")
            .all(|row| row["recoverable_gas"].is_empty()));
        assert!(rows.iter().all(|row| row["estimate_date"] == "2025-12-31"));
    }

    #[test]
    fn sole_discovery_gets_latest_original_recoverable_copy_without_zero_fill() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,dscNpdidResInclInDisc,fldNpdidField\n1,,10\n",
        );
        write(
            tmp.path(),
            "field_discoveries_incl_hst.csv",
            "fldNpdidField,dscNpdidDiscovery,fldDiscoveryInclFromDate,fldDiscoveryInclToDate\n10,1,1979-01-01,\n",
        );
        write(
            tmp.path(),
            "field_reserves.csv",
            "OBJECTID,fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableGas,fldRecoverableNGL,fldRecoverableCondensate,fldRecoverableOE\n\
             1,10,2024-12-31,700,300,,,1000\n2,10,2025-12-31,717.349,350.956,,,1068.305\n",
        );
        let report = apply(tmp.path()).unwrap();
        assert_eq!(report.singleton_copies, 1);
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        let row = rows.iter().find(|row| row["usable"] == "true").unwrap();
        assert_eq!(row["method"], "singleton_field_copy");
        assert_eq!(row["estimate_date"], "2025-12-31");
        assert_eq!(row["recoverable_oil"], "717.349");
        assert_eq!(row["recoverable_gas"], "350.956");
        assert!(row["recoverable_ngl"].is_empty());
    }

    #[test]
    fn entrant_delta_is_signed_and_negative_change_is_unresolved() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "discovery.csv",
            "dscNpdidDiscovery,dscNpdidResInclInDisc,fldNpdidField\n1,,10\n2,,10\n3,,10\n",
        );
        write(
            tmp.path(),
            "field_discoveries_incl_hst.csv",
            "fldNpdidField,dscNpdidDiscovery,fldDiscoveryInclFromDate,fldDiscoveryInclToDate\n\
             10,1,1990-01-01,\n10,2,2021-06-01,\n10,3,2022-06-01,\n",
        );
        write(
            tmp.path(),
            "field_reserves.csv",
            "fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableGas\n\
             10,2020-12-31,100,100\n10,2021-12-31,120,130\n10,2022-12-31,110,150\n",
        );
        let report = apply(tmp.path()).unwrap();
        assert_eq!(report.inclusion_deltas, 1);
        assert_eq!(report.unresolved, 4);
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        let delta = rows
            .iter()
            .find(|row| {
                row["dscNpdidDiscovery"] == "2" && row["method"] == "single_entrant_positive_delta"
            })
            .unwrap();
        assert_eq!(delta["recoverable_oil"], "20");
        assert_eq!(delta["recoverable_gas"], "30");
        let unresolved = rows
            .iter()
            .find(|row| {
                row["dscNpdidDiscovery"] == "3" && row["method"] == "single_entrant_positive_delta"
            })
            .unwrap();
        assert_eq!(unresolved["usable"], "false");
        assert_eq!(unresolved["unresolved_reason"], "negative_component_delta");
        assert!(unresolved["recoverable_oil"].is_empty());
        assert!(unresolved["signed_deltas_json"].contains("-10"));
    }

    #[test]
    fn troll_allocation_reconciles_latest_snapshot_and_rejects_direct_overlap() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "discovery.csv", "dscNpdidDiscovery,dscNpdidResInclInDisc,fldNpdidField\n44552,44534,46437\n44534,,46437\n");
        write(tmp.path(), "field_discoveries_incl_hst.csv", "fldNpdidField,dscNpdidDiscovery,fldDiscoveryInclFromDate,fldDiscoveryInclToDate\n46437,44552,1990-01-01,\n46437,44534,1990-01-01,\n");
        write(tmp.path(), "field_reserves.csv", "OBJECTID,fldNpdidField,fldDateOffResEstDisplay,fldRecoverableOil,fldRecoverableGas,fldRecoverableNGL,fldRecoverableCondensate,fldRecoverableOE\n1,46437,2024-12-31,100,300,30,3,433\n2,46437,2025-12-31,120,330,33,6,489\n");
        let report = apply(tmp.path()).unwrap();
        assert_eq!(report.unresolved, 0);
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        assert_eq!(rows.len(), 2);
        let east = rows
            .iter()
            .find(|row| row["dscNpdidDiscovery"] == TROLL_EAST)
            .unwrap();
        let west = rows
            .iter()
            .find(|row| row["dscNpdidDiscovery"] == TROLL_WEST)
            .unwrap();
        assert_eq!(east["method"], "troll_published_component_allocation");
        assert_eq!(east["raw_redirect_id"], TROLL_WEST);
        assert_eq!(east["recoverable_oil"], "0");
        assert_eq!(east["recoverable_gas"].parse::<f64>().unwrap(), 220.0);
        assert_eq!(west["recoverable_oil"], "120");
        assert_eq!(west["recoverable_gas"].parse::<f64>().unwrap(), 110.0);
        assert_eq!(
            east["recoverable_oe"].parse::<f64>().unwrap()
                + west["recoverable_oe"].parse::<f64>().unwrap(),
            489.0
        );

        write(
            tmp.path(),
            "discovery_reserves.csv",
            "dscNpdidDiscovery,dscDateOffResEstDisplay,dscRecoverableOil\n44552,2025-12-31,1\n",
        );
        apply(tmp.path()).unwrap();
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        assert!(!rows.iter().any(
            |row| row["method"] == "troll_published_component_allocation"
                && row["usable"] == "true"
        ));
        assert!(rows
            .iter()
            .any(|row| row["unresolved_reason"] == "reported_overlap"));
    }

    #[test]
    fn gjoa_nord_zero_parent_observation_is_unusable_and_midpoint_is_component_null() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "discovery.csv", "dscNpdidDiscovery,dscName,dscNpdidResInclInDisc,fldNpdidField\n45651,35/9-3 (Gjøa Nord),44786,4467574\n44786,35/9-1 Gjøa,,4467574\n");
        write(tmp.path(), "discovery_reserves.csv", "dscNpdidDiscovery,dscDateOffResEstDisplay,dscReservesRC,dscRecoverableOil,dscRecoverableGas,dscRecoverableNGL,dscRecoverableCondensate,dscRecoverableOe\n45651,2025-12-31,5F,0,0,0,0,0\n");
        apply(tmp.path()).unwrap();
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        let source = rows
            .iter()
            .find(|row| row["method"] == "reported_observation")
            .unwrap();
        assert_eq!(source["usable"], "false");
        assert_eq!(
            source["unresolved_reason"],
            "resources_reported_with_parent"
        );
        assert_eq!(source["recoverable_oe"], "0");
        let midpoint = rows
            .iter()
            .find(|row| row["method"] == "published_resource_range_midpoint")
            .unwrap();
        assert_eq!(midpoint["usable"], "true");
        assert_eq!(midpoint["estimate_date"], "2022-05-12");
        assert_eq!(midpoint["recoverable_oe"], "2.8");
        assert!(midpoint["recoverable_oil"].is_empty());
        assert!(midpoint["recoverable_gas"].is_empty());
        assert!(midpoint["source_record_json"].contains("million_sm3_oil_equivalent"));

        write(tmp.path(), "discovery.csv", "dscNpdidDiscovery,dscName,dscNpdidResInclInDisc,fldNpdidField\n45651,35/9-3 (Gjøa Nord),44786,999\n44786,35/9-1 Gjøa,,4467574\n");
        apply(tmp.path()).unwrap();
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        assert!(!rows
            .iter()
            .any(|row| row["method"] == "published_resource_range_midpoint"));

        write(tmp.path(), "discovery.csv", "dscNpdidDiscovery,dscName,dscNpdidResInclInDisc,fldNpdidField\n45651,35/9-3 (Gjøa Nord),44786,4467574\n44786,35/9-1 Gjøa,,4467574\n");
        write(tmp.path(), "discovery_reserves.csv", "dscNpdidDiscovery,dscDateOffResEstDisplay,dscReservesRC,dscRecoverableOil,dscRecoverableGas,dscRecoverableNGL,dscRecoverableCondensate,dscRecoverableOe\n45651,2025-12-31,5F,0,0,0,0,1\n");
        apply(tmp.path()).unwrap();
        let rows = read_rows(&tmp.path().join(OUTPUT)).unwrap();
        assert!(!rows
            .iter()
            .any(|row| row["method"] == "published_resource_range_midpoint"));
    }

    #[test]
    fn published_range_midpoint_has_stable_decimal_precision() {
        use crate::sodir::estimates::{EstimateValue, Unit};
        let value = EstimateValue {
            unit: Unit::MillionSm3OilEquivalent,
            point: None,
            minimum: Some(2.7),
            maximum: Some(7.4),
            preliminary: true,
        };
        assert_eq!(
            estimate_value(Some(value), Unit::MillionSm3OilEquivalent),
            Some(5.05)
        );
    }
}
