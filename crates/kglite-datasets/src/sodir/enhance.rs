//! Deterministic discovery-to-play inference over cached SODIR CSVs.

use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use geo::{Contains, CoordsIter, Distance, Euclidean, Geometry, Intersects, MapCoords};
use wkt::Wkt;

use crate::sodir::ages::{self, Compatibility};
use crate::sodir::error::{Result, SodirError};

pub const VERSION: u32 = 14;
const OUTPUT: &str = "_derived_discovery_play.csv";
const CANDIDATE_OUTPUT: &str = "_derived_discovery_play_candidate.csv";
const FIELD_OUTPUT: &str = "_derived_field_play.csv";
const EARTH_RADIUS_M: f64 = 6_371_008.8;
const DISTANCE_TIE_EPSILON_M: f64 = 0.001;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnhancementReport {
    pub links: usize,
    pub contained: usize,
    pub nearest: usize,
    pub unmatched: usize,
    pub candidates: usize,
    pub field_links: usize,
}

#[derive(Clone)]
struct Well {
    id: String,
    point: geo::Point<f64>,
    ages: [String; 3],
}

#[derive(Clone)]
struct Play {
    id: String,
    geometry: Geometry<f64>,
    age: String,
}

struct Candidate<'a> {
    play: &'a Play,
    compatibility: Compatibility,
    matched: Vec<ages::AgeAtom>,
    matched_slots: Vec<usize>,
    distance_m: f64,
}

/// Rebuild the derived junction from the three source CSVs.
pub fn apply(csv_dir: &Path) -> Result<EnhancementReport> {
    let discovery_path = csv_dir.join("discovery.csv");
    let well_path = csv_dir.join("wellbore.csv");
    let play_path = csv_dir.join("play.csv");
    let output = csv_dir.join(OUTPUT);
    if !discovery_path.is_file() || !well_path.is_file() || !play_path.is_file() {
        if is_owned_output(&output) {
            write_links(&output, &[])?;
        }
        let candidates = csv_dir.join(CANDIDATE_OUTPUT);
        if is_owned_table(
            &candidates,
            "dscNpdidDiscovery,plyNPDID,wlbNpdidWellbore,rejection_reason,",
        ) {
            write_candidates(&candidates, &[])?;
        }
        let fields = csv_dir.join(FIELD_OUTPUT);
        if is_owned_table(&fields, "fldNpdidField,plyNPDID,match_method,inferred,") {
            write_field_links(&fields, &[])?;
        }
        return Ok(EnhancementReport::default());
    }

    let discoveries = read_records(&discovery_path)?;
    let wells = load_wells(&well_path)?;
    let plays = load_plays(&play_path)?;
    let play_catalog: BTreeMap<String, String> = read_records(&play_path)?
        .into_iter()
        .filter_map(|row| {
            Some((
                cell(&row, "plyNPDID")?.to_string(),
                cell(&row, "plyName")?.to_string(),
            ))
        })
        .collect();
    let mut links = Vec::new();
    let mut diagnostics = Vec::new();
    let mut field_links = Vec::new();
    let mut report = EnhancementReport::default();

    let mut discovery_groups: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for discovery in &discoveries {
        if let Some(id) = cell(discovery, "dscNpdidDiscovery") {
            discovery_groups
                .entry(id.to_string())
                .or_default()
                .push(discovery);
        } else {
            report.unmatched += 1;
        }
    }

    let mut published_by_field: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for discovery in &discoveries {
        let (Some(field_id), Some(field_name)) =
            (cell(discovery, "fldNpdidField"), cell(discovery, "fldName"))
        else {
            continue;
        };
        let Ok(field_id_number) = field_id.parse::<u64>() else {
            continue;
        };
        published_by_field
            .entry(field_id.to_string())
            .or_insert_with(|| {
                crate::sodir::play_examples::examples_for_field(field_name, field_id_number)
                    .filter(|example| {
                        play_catalog
                            .get(&example.play_id.to_string())
                            .is_some_and(|name| name == example.play_name)
                    })
                    .collect()
            });
    }
    for (field_id, examples) in &published_by_field {
        for example in examples {
            field_links.push(vec![
                field_id.clone(),
                example.play_id.to_string(),
                "published_example".into(),
                "false".into(),
                example.source_label.into(),
                example.source_url.into(),
                example.accessed_on.into(),
            ]);
        }
    }
    let mut published_by_discovery: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for discovery in &discoveries {
        let (Some(discovery_id), Some(discovery_name)) = (
            cell(discovery, "dscNpdidDiscovery"),
            cell(discovery, "dscName"),
        ) else {
            continue;
        };
        let Ok(id_number) = discovery_id.parse::<u64>() else {
            continue;
        };
        published_by_discovery
            .entry(discovery_id.to_string())
            .or_insert_with(|| {
                crate::sodir::play_examples::examples_for_discovery(discovery_name, id_number)
                    .filter(|example| {
                        play_catalog
                            .get(&example.play_id.to_string())
                            .is_some_and(|name| name == example.play_name)
                    })
                    .collect()
            });
    }

    for (discovery_id, rows) in discovery_groups {
        let signatures: std::collections::BTreeSet<_> = rows
            .iter()
            .map(|row| {
                (
                    cell(row, "wlbNpdidWellbore").unwrap_or_default(),
                    cell(row, "fldNpdidField").unwrap_or_default(),
                )
            })
            .collect();
        if signatures.len() != 1 {
            report.unmatched += 1;
            diagnostics.push(diagnostic_row(
                &discovery_id,
                "",
                "",
                "conflicting_discovery_rows",
            ));
            continue;
        }
        let discovery = rows[0];
        let published = published_by_discovery
            .get(&discovery_id)
            .cloned()
            .unwrap_or_default();
        let published_play_ids: std::collections::BTreeSet<_> = published
            .iter()
            .map(|example| example.play_id.to_string())
            .collect();
        for example in &published {
            links.push(published_discovery_link_row(&discovery_id, example));
        }
        let Some(well_id) = cell(discovery, "wlbNpdidWellbore") else {
            if published.is_empty() {
                report.unmatched += 1;
            }
            continue;
        };
        let Some(well) = wells.get(well_id) else {
            if published.is_empty() {
                report.unmatched += 1;
            }
            continue;
        };

        let mut candidates: Vec<Candidate<'_>> = plays
            .iter()
            .filter_map(|play| candidate(well, play))
            .filter(|candidate| candidate.distance_m.is_finite())
            .collect();
        if candidates.is_empty() {
            if published.is_empty() {
                report.unmatched += 1;
            }
            continue;
        }
        candidates.sort_by(|a, b| {
            a.distance_m
                .total_cmp(&b.distance_m)
                .then_with(|| stable_id_cmp(&a.play.id, &b.play.id))
        });
        let contained: Vec<_> = candidates
            .iter()
            .filter(|c| c.distance_m <= DISTANCE_TIE_EPSILON_M)
            .collect();
        if !contained.is_empty() {
            for candidate in &contained {
                if !published_play_ids.contains(&candidate.play.id) {
                    links.push(inferred_link_row(
                        &discovery_id,
                        well,
                        candidate,
                        "contains",
                        contained.len(),
                        false,
                    ));
                    report.contained += 1;
                }
            }
            for candidate in candidates.iter().filter(|c| {
                c.distance_m > DISTANCE_TIE_EPSILON_M && !published_play_ids.contains(&c.play.id)
            }) {
                diagnostics.push(candidate_diagnostic_row(
                    &discovery_id,
                    well,
                    candidate,
                    "not_containing",
                ));
            }
        } else if published.is_empty() {
            let nearest_distance = candidates[0].distance_m;
            let nearest: Vec<_> = candidates
                .iter()
                .filter(|c| (c.distance_m - nearest_distance).abs() <= DISTANCE_TIE_EPSILON_M)
                .collect();
            for candidate in &nearest {
                links.push(inferred_link_row(
                    &discovery_id,
                    well,
                    candidate,
                    "nearest",
                    nearest.len(),
                    nearest.len() > 1,
                ));
                report.nearest += 1;
            }
            for candidate in candidates
                .iter()
                .filter(|c| (c.distance_m - nearest_distance).abs() > DISTANCE_TIE_EPSILON_M)
            {
                diagnostics.push(candidate_diagnostic_row(
                    &discovery_id,
                    well,
                    candidate,
                    "farther_fallback_candidate",
                ));
            }
        }
    }
    links.sort_by(|a, b| stable_id_cmp(&a[0], &b[0]).then_with(|| stable_id_cmp(&a[1], &b[1])));
    links.dedup_by(|left, right| left[0] == right[0] && left[1] == right[1]);
    diagnostics.sort_by(|a, b| {
        stable_id_cmp(&a[0], &b[0])
            .then_with(|| stable_id_cmp(&a[1], &b[1]))
            .then_with(|| a[3].cmp(&b[3]))
    });
    diagnostics
        .dedup_by(|left, right| left[0] == right[0] && left[1] == right[1] && left[3] == right[3]);
    report.links = links.len();
    report.candidates = diagnostics.len();
    report.field_links = field_links.len();
    write_links(&output, &links)?;
    write_candidates(&csv_dir.join(CANDIDATE_OUTPUT), &diagnostics)?;
    write_field_links(&csv_dir.join(FIELD_OUTPUT), &field_links)?;
    Ok(report)
}

fn candidate<'a>(well: &Well, play: &'a Play) -> Option<Candidate<'a>> {
    let mut best: Option<(Compatibility, Vec<ages::AgeAtom>, Vec<usize>)> = None;
    for (index, source_age) in well.ages.iter().enumerate() {
        let Some(matched) = ages::compatibility(source_age, &play.age) else {
            continue;
        };
        match &mut best {
            None => best = Some((matched.compatibility, matched.atoms, vec![index + 1])),
            Some((compatibility, atoms, slots)) => {
                if compatibility_rank(matched.compatibility) > compatibility_rank(*compatibility) {
                    *compatibility = matched.compatibility;
                }
                atoms.extend(matched.atoms);
                atoms.sort();
                atoms.dedup();
                slots.push(index + 1);
            }
        }
    }
    let (compatibility, matched, matched_slots) = best?;
    Some(Candidate {
        play,
        compatibility,
        matched,
        matched_slots,
        distance_m: distance_metres(well.point, &play.geometry),
    })
}

fn inferred_link_row(
    discovery_id: &str,
    well: &Well,
    selected: &Candidate<'_>,
    method: &str,
    candidate_count: usize,
    ambiguous: bool,
) -> Vec<String> {
    vec![
        discovery_id.into(),
        selected.play.id.clone(),
        well.id.clone(),
        "sodir_designated_discovery_wellbore".into(),
        method.into(),
        format!("{:.3}", selected.distance_m),
        compatibility_name(selected.compatibility).into(),
        json_atoms(&selected.matched),
        serde_json::to_string(&selected.matched_slots).expect("slot list serializes"),
        json_atoms(&normalized_well_ages(well)),
        json_atoms(ages::normalize(&selected.play.age).unwrap_or_default()),
        "local_equirectangular_polygon_boundary".into(),
        "true".into(),
        candidate_count.to_string(),
        ambiguous.to_string(),
        "designated_discovery_wellbore".into(),
        String::new(),
        String::new(),
    ]
}

fn published_discovery_link_row(
    discovery_id: &str,
    example: &crate::sodir::play_examples::PublishedDiscoveryExample,
) -> Vec<String> {
    vec![
        discovery_id.into(),
        example.play_id.to_string(),
        String::new(),
        example.source_label.into(),
        "published_discovery_example".into(),
        String::new(),
        String::new(),
        "[]".into(),
        "[]".into(),
        "[]".into(),
        "[]".into(),
        String::new(),
        "false".into(),
        "1".into(),
        "false".into(),
        "discovery_published_example".into(),
        example.source_url.into(),
        example.accessed_on.into(),
    ]
}

fn candidate_diagnostic_row(
    discovery_id: &str,
    well: &Well,
    candidate: &Candidate<'_>,
    reason: &str,
) -> Vec<String> {
    vec![
        discovery_id.into(),
        candidate.play.id.clone(),
        well.id.clone(),
        reason.into(),
        if candidate.distance_m <= DISTANCE_TIE_EPSILON_M {
            "contains".into()
        } else {
            "nearest".into()
        },
        format!("{:.3}", candidate.distance_m),
        compatibility_name(candidate.compatibility).into(),
        json_atoms(&candidate.matched),
        serde_json::to_string(&candidate.matched_slots).expect("slot list serializes"),
        json_atoms(&normalized_well_ages(well)),
        json_atoms(ages::normalize(&candidate.play.age).unwrap_or_default()),
        "local_equirectangular_polygon_boundary".into(),
    ]
}

fn diagnostic_row(discovery_id: &str, play_id: &str, well_id: &str, reason: &str) -> Vec<String> {
    vec![
        discovery_id.into(),
        play_id.into(),
        well_id.into(),
        reason.into(),
        String::new(),
        String::new(),
        String::new(),
        "[]".into(),
        "[]".into(),
        "[]".into(),
        "[]".into(),
        String::new(),
    ]
}

fn compatibility_rank(value: Compatibility) -> u8 {
    match value {
        Compatibility::Partial => 1,
        Compatibility::Full => 2,
    }
}

fn compatibility_name(value: Compatibility) -> &'static str {
    match value {
        Compatibility::Partial => "partial",
        Compatibility::Full => "full",
    }
}

fn normalized_well_ages(well: &Well) -> Vec<ages::AgeAtom> {
    let mut atoms: Vec<_> = well
        .ages
        .iter()
        .filter_map(|source| ages::normalize(source))
        .flatten()
        .copied()
        .collect();
    atoms.sort();
    atoms.dedup();
    atoms
}

fn json_atoms(atoms: &[ages::AgeAtom]) -> String {
    serde_json::to_string(&atoms.iter().map(|atom| atom.as_str()).collect::<Vec<_>>())
        .expect("age atom list serializes")
}

fn distance_metres(origin: geo::Point<f64>, geometry: &Geometry<f64>) -> f64 {
    let lat0 = origin.y().to_radians();
    let lon0 = origin.x().to_radians();
    let projected = geometry.map_coords(|coord| geo::Coord {
        x: EARTH_RADIUS_M * (coord.x.to_radians() - lon0) * lat0.cos(),
        y: EARTH_RADIUS_M * (coord.y.to_radians() - lat0),
    });
    let point = geo::Point::new(0.0, 0.0);
    if projected.contains(&point) || projected.intersects(&point) {
        0.0
    } else {
        Euclidean::distance(&projected, &point)
    }
}

fn load_wells(path: &Path) -> Result<BTreeMap<String, Well>> {
    let mut out = BTreeMap::new();
    for row in read_records(path)? {
        let (Some(id), Some(geometry)) = (
            cell(&row, "wlbNpdidWellbore"),
            cell(&row, "wkt_geometry").and_then(parse_geometry),
        ) else {
            continue;
        };
        let Some(point) =
            point_from_geometry(&geometry).filter(|point| valid_coord(point.x(), point.y()))
        else {
            continue;
        };
        out.insert(
            id.to_string(),
            Well {
                id: id.to_string(),
                point,
                ages: [
                    cell(&row, "wlbAgeWithHc1").unwrap_or_default().to_string(),
                    cell(&row, "wlbAgeWithHc2").unwrap_or_default().to_string(),
                    cell(&row, "wlbAgeWithHc3").unwrap_or_default().to_string(),
                ],
            },
        );
    }
    Ok(out)
}

fn load_plays(path: &Path) -> Result<Vec<Play>> {
    let mut out = Vec::new();
    for row in read_records(path)? {
        let (Some(id), Some(age), Some(geometry)) = (
            cell(&row, "plyNPDID"),
            cell(&row, "plyAge"),
            cell(&row, "wkt_geometry").and_then(parse_geometry),
        ) else {
            continue;
        };
        if matches!(geometry, Geometry::Polygon(_) | Geometry::MultiPolygon(_))
            && geometry
                .coords_iter()
                .all(|coord| valid_coord(coord.x, coord.y))
            && geometry.coords_count() >= 4
        {
            out.push(Play {
                id: id.to_string(),
                geometry,
                age: age.to_string(),
            });
        }
    }
    out.sort_by(|a, b| stable_id_cmp(&a.id, &b.id));
    Ok(out)
}

fn parse_geometry(value: &str) -> Option<Geometry<f64>> {
    Geometry::try_from(Wkt::<f64>::from_str(value).ok()?).ok()
}

fn point_from_geometry(geometry: &Geometry<f64>) -> Option<geo::Point<f64>> {
    match geometry {
        Geometry::Point(point) => Some(*point),
        _ => None,
    }
}

fn valid_coord(longitude: f64, latitude: f64) -> bool {
    longitude.is_finite()
        && latitude.is_finite()
        && (-180.0..=180.0).contains(&longitude)
        && (-90.0..=90.0).contains(&latitude)
}

type Row = BTreeMap<String, String>;

fn read_records(path: &Path) -> Result<Vec<Row>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|error| SodirError::Csv(format!("open {}: {error}", path.display())))?;
    let headers = reader
        .headers()
        .map_err(|error| SodirError::Csv(format!("headers {}: {error}", path.display())))?
        .clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record =
            record.map_err(|error| SodirError::Csv(format!("row {}: {error}", path.display())))?;
        rows.push(
            headers
                .iter()
                .zip(record.iter())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        );
    }
    Ok(rows)
}

fn cell<'a>(row: &'a Row, column: &str) -> Option<&'a str> {
    row.get(column)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn write_links(path: &Path, rows: &[Vec<String>]) -> Result<()> {
    let tmp = path.with_extension("csv.tmp");
    let mut writer = csv::Writer::from_path(&tmp)
        .map_err(|error| SodirError::Csv(format!("open {}: {error}", tmp.display())))?;
    writer
        .write_record([
            "dscNpdidDiscovery",
            "plyNPDID",
            "wlbNpdidWellbore",
            "source",
            "match_method",
            "distance_m",
            "age_compatibility",
            "matched_ages",
            "matched_hc_slots",
            "wellbore_ages",
            "play_ages",
            "distance_method",
            "inferred",
            "candidate_tie_count",
            "ambiguous",
            "evidence_scope",
            "source_url",
            "source_accessed_on",
        ])
        .map_err(|error| SodirError::Csv(format!("header {}: {error}", tmp.display())))?;
    for row in rows {
        writer
            .write_record(row)
            .map_err(|error| SodirError::Csv(format!("row {}: {error}", tmp.display())))?;
    }
    writer.flush()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn write_candidates(path: &Path, rows: &[Vec<String>]) -> Result<()> {
    write_table(
        path,
        &[
            "dscNpdidDiscovery",
            "plyNPDID",
            "wlbNpdidWellbore",
            "rejection_reason",
            "match_method",
            "distance_m",
            "age_compatibility",
            "matched_ages",
            "matched_hc_slots",
            "wellbore_ages",
            "play_ages",
            "distance_method",
        ],
        rows,
    )
}

fn write_field_links(path: &Path, rows: &[Vec<String>]) -> Result<()> {
    write_table(
        path,
        &[
            "fldNpdidField",
            "plyNPDID",
            "match_method",
            "inferred",
            "source_label",
            "source_url",
            "source_accessed_on",
        ],
        rows,
    )
}

fn write_table(path: &Path, header: &[&str], rows: &[Vec<String>]) -> Result<()> {
    let tmp = path.with_extension("csv.tmp");
    let mut writer = csv::Writer::from_path(&tmp)
        .map_err(|error| SodirError::Csv(format!("open {}: {error}", tmp.display())))?;
    writer
        .write_record(header)
        .map_err(|error| SodirError::Csv(format!("header {}: {error}", tmp.display())))?;
    for row in rows {
        writer
            .write_record(row)
            .map_err(|error| SodirError::Csv(format!("row {}: {error}", tmp.display())))?;
    }
    writer.flush()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn is_owned_output(path: &Path) -> bool {
    is_owned_table(
        path,
        "dscNpdidDiscovery,plyNPDID,wlbNpdidWellbore,source,match_method,distance_m,",
    )
}

fn is_owned_table(path: &Path, prefix: &str) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| contents.lines().next().map(str::to_string))
        .is_some_and(|header| header.starts_with(prefix))
}

fn stable_id_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn projected_distance_honours_boundary_holes_and_multipolygons() {
        let polygon =
            parse_geometry("POLYGON ((0 0, 4 0, 4 4, 0 4, 0 0), (1 1, 1 3, 3 3, 3 1, 1 1))")
                .unwrap();
        assert!(distance_metres(geo::Point::new(0.0, 2.0), &polygon) < 0.001);
        assert!(distance_metres(geo::Point::new(2.0, 2.0), &polygon) > 100_000.0);

        let multipolygon = parse_geometry(
            "MULTIPOLYGON (((0 0, 1 0, 1 1, 0 1, 0 0)), ((3 3, 4 3, 4 4, 3 4, 3 3)))",
        )
        .unwrap();
        assert_eq!(
            distance_metres(geo::Point::new(3.5, 3.5), &multipolygon),
            0.0
        );
    }

    #[test]
    fn derives_containment_then_nearest_with_stable_provenance() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "discovery.csv",
            "dscNpdidDiscovery,wlbNpdidWellbore\n20,200\n10,100\n30,300\n",
        );
        write(
            dir,
            "wellbore.csv",
            "wlbNpdidWellbore,wlbAgeWithHc1,wlbAgeWithHc2,wlbAgeWithHc3,wkt_geometry\n\
             100,EARLY JURASSIC,INDETERMINATE,,POINT (1 1)\n\
             200,LATE JURASSIC,,,POINT (10 10)\n\
             300,INDETERMINATE,,,POINT (1 1)\n",
        );
        write(
            dir,
            "play.csv",
            "plyNPDID,plyAge,wkt_geometry\n\
             2,Lower-Middle Jurassic,\"POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))\"\n\
             1,Upper Jurassic,\"MULTIPOLYGON (((8 8, 9 8, 9 9, 8 9, 8 8)), ((12 12, 13 12, 13 13, 12 13, 12 12)))\"\n",
        );

        let report = apply(dir).unwrap();
        assert_eq!(
            report,
            EnhancementReport {
                links: 2,
                contained: 1,
                nearest: 1,
                unmatched: 1,
                candidates: 0,
                field_links: 0,
            }
        );
        let first = std::fs::read_to_string(dir.join(OUTPUT)).unwrap();
        let derived = read_records(&dir.join(OUTPUT)).unwrap();
        assert_eq!(derived[0]["dscNpdidDiscovery"], "10");
        assert_eq!(derived[0]["plyNPDID"], "2");
        assert_eq!(derived[0]["match_method"], "contains");
        assert_eq!(derived[0]["matched_ages"], r#"["Lower Jurassic"]"#);
        assert_eq!(derived[0]["matched_hc_slots"], "[1]");
        assert_eq!(derived[0]["wellbore_ages"], r#"["Lower Jurassic"]"#);
        assert_eq!(
            derived[0]["play_ages"],
            r#"["Lower Jurassic","Middle Jurassic"]"#
        );
        assert!(first.contains("20,1,200,sodir_designated_discovery_wellbore,nearest,"));

        apply(dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(OUTPUT)).unwrap(), first);

        write(
            dir,
            "discovery.csv",
            "dscNpdidDiscovery,wlbNpdidWellbore\n10,100\n",
        );
        let report = apply(dir).unwrap();
        assert_eq!(report.links, 1);
        assert!(!std::fs::read_to_string(dir.join(OUTPUT))
            .unwrap()
            .contains("20,1,200"));
    }

    #[test]
    fn malformed_or_non_finite_geometry_never_fabricates_a_link() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "discovery.csv",
            "dscNpdidDiscovery,wlbNpdidWellbore\n1,1\n2,2\n3,3\n",
        );
        write(
            dir,
            "wellbore.csv",
            "wlbNpdidWellbore,wlbAgeWithHc1,wlbAgeWithHc2,wlbAgeWithHc3,wkt_geometry\n\
             1,EARLY JURASSIC,,,POINT (NaN 61)\n\
             2,EARLY JURASSIC,,,POINT (2 181)\n\
             3,EARLY JURASSIC,,,not-wkt\n",
        );
        write(
            dir,
            "play.csv",
            "plyNPDID,plyAge,wkt_geometry\n1,Lower-Middle Jurassic,POLYGON EMPTY\n",
        );
        let report = apply(dir).unwrap();
        assert_eq!(report.links, 0);
        assert_eq!(report.unmatched, 3);
        assert!(!std::fs::read_to_string(dir.join(OUTPUT))
            .unwrap()
            .contains("NaN"));
    }

    #[test]
    fn missing_prerequisites_do_not_overwrite_caller_data() {
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join(OUTPUT);
        std::fs::write(&output, "caller,owned\n1,value\n").unwrap();
        assert_eq!(apply(tmp.path()).unwrap(), EnhancementReport::default());
        assert_eq!(
            std::fs::read_to_string(output).unwrap(),
            "caller,owned\n1,value\n"
        );
    }

    #[test]
    fn all_compatible_hc_slots_are_preserved_as_provenance() {
        let well = Well {
            id: "1".into(),
            point: geo::Point::new(1.0, 1.0),
            ages: ["JURASSIC".into(), "EARLY JURASSIC".into(), String::new()],
        };
        let play = Play {
            id: "1".into(),
            geometry: parse_geometry("POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))").unwrap(),
            age: "Lower-Middle Jurassic".into(),
        };
        let candidate = candidate(&well, &play).unwrap();
        assert_eq!(candidate.matched_slots, vec![1, 2]);
        assert_eq!(candidate.compatibility, Compatibility::Full);
    }

    #[test]
    fn duplicate_discovery_conflict_is_unassigned_and_diagnosed_once() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "discovery.csv",
            "dscNpdidDiscovery,wlbNpdidWellbore,fldNpdidField,fldName\n1,10,,\n1,11,,\n",
        );
        write(dir, "wellbore.csv", "wlbNpdidWellbore,wlbAgeWithHc1,wkt_geometry\n10,EARLY JURASSIC,POINT (1 1)\n11,EARLY JURASSIC,POINT (1 1)\n");
        write(dir, "play.csv", "plyNPDID,plyAge,wkt_geometry\n1,Lower-Middle Jurassic,\"POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))\"\n");
        let report = apply(dir).unwrap();
        assert_eq!(report.links, 0);
        assert_eq!(report.unmatched, 1);
        let diagnostics = read_records(&dir.join(CANDIDATE_OUTPUT)).unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0]["rejection_reason"],
            "conflicting_discovery_rows"
        );
    }

    #[test]
    fn published_field_example_remains_field_scoped() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "discovery.csv",
            "dscNpdidDiscovery,wlbNpdidWellbore,fldNpdidField,fldName\n1,,46437,TROLL\n",
        );
        write(dir, "wellbore.csv", "wlbNpdidWellbore,wkt_geometry\n");
        write(dir, "play.csv", "plyNPDID,plyName,plyAge,wkt_geometry\n242,nju-1,Upper Jurassic,\"POLYGON ((0 0, 1 0, 1 1, 0 1, 0 0))\"\n");
        let report = apply(dir).unwrap();
        assert_eq!(report.links, 0);
        assert_eq!(report.field_links, 1);
        let assigned = read_records(&dir.join(OUTPUT)).unwrap();
        assert!(assigned.is_empty());
        let fields = read_records(&dir.join(FIELD_OUTPUT)).unwrap();
        assert_eq!(fields[0]["match_method"], "published_example");
        assert_eq!(fields[0]["inferred"], "false");

        write(
            dir,
            "play.csv",
            "plyNPDID,plyName,plyAge,wkt_geometry\n242,RENAMED,Upper Jurassic,\"POLYGON ((0 0, 1 0, 1 1, 0 1, 0 0))\"\n",
        );
        let report = apply(dir).unwrap();
        assert_eq!(report.links, 0);
        assert_eq!(report.field_links, 0);
    }

    #[test]
    fn published_gjoa_nord_plays_do_not_inherit_the_field_play() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "discovery.csv", "dscNpdidDiscovery,dscName,wlbNpdidWellbore,fldNpdidField,fldName\n45651,35/9-3 (Gjøa Nord),,4467574,GJØA\n");
        write(dir, "wellbore.csv", "wlbNpdidWellbore,wkt_geometry\n");
        write(dir, "play.csv", "plyNPDID,plyName,plyAge,wkt_geometry\n242,nju-1,Upper Jurassic,\n243,nkl-2,Cretaceous,\n247,nku-5,Upper Cretaceous,\n");
        let report = apply(dir).unwrap();
        assert_eq!(report.links, 2);
        assert_eq!(report.field_links, 1);
        let assigned = read_records(&dir.join(OUTPUT)).unwrap();
        assert_eq!(
            assigned
                .iter()
                .map(|row| row["plyNPDID"].as_str())
                .collect::<Vec<_>>(),
            vec!["243", "247"]
        );
        assert!(assigned
            .iter()
            .all(|row| row["match_method"] == "published_discovery_example"));
        assert!(!assigned.iter().any(|row| row["plyNPDID"] == "242"));
    }

    #[test]
    fn assigns_all_containing_plays_across_hc_slots_once_per_pair() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(
            dir,
            "discovery.csv",
            "dscNpdidDiscovery,dscName,wlbNpdidWellbore\n1,Synthetic,10\n",
        );
        write(dir, "wellbore.csv", "wlbNpdidWellbore,wlbAgeWithHc1,wlbAgeWithHc2,wlbAgeWithHc3,wkt_geometry\n10,EARLY JURASSIC,LATE JURASSIC,,POINT (1 1)\n");
        write(dir, "play.csv", "plyNPDID,plyName,plyAge,wkt_geometry\n1,lower,Lower-Middle Jurassic,\"POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))\"\n2,upper,Upper Jurassic,\"POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))\"\n2,upper,Upper Jurassic,\"POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))\"\n");
        let report = apply(dir).unwrap();
        assert_eq!(report.links, 2);
        let rows = read_records(&dir.join(OUTPUT)).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row["plyNPDID"].as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2"]
        );
        assert_eq!(rows[0]["matched_hc_slots"], "[1]");
        assert_eq!(rows[1]["matched_hc_slots"], "[2]");
        assert!(read_records(&dir.join(CANDIDATE_OUTPUT))
            .unwrap()
            .is_empty());
    }
}
