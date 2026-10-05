//! Derived tables the packaged blueprint reads: modelled layers Sodir does
//! not publish as such (status timelines, reserve version chains, enrichment
//! junctions) and augmented copies of source tables.
//!
//! Each output is a `_derived_<name>.csv` in the CSV directory, built from the
//! cached source CSVs it names. Only the outputs a blueprint references are
//! built, so a custom blueprint pays for nothing it does not read. The source
//! CSVs are never modified. An output whose primary source is missing is
//! removed rather than left stale.
//!
//! Dates are written as `YYYY-MM-DD`. Half-open windows (`validTo`,
//! `existsTo`) name the first day the row is no longer valid.

mod reserves;
mod status;

use std::collections::BTreeMap;
use std::path::Path;

use chrono::NaiveDate;

use crate::sodir::error::Result;
use crate::sodir::preprocess::{read_csv, write_csv};

/// Counts per derived output, keyed `<output>_<what>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DerivedReport {
    pub counts: BTreeMap<String, usize>,
}

impl DerivedReport {
    fn add(&mut self, key: &str, n: usize) {
        *self.counts.entry(key.to_string()).or_default() += n;
    }
}

/// One derived output: its stem, the source stems it reads (the first is
/// required), and the builder.
struct Derived {
    stem: &'static str,
    sources: &'static [&'static str],
    build: fn(&Path, &mut DerivedReport) -> Result<Table>,
}

const OUTPUTS: &[Derived] = &[
    Derived {
        stem: "_derived_wellbore",
        sources: &["wellbore"],
        build: status::wellbore,
    },
    Derived {
        stem: "_derived_wellbore_status_hst",
        sources: &["wellbore"],
        build: status::wellbore_status_hst,
    },
    Derived {
        stem: "_derived_well_status",
        sources: &["wellbore"],
        build: status::well_status,
    },
    Derived {
        stem: "_derived_facility_status_hst",
        sources: &["facility"],
        build: status::facility_status_hst,
    },
    Derived {
        stem: "_derived_pipeline_status_hst",
        sources: &["pipeline"],
        build: status::pipeline_status_hst,
    },
    Derived {
        stem: "_derived_facility_status",
        sources: &["facility", "pipeline"],
        build: status::facility_status,
    },
    Derived {
        stem: "_derived_field_reserves",
        sources: &["field_reserves"],
        build: reserves::field_reserves,
    },
    Derived {
        stem: "_derived_discovery_reserves",
        sources: &["discovery_reserves"],
        build: reserves::discovery_reserves,
    },
    Derived {
        stem: "_derived_field_reserves_company",
        sources: &["field_reserves_company"],
        build: reserves::field_reserves_company,
    },
];

/// The source stems behind a derived output, or `None` for any other stem.
pub fn sources_for(stem: &str) -> Option<&'static [&'static str]> {
    OUTPUTS.iter().find(|d| d.stem == stem).map(|d| d.sources)
}

/// The registered derived stem equal to `stem`, if any.
pub fn known(stem: &str) -> Option<&'static str> {
    OUTPUTS.iter().find(|d| d.stem == stem).map(|d| d.stem)
}

/// Build each `targets` output in registry order.
pub fn apply(csv_dir: &Path, targets: &[&str]) -> Result<DerivedReport> {
    let mut report = DerivedReport::default();
    for output in OUTPUTS.iter().filter(|d| targets.contains(&d.stem)) {
        let path = csv_dir.join(format!("{}.csv", output.stem));
        if !csv_dir.join(format!("{}.csv", output.sources[0])).is_file() {
            if path.is_file() {
                std::fs::remove_file(&path)?;
            }
            continue;
        }
        let table = (output.build)(csv_dir, &mut report)?;
        table.write(&path)?;
    }
    Ok(report)
}

/// A CSV held in memory with name-based cell access.
#[derive(Debug, Clone, Default)]
pub(crate) struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Self {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let (headers, rows) = read_csv(path)?;
        Ok(Self { headers, rows })
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        write_csv(path, &self.headers, &self.rows)
    }

    pub fn col(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == name)
    }

    /// Cell accessor for `name`; a missing column reads as empty.
    pub fn getter(&self, name: &str) -> Col {
        Col(self.col(name))
    }

    /// Overwrite a column's cells, or append it.
    pub fn set_column(&mut self, name: &str, values: Vec<String>) {
        match self.col(name) {
            Some(i) => {
                for (row, v) in self.rows.iter_mut().zip(values) {
                    row[i] = v;
                }
            }
            None => {
                self.headers.push(name.to_string());
                for (row, v) in self.rows.iter_mut().zip(values) {
                    row.push(v);
                }
            }
        }
    }

    pub fn push(&mut self, row: Vec<String>) {
        debug_assert_eq!(row.len(), self.headers.len());
        self.rows.push(row);
    }
}

/// A column position resolved once, read per row.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Col(Option<usize>);

impl Col {
    pub fn get<'r>(&self, row: &'r [String]) -> &'r str {
        self.0.and_then(|i| row.get(i)).map_or("", |s| s.trim())
    }
}

/// A source date as kglite reads it, or `None`.
pub(crate) fn date(cell: &str) -> Option<NaiveDate> {
    crate::sodir::temporal::parse_date(cell)
}

pub(crate) fn iso(d: Option<NaiveDate>) -> String {
    d.map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
