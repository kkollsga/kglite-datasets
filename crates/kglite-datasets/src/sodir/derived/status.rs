//! Status timelines: `(Wellbore)-[:HAS_STATUS]->(:WellStatus)` from the
//! wellbore's dated lifecycle events, and
//! `(Facility|Pipeline)-[:HAS_STATUS]->(:FacilityStatus)`.
//!
//! Each dated event opens a period that lasts until the next event
//! (half-open; the last period is open). Sodir publishes the current status
//! but not the date it took effect, so a current status that differs from the
//! last dated event closes the timeline with `basis: inferred-start`: it
//! replaces a final COMPLETED period (a producing well is producing from
//! completion), otherwise it starts on the last event's date. JUNKED and
//! BLOWOUT are outcomes, not lifecycle stages: they go to `wlbOutcome` and
//! never enter the timeline.
//!
//! A facility is IN SERVICE from startup, SHUT DOWN and REMOVED from the
//! FactMaps `fclDateShutdown` / `fclDateRemoved` columns, closed by its
//! current `fclPhase` under the same rule. A startup date on a facility that
//! is not yet in service (FUTURE, FABRICATION, INSTALLATION) is a plan, not
//! an event. Sodir writes 1900-01-01 (and a few older dates) for an unknown
//! date; an event before 1960, before any activity on the shelf, is ignored.
//! Sodir publishes no pipeline history: a pipeline has one period,
//! its current phase from `pplCurrentPhaseFromDate`.

use chrono::NaiveDate;

use super::{date, iso, DerivedReport, Table};
use crate::sodir::error::Result;

/// Wellbore lifecycle events in the order they happen on a normal well.
const WELL_EVENTS: &[(&str, &str)] = &[
    ("wlbDrillPerApprovedPermitDate", "PERMITTED"),
    ("wlbEntryPreDrillDate", "PREDRILLING"),
    ("wlbCompPreDrillDate", "PREDRILLED"),
    ("wlbEntryDate", "DRILLING"),
    ("wlbCompletionDate", "COMPLETED"),
    // Labelled with the well's actual RE-CLASS status; see `timeline`.
    (RECLASS_COL, "RE-CLASS"),
    ("wlbPluggedDate", "PLUGGED"),
    ("wlbPluggedAbandonDate", "P&A"),
];
const RECLASS_COL: &str = "wlbDateReclass";

/// `wlbStatus` values that are outcomes rather than lifecycle stages.
const WELL_OUTCOMES: &[&str] = &["JUNKED", "BLOWOUT"];

/// The `WellStatus` nodes, in lifecycle order (`phase_order` = position).
const WELL_STATUS_ORDER: &[&str] = &[
    "PERMITTED",
    "PREDRILLING",
    "PREDRILLED",
    "DRILLING",
    "COMPLETED",
    "RE-CLASS TO DEV",
    "RE-CLASS TO TEST",
    "ONLINE/OPERATIONAL",
    "PRODUCING",
    "INJECTING",
    "PRODUCING/INJECTING",
    "SUSPENDED",
    "CLOSED",
    "PLUGGED",
    "P&A",
    "WILL NEVER BE DRILLED",
];

const FACILITY_EVENTS: &[(&str, &str)] = &[
    ("fclStartupDate", "IN SERVICE"),
    ("fclDateShutdown", "SHUT DOWN"),
    ("fclDateRemoved", "REMOVED"),
];
/// Facility phases that precede service.
const FACILITY_PRE_SERVICE: &[&str] = &["FUTURE", "FABRICATION", "INSTALLATION"];

/// Event dates before this are Sodir's "unknown" sentinels, not events.
const EARLIEST_EVENT: NaiveDate = match NaiveDate::from_ymd_opt(1960, 1, 1) {
    Some(d) => d,
    None => panic!("valid date"),
};

/// A lifecycle event date, or `None` for a missing or sentinel date.
fn event_date(cell: &str) -> Option<NaiveDate> {
    date(cell).filter(|d| *d >= EARLIEST_EVENT)
}

const WELL_KEY: &str = "wlbNpdidWellbore";
const FACILITY_KEY: &str = "fclNpdidFacility";
const PIPELINE_KEY: &str = "pplNpdidPipeline";
const BASIS_REPORTED: &str = "reported";
const BASIS_INFERRED: &str = "inferred-start";

/// One half-open status period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Period {
    pub status: String,
    pub from: NaiveDate,
    pub to: Option<NaiveDate>,
    pub basis: &'static str,
}

/// The status periods of one entity. `current` is its current status, if it
/// is a lifecycle status; `reclass` names the event column whose label is
/// the current status (only when that status is a RE-CLASS).
fn timeline(
    events: &[(Option<NaiveDate>, &str)],
    current: Option<&str>,
    reclass: Option<usize>,
) -> Vec<Period> {
    let mut dated: Vec<(NaiveDate, String, &'static str)> = Vec::new();
    for (i, (day, label)) in events.iter().enumerate() {
        let Some(day) = day else { continue };
        let label = if Some(i) == reclass {
            match current {
                Some(c) if c.starts_with("RE-CLASS") => c.to_string(),
                _ => continue,
            }
        } else {
            label.to_string()
        };
        dated.push((*day, label, BASIS_REPORTED));
    }
    // Stable: events on the same day keep their lifecycle order.
    dated.sort_by_key(|e| e.0);
    if let (Some(last), Some(cur)) = (dated.last().cloned(), current) {
        let reclass_already = cur.starts_with("RE-CLASS") && dated.iter().any(|e| e.1 == cur);
        if last.1 != cur && !reclass_already {
            if last.1 == "COMPLETED" {
                let n = dated.len();
                dated[n - 1] = (last.0, cur.to_string(), BASIS_INFERRED);
            } else {
                dated.push((last.0, cur.to_string(), BASIS_INFERRED));
            }
        }
    }
    let mut periods = Vec::with_capacity(dated.len());
    for (i, (from, status, basis)) in dated.iter().enumerate() {
        periods.push(Period {
            status: status.clone(),
            from: *from,
            to: dated.get(i + 1).map(|e| e.0),
            basis,
        });
    }
    periods
}

fn outcome(status: &str) -> Option<&str> {
    WELL_OUTCOMES.contains(&status).then_some(status)
}

/// Every wellbore's status periods, keyed by its NPDID, in source order.
pub(crate) fn well_timelines(wells: &Table) -> Vec<(String, Vec<Period>)> {
    let key = wells.getter(WELL_KEY);
    let status = wells.getter("wlbStatus");
    let cols: Vec<_> = WELL_EVENTS.iter().map(|(c, _)| wells.getter(c)).collect();
    let reclass = WELL_EVENTS.iter().position(|(c, _)| *c == RECLASS_COL);
    wells
        .rows
        .iter()
        .map(|row| {
            let events: Vec<_> = cols
                .iter()
                .zip(WELL_EVENTS)
                .map(|(col, (_, label))| (event_date(col.get(row)), *label))
                .collect();
            let cur = status.get(row);
            let cur = (!cur.is_empty() && outcome(cur).is_none()).then_some(cur);
            (key.get(row).to_string(), timeline(&events, cur, reclass))
        })
        .collect()
}

fn read_wells(csv_dir: &std::path::Path) -> Result<Table> {
    Table::read(&csv_dir.join("wellbore.csv"))
}

/// `wellbore.csv` plus `wlbOutcome` (JUNKED / BLOWOUT, else empty).
pub(super) fn wellbore(csv_dir: &std::path::Path, report: &mut DerivedReport) -> Result<Table> {
    let mut wells = read_wells(csv_dir)?;
    let status = wells.getter("wlbStatus");
    let outcomes: Vec<String> = wells
        .rows
        .iter()
        .map(|row| outcome(status.get(row)).unwrap_or_default().to_string())
        .collect();
    report.add(
        "wellbore_outcomes",
        outcomes.iter().filter(|o| !o.is_empty()).count(),
    );
    wells.set_column("wlbOutcome", outcomes);
    Ok(wells)
}

/// The `HAS_STATUS` junction: one row per period.
pub(super) fn wellbore_status_hst(
    csv_dir: &std::path::Path,
    report: &mut DerivedReport,
) -> Result<Table> {
    let wells = read_wells(csv_dir)?;
    let mut out = Table::new(&[WELL_KEY, "status", "validFrom", "validTo", "basis"]);
    let mut with_status = 0;
    for (id, periods) in well_timelines(&wells) {
        with_status += usize::from(!periods.is_empty());
        for p in periods {
            out.push(vec![
                id.clone(),
                p.status,
                iso(Some(p.from)),
                iso(p.to),
                p.basis.to_string(),
            ]);
        }
    }
    report.add("well_status_periods", out.rows.len());
    report.add("well_status_wells", with_status);
    Ok(out)
}

/// The `WellStatus` nodes: the lifecycle list, plus any other status the
/// timelines use (no `phase_order`).
pub(super) fn well_status(csv_dir: &std::path::Path, _: &mut DerivedReport) -> Result<Table> {
    let wells = read_wells(csv_dir)?;
    let mut extra: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (_, periods) in well_timelines(&wells) {
        for p in periods {
            if !WELL_STATUS_ORDER.contains(&p.status.as_str()) {
                extra.insert(p.status);
            }
        }
    }
    Ok(status_nodes(WELL_STATUS_ORDER, extra))
}

fn status_nodes(order: &[&str], extra: std::collections::BTreeSet<String>) -> Table {
    let mut out = Table::new(&["name", "phase_order"]);
    for (i, name) in order.iter().enumerate() {
        out.push(vec![name.to_string(), (i + 1).to_string()]);
    }
    for name in extra {
        out.push(vec![name, String::new()]);
    }
    out
}

/// Every facility's status periods, keyed by its NPDID, in source order.
fn facility_timelines(facilities: &Table) -> Vec<(String, Vec<Period>)> {
    let key = facilities.getter(FACILITY_KEY);
    let phase = facilities.getter("fclPhase");
    let cols: Vec<_> = FACILITY_EVENTS
        .iter()
        .map(|(c, _)| facilities.getter(c))
        .collect();
    facilities
        .rows
        .iter()
        .map(|row| {
            let cur = phase.get(row);
            let planned = FACILITY_PRE_SERVICE.contains(&cur);
            let events: Vec<_> = cols
                .iter()
                .zip(FACILITY_EVENTS)
                .enumerate()
                .map(|(i, (col, (_, label)))| {
                    let day = if i == 0 && planned {
                        None
                    } else {
                        event_date(col.get(row))
                    };
                    (day, *label)
                })
                .collect();
            let cur = (!cur.is_empty()).then_some(cur);
            (key.get(row).to_string(), timeline(&events, cur, None))
        })
        .collect()
}

fn pipeline_periods(pipelines: &Table) -> Vec<(String, Period)> {
    let key = pipelines.getter(PIPELINE_KEY);
    let phase = pipelines.getter("pplCurrentPhase");
    let from = pipelines.getter("pplCurrentPhaseFromDate");
    pipelines
        .rows
        .iter()
        .filter_map(|row| {
            let status = phase.get(row);
            let from = event_date(from.get(row))?;
            (!status.is_empty()).then(|| {
                (
                    key.get(row).to_string(),
                    Period {
                        status: status.to_string(),
                        from,
                        to: None,
                        basis: BASIS_REPORTED,
                    },
                )
            })
        })
        .collect()
}

fn period_table(key: &str, periods: impl IntoIterator<Item = (String, Period)>) -> Table {
    let mut out = Table::new(&[key, "status", "validFrom", "validTo", "basis"]);
    for (id, p) in periods {
        out.push(vec![
            id,
            p.status,
            iso(Some(p.from)),
            iso(p.to),
            p.basis.to_string(),
        ]);
    }
    out
}

/// Facility `HAS_STATUS` junction.
pub(super) fn facility_status_hst(
    csv_dir: &std::path::Path,
    report: &mut DerivedReport,
) -> Result<Table> {
    let facilities = Table::read(&csv_dir.join("facility.csv"))?;
    let periods = facility_timelines(&facilities)
        .into_iter()
        .flat_map(|(id, ps)| ps.into_iter().map(move |p| (id.clone(), p)));
    let out = period_table(FACILITY_KEY, periods);
    report.add("facility_status_periods", out.rows.len());
    Ok(out)
}

/// Pipeline `HAS_STATUS` junction.
pub(super) fn pipeline_status_hst(
    csv_dir: &std::path::Path,
    report: &mut DerivedReport,
) -> Result<Table> {
    let pipelines = Table::read(&csv_dir.join("pipeline.csv"))?;
    let out = period_table(PIPELINE_KEY, pipeline_periods(&pipelines));
    report.add("pipeline_status_periods", out.rows.len());
    Ok(out)
}

/// The `FacilityStatus` nodes: every status a facility or pipeline period
/// uses, sorted.
pub(super) fn facility_status(csv_dir: &std::path::Path, _: &mut DerivedReport) -> Result<Table> {
    let mut names = std::collections::BTreeSet::new();
    let facilities = csv_dir.join("facility.csv");
    if facilities.is_file() {
        for (_, periods) in facility_timelines(&Table::read(&facilities)?) {
            names.extend(periods.into_iter().map(|p| p.status));
        }
    }
    let pipelines = csv_dir.join("pipeline.csv");
    if pipelines.is_file() {
        names.extend(
            pipeline_periods(&Table::read(&pipelines)?)
                .into_iter()
                .map(|(_, p)| p.status),
        );
    }
    let mut out = Table::new(&["name"]);
    for name in names {
        out.push(vec![name]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn wells(rows: &str) -> Table {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("wellbore.csv");
        let header =
            "wlbNpdidWellbore,wlbStatus,wlbDrillPerApprovedPermitDate,wlbEntryPreDrillDate,\
            wlbCompPreDrillDate,wlbEntryDate,wlbCompletionDate,wlbDateReclass,wlbPluggedDate,\
            wlbPluggedAbandonDate\n";
        std::fs::write(&path, format!("{header}{rows}")).unwrap();
        Table::read(&path).unwrap()
    }

    fn summary(periods: &[Period]) -> Vec<(String, String, String, &str)> {
        periods
            .iter()
            .map(|p| (p.status.clone(), iso(Some(p.from)), iso(p.to), p.basis))
            .collect()
    }

    #[test]
    fn reported_events_chain_half_open() {
        let t = well_timelines(&wells(
            "1,P&A,,,,1181779200000,1188172800000,,,1476921600000\n",
        ));
        assert_eq!(
            summary(&t[0].1),
            [
                (
                    "DRILLING".into(),
                    "2007-06-14".into(),
                    "2007-08-27".into(),
                    "reported"
                ),
                (
                    "COMPLETED".into(),
                    "2007-08-27".into(),
                    "2016-10-20".into(),
                    "reported"
                ),
                ("P&A".into(), "2016-10-20".into(), String::new(), "reported"),
            ]
        );
    }

    #[test]
    fn current_status_replaces_completed_or_follows_the_last_event() {
        let t = well_timelines(&wells(
            "1,PRODUCING,,,,2010-01-01,2010-03-01,,,\n\
             2,SUSPENDED,,,,2011-01-01,2011-02-01,,2015-05-05,\n",
        ));
        assert_eq!(
            summary(&t[0].1)[1],
            (
                "PRODUCING".into(),
                "2010-03-01".into(),
                String::new(),
                BASIS_INFERRED
            )
        );
        let s = summary(&t[1].1);
        assert_eq!(s.len(), 4);
        assert_eq!(
            s[2],
            (
                "PLUGGED".into(),
                "2015-05-05".into(),
                "2015-05-05".into(),
                "reported"
            )
        );
        assert_eq!(
            s[3],
            (
                "SUSPENDED".into(),
                "2015-05-05".into(),
                String::new(),
                BASIS_INFERRED
            )
        );
    }

    #[test]
    fn reclass_date_is_labelled_with_the_reclass_status_only() {
        let t = well_timelines(&wells(
            "1,RE-CLASS TO TEST,,,,2013-01-01,2013-02-01,2013-06-01,,\n\
             2,P&A,,,,2013-01-01,2013-02-01,2013-06-01,,2014-01-01\n",
        ));
        let first: Vec<_> = t[0].1.iter().map(|p| p.status.as_str()).collect();
        assert_eq!(first, ["DRILLING", "COMPLETED", "RE-CLASS TO TEST"]);
        let second: Vec<_> = t[1].1.iter().map(|p| p.status.as_str()).collect();
        assert_eq!(second, ["DRILLING", "COMPLETED", "P&A"]);
        assert_eq!(t[0].1[2].from, d("2013-06-01"));
    }

    #[test]
    fn outcomes_and_undated_wells() {
        let t = well_timelines(&wells(
            "1,JUNKED,,,,2012-01-01,2012-01-20,,,\n2,WILL NEVER BE DRILLED,,,,,,,,\n3,,,,,2012-01-01,,,,\n",
        ));
        let junked: Vec<_> = t[0].1.iter().map(|p| p.status.as_str()).collect();
        assert_eq!(junked, ["DRILLING", "COMPLETED"]);
        assert!(t[1].1.is_empty());
        assert_eq!(summary(&t[2].1).len(), 1);
    }

    #[test]
    fn facility_events_skip_planned_startups_and_sentinels() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("facility.csv");
        std::fs::write(
            &path,
            "fclNpdidFacility,fclPhase,fclStartupDate,fclDateShutdown,fclDateRemoved\n\
             1,REMOVED,1980-01-01,1999-01-01,2005-06-01\n\
             2,FUTURE,2030-01-01,,\n\
             3,SHUT DOWN,1990-01-01,-2208988800000,\n",
        )
        .unwrap();
        let t = facility_timelines(&Table::read(&path).unwrap());
        let names = |ps: &[Period]| ps.iter().map(|p| p.status.clone()).collect::<Vec<_>>();
        assert_eq!(names(&t[0].1), ["IN SERVICE", "SHUT DOWN", "REMOVED"]);
        assert!(t[1].1.is_empty());
        assert_eq!(
            summary(&t[2].1),
            [
                (
                    "IN SERVICE".into(),
                    "1990-01-01".into(),
                    "1990-01-01".into(),
                    "reported"
                ),
                (
                    "SHUT DOWN".into(),
                    "1990-01-01".into(),
                    String::new(),
                    BASIS_INFERRED
                ),
            ]
        );
    }

    #[test]
    fn unknown_current_statuses_become_unordered_nodes() {
        let nodes = status_nodes(WELL_STATUS_ORDER, ["NEW STATUS".to_string()].into());
        assert_eq!(nodes.rows[14], ["P&A", "15"]);
        assert_eq!(nodes.rows.last().unwrap(), &["NEW STATUS", ""]);
    }
}
