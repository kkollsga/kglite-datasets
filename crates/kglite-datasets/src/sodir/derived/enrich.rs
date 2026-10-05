//! Public enrichment: formation links and spatial containment.
//!
//! - `HC_IN_FORMATION` (Discovery → Stratigraphy): the formations with
//!   hydrocarbons in the discovery's discovery wellbore, ranked by the
//!   wellbore's HC slot (`wlbFormationWithHc1..3`, rank 1..3). A formation
//!   name resolves to `strat_litho` by exact name, else with Sodir's ` FM`
//!   suffix (`TARBERT` → `TARBERT FM`, `X (Y)` → `X FM (Y)`).
//! - `PLAY_HAS_FORMATION` (Play → Stratigraphy): the HC formations of the
//!   play's discoveries (the derived `Discovery IN_PLAY` links), restricted
//!   to the HC slots whose age matched the play; a published example records
//!   no slot and contributes every slot. `discovery_count` counts distinct
//!   discoveries.
//! - `ENCLOSES`: a structural element encloses a wellbore (its point) or a
//!   discovery (its whole polygon), and a play encloses a structural element
//!   (its whole polygon), as kglite's `contains()` decides.
//!
//! Sodir's structural-element layer repeats ids. A repeated id whose rows
//! share one code (`KODE`) is one element published in parts: the parts are
//! merged into one MULTIPOLYGON. An id shared by different codes names
//! different elements: the one at the highest level (lowest `LEVEL`) keeps
//! it, the others get a new negative id, and every element under id 0 (no
//! id) gets one. `STRUCTID_SOURCE` keeps the published id.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use geo::{BoundingRect, Contains, Geometry, MultiPolygon, Polygon, Rect};
use wkt::ToWkt;

use super::{DerivedReport, Table};
use crate::sodir::enhance::parse_geometry;
use crate::sodir::error::Result;

const SE_KEY: &str = "STRUCTID";
const HC_COLUMNS: [&str; 3] = [
    "wlbFormationWithHc1",
    "wlbFormationWithHc2",
    "wlbFormationWithHc3",
];

/// A structural element after de-duplication.
struct Element {
    id: String,
    row: Vec<String>,
    geometry: Option<Geometry<f64>>,
}

/// One element per (published id, code), merged and with unique ids.
fn structural_elements(table: &Table, report: &mut DerivedReport) -> Vec<Element> {
    let id = table.getter(SE_KEY);
    let code = table.getter("KODE");
    let level = table.getter("LEVEL");
    let wkt = table.getter("wkt_geometry");

    let mut order: Vec<(String, String)> = Vec::new();
    let mut groups: BTreeMap<(String, String), Vec<&Vec<String>>> = BTreeMap::new();
    for row in &table.rows {
        let key = (id.get(row).to_string(), code.get(row).to_string());
        let parts = groups.entry(key.clone()).or_default();
        if parts.is_empty() {
            order.push(key);
        }
        parts.push(row);
    }
    report.add(
        "structural_elements_parts_merged",
        table.rows.len() - groups.len(),
    );

    // Codes per published id; the keeper of a shared id.
    let mut codes: BTreeMap<&str, Vec<(&str, i64)>> = BTreeMap::new();
    for (sid, kode) in &order {
        let lvl = level
            .get(groups[&(sid.clone(), kode.clone())][0])
            .parse()
            .unwrap_or(i64::MAX);
        codes.entry(sid).or_default().push((kode, lvl));
    }
    let keeps = |sid: &str, kode: &str| -> bool {
        let entries = &codes[sid];
        if sid.trim() == "0" || sid.is_empty() {
            return false;
        }
        if entries.len() == 1 {
            return true;
        }
        let top = entries.iter().map(|e| e.1).min().unwrap_or(i64::MAX);
        let at_top: Vec<_> = entries.iter().filter(|e| e.1 == top).collect();
        at_top.len() == 1 && at_top[0].0 == kode
    };
    let mut reassigned: Vec<&(String, String)> = order
        .iter()
        .filter(|(sid, kode)| !keeps(sid, kode))
        .collect();
    reassigned.sort_by(|a, b| {
        let num = |s: &str| s.parse::<i64>().unwrap_or(i64::MAX);
        (num(&a.0), &a.1).cmp(&(num(&b.0), &b.1))
    });
    report.add("structural_elements_ids_reassigned", reassigned.len());
    let new_ids: BTreeMap<&(String, String), String> = reassigned
        .into_iter()
        .enumerate()
        .map(|(i, key)| (key, format!("-{}", i + 1)))
        .collect();

    order
        .iter()
        .map(|key| {
            let parts = &groups[key];
            let mut row = parts[0].clone();
            let geometry = merge(parts.iter().filter_map(|r| parse_geometry(wkt.get(r))));
            if let (Some(i), Some(g)) = (table.col("wkt_geometry"), &geometry) {
                if parts.len() > 1 {
                    row[i] = g.wkt_string();
                }
            }
            let id = new_ids.get(key).cloned().unwrap_or_else(|| key.0.clone());
            row.push(key.0.clone());
            Element { id, row, geometry }
        })
        .collect()
}

/// The parts of one element as a single geometry.
fn merge(parts: impl Iterator<Item = Geometry<f64>>) -> Option<Geometry<f64>> {
    let mut polygons: Vec<Polygon<f64>> = Vec::new();
    let mut count = 0;
    let mut single = None;
    for part in parts {
        count += 1;
        match part {
            Geometry::Polygon(p) => polygons.push(p),
            Geometry::MultiPolygon(mp) => polygons.extend(mp.0),
            other => single = Some(other),
        }
    }
    match (count, polygons.len()) {
        (0, _) => None,
        (_, 0) => single,
        (1, 1) => polygons.pop().map(Geometry::Polygon),
        _ => Some(Geometry::MultiPolygon(MultiPolygon(polygons))),
    }
}

fn read_elements(csv_dir: &Path, report: &mut DerivedReport) -> Result<(Table, Vec<Element>)> {
    let table = Table::read(&csv_dir.join("structural_elements.csv"))?;
    let elements = structural_elements(&table, report);
    Ok((table, elements))
}

/// `structural_elements.csv` with one row per element and unique ids.
pub(super) fn structural_elements_table(
    csv_dir: &Path,
    report: &mut DerivedReport,
) -> Result<Table> {
    let (table, elements) = read_elements(csv_dir, report)?;
    let mut out = Table {
        headers: table.headers.clone(),
        rows: Vec::new(),
    };
    out.headers.push("STRUCTID_SOURCE".to_string());
    let id_col = table.col(SE_KEY);
    for mut e in elements {
        if let Some(i) = id_col {
            e.row[i] = e.id;
        }
        out.push(e.row);
    }
    Ok(out)
}

/// A geometry with its bounding box, for a cheap rejection before
/// `contains`.
struct Shape {
    id: String,
    geometry: Geometry<f64>,
    bbox: Rect<f64>,
}

fn shape(id: String, geometry: Geometry<f64>) -> Option<Shape> {
    let bbox = geometry.bounding_rect()?;
    Some(Shape { id, geometry, bbox })
}

fn shapes(table: &Table, key: &str) -> Vec<Shape> {
    let id = table.getter(key);
    let wkt = table.getter("wkt_geometry");
    table
        .rows
        .iter()
        .filter_map(|row| shape(id.get(row).to_string(), parse_geometry(wkt.get(row))?))
        .collect()
}

fn bbox_within(outer: &Rect<f64>, inner: &Rect<f64>) -> bool {
    outer.min().x <= inner.min().x
        && outer.min().y <= inner.min().y
        && outer.max().x >= inner.max().x
        && outer.max().y >= inner.max().y
}

/// kglite's `contains(a, b)`: point-in-polygon, or full containment.
fn contains(outer: &Shape, inner: &Shape) -> bool {
    if !bbox_within(&outer.bbox, &inner.bbox) {
        return false;
    }
    match (&outer.geometry, &inner.geometry) {
        (Geometry::Polygon(a), Geometry::Point(b)) => a.contains(b),
        (Geometry::Polygon(a), Geometry::Polygon(b)) => a.contains(b),
        (Geometry::Polygon(a), Geometry::MultiPolygon(b)) => b.0.iter().all(|p| a.contains(p)),
        (Geometry::MultiPolygon(a), Geometry::Point(b)) => a.contains(b),
        (Geometry::MultiPolygon(a), Geometry::Polygon(b)) => a.contains(b),
        (Geometry::MultiPolygon(a), Geometry::MultiPolygon(b)) => a.contains(b),
        _ => false,
    }
}

fn element_shapes(elements: Vec<Element>) -> Vec<Shape> {
    elements
        .into_iter()
        .filter_map(|e| shape(e.id, e.geometry?))
        .collect()
}

/// `(StructuralElement)-[:ENCLOSES]->(Wellbore|Discovery)`.
pub(super) fn structural_encloses(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    let (_, elements) = read_elements(csv_dir, &mut DerivedReport::default())?;
    let elements = element_shapes(elements);
    let mut out = Table::new(&[SE_KEY, "target_type", "target_id"]);
    for (stem, key, label) in [
        ("wellbore", "wlbNpdidWellbore", "Wellbore"),
        ("discovery", "dscNpdidDiscovery", "Discovery"),
    ] {
        let path = csv_dir.join(format!("{stem}.csv"));
        if !path.is_file() {
            continue;
        }
        let targets = shapes(&Table::read(&path)?, key);
        let mut n = 0;
        for element in &elements {
            for target in &targets {
                if contains(element, target) {
                    out.push(vec![
                        element.id.clone(),
                        label.to_string(),
                        target.id.clone(),
                    ]);
                    n += 1;
                }
            }
        }
        report.add(&format!("structural_encloses_{stem}"), n);
    }
    Ok(out)
}

/// `(Play)-[:ENCLOSES]->(StructuralElement)`.
pub(super) fn play_encloses(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    let plays = shapes(&Table::read(&csv_dir.join("play.csv"))?, "plyNPDID");
    let mut out = Table::new(&["plyNPDID", SE_KEY]);
    let path = csv_dir.join("structural_elements.csv");
    if path.is_file() {
        let elements = element_shapes(structural_elements(
            &Table::read(&path)?,
            &mut DerivedReport::default(),
        ));
        for play in &plays {
            for element in &elements {
                if contains(play, element) {
                    out.push(vec![play.id.clone(), element.id.clone()]);
                }
            }
        }
    }
    report.add("play_encloses_structural", out.rows.len());
    Ok(out)
}

/// Stratigraphy name → id, with the ` FM` spellings a wellbore uses.
struct Formations(BTreeMap<String, String>);

impl Formations {
    fn read(csv_dir: &Path) -> Result<Self> {
        let path = csv_dir.join("strat_litho.csv");
        if !path.is_file() {
            return Ok(Self(BTreeMap::new()));
        }
        let table = Table::read(&path)?;
        let (id, name) = (table.getter("lsuNpdidLithoStrat"), table.getter("lsuName"));
        Ok(Self(
            table
                .rows
                .iter()
                .map(|r| (name.get(r).to_uppercase(), id.get(r).to_string()))
                .collect(),
        ))
    }

    fn resolve(&self, name: &str) -> Option<&str> {
        let name = name.trim().to_uppercase();
        if name.is_empty() {
            return None;
        }
        let candidates = [
            name.clone(),
            format!("{name} FM"),
            name.replacen(" (", " FM (", 1),
        ];
        candidates
            .iter()
            .find_map(|c| self.0.get(c).map(String::as_str))
    }
}

/// Each discovery's HC formations: `(discovery, [(slot 1..3, strat id)])`.
fn discovery_formations(
    csv_dir: &Path,
    formations: &Formations,
) -> Result<BTreeMap<String, Vec<(usize, String)>>> {
    let discoveries = Table::read(&csv_dir.join("discovery.csv"))?;
    let wells_path = csv_dir.join("wellbore.csv");
    if !wells_path.is_file() {
        return Ok(BTreeMap::new());
    }
    let wells = Table::read(&wells_path)?;
    let (wid, wname) = (
        wells.getter("wlbNpdidWellbore"),
        wells.getter("wlbWellboreName"),
    );
    let hc: Vec<_> = HC_COLUMNS.iter().map(|c| wells.getter(c)).collect();
    let mut by_id = BTreeMap::new();
    let mut by_name = BTreeMap::new();
    for row in &wells.rows {
        by_id.insert(wid.get(row), row);
        by_name.insert(wname.get(row), row);
    }
    let (did, dwell, dwname) = (
        discoveries.getter("dscNpdidDiscovery"),
        discoveries.getter("wlbNpdidWellbore"),
        discoveries.getter("wlbName"),
    );
    let mut out: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    for row in &discoveries.rows {
        let Some(well) = by_id
            .get(dwell.get(row))
            .or_else(|| by_name.get(dwname.get(row)))
        else {
            continue;
        };
        let links = out.entry(did.get(row).to_string()).or_default();
        for (slot, col) in hc.iter().enumerate() {
            if let Some(strat) = formations.resolve(col.get(well)) {
                if !links.iter().any(|(_, s)| s == strat) {
                    links.push((slot + 1, strat.to_string()));
                }
            }
        }
    }
    Ok(out)
}

/// `(Discovery)-[:HC_IN_FORMATION {hc_rank}]->(Stratigraphy)`.
pub(super) fn hc_in_formation(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    let formations = Formations::read(csv_dir)?;
    let mut out = Table::new(&["dscNpdidDiscovery", "lsuNpdidLithoStrat", "hc_rank"]);
    for (discovery, links) in discovery_formations(csv_dir, &formations)? {
        for (rank, strat) in links {
            out.push(vec![discovery.clone(), strat, rank.to_string()]);
        }
    }
    report.add("hc_in_formation", out.rows.len());
    Ok(out)
}

/// `(Play)-[:PLAY_HAS_FORMATION {discovery_count}]->(Stratigraphy)`.
pub(super) fn play_has_formation(csv_dir: &Path, report: &mut DerivedReport) -> Result<Table> {
    let mut out = Table::new(&["plyNPDID", "lsuNpdidLithoStrat", "discovery_count"]);
    let links_path = csv_dir.join("_derived_discovery_play.csv");
    if !links_path.is_file() {
        return Ok(out);
    }
    let formations = Formations::read(csv_dir)?;
    let by_discovery = discovery_formations(csv_dir, &formations)?;
    let links = Table::read(&links_path)?;
    let (did, pid, slots) = (
        links.getter("dscNpdidDiscovery"),
        links.getter("plyNPDID"),
        links.getter("matched_hc_slots"),
    );
    let mut counts: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for row in &links.rows {
        let Some(hc) = by_discovery.get(did.get(row)) else {
            continue;
        };
        let matched: Vec<usize> = serde_json::from_str(slots.get(row)).unwrap_or_default();
        for (slot, strat) in hc {
            if matched.is_empty() || matched.contains(slot) {
                counts
                    .entry((pid.get(row).to_string(), strat.clone()))
                    .or_default()
                    .insert(did.get(row).to_string());
            }
        }
    }
    for ((play, strat), discoveries) in counts {
        out.push(vec![play, strat, discoveries.len().to_string()]);
    }
    report.add("play_has_formation", out.rows.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(csv: &str) -> Table {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.csv");
        std::fs::write(&path, csv).unwrap();
        Table::read(&path).unwrap()
    }

    #[test]
    fn repeated_ids_merge_parts_and_split_distinct_elements() {
        let t = table(
            "STRUCTID,KODE,LEVEL,wkt_geometry\n\
             5,A,3,\"POLYGON((0 0,1 0,1 1,0 1,0 0))\"\n\
             5,A,3,\"POLYGON((5 0,6 0,6 1,5 1,5 0))\"\n\
             24,P,2,\"POLYGON((0 0,1 0,1 1,0 1,0 0))\"\n\
             24,C,3,\"POLYGON((0 0,1 0,1 1,0 1,0 0))\"\n\
             0,X,3,\n0,Y,3,\n",
        );
        let mut report = DerivedReport::default();
        let elements = structural_elements(&t, &mut report);
        let ids: Vec<_> = elements.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["5", "24", "-3", "-1", "-2"]);
        assert!(matches!(
            elements[0].geometry,
            Some(Geometry::MultiPolygon(ref mp)) if mp.0.len() == 2
        ));
        assert!(elements[0].row[3].starts_with("MULTIPOLYGON"));
        assert_eq!(report.counts["structural_elements_parts_merged"], 1);
        assert_eq!(report.counts["structural_elements_ids_reassigned"], 3);
    }

    #[test]
    fn formation_names_resolve_with_the_fm_suffix() {
        let f = Formations(
            [
                ("BRENT GP".to_string(), "1".to_string()),
                ("TARBERT FM".to_string(), "2".to_string()),
                ("NORDLAND GP FM (INFORMAL)".to_string(), "3".to_string()),
            ]
            .into(),
        );
        assert_eq!(f.resolve("Brent Gp"), Some("1"));
        assert_eq!(f.resolve("TARBERT"), Some("2"));
        assert_eq!(f.resolve("NORDLAND GP (INFORMAL)"), Some("3"));
        assert_eq!(f.resolve("EGGA (INFORMAL)"), None);
        assert_eq!(f.resolve(""), None);
    }
}
