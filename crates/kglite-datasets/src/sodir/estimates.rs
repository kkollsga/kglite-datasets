//! Curated, typed resource estimates published for named discoveries.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateScope {
    WholeDiscovery,
    AppraisedSegment,
    Field,
    Potential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    MillionSm3OilEquivalent,
    MillionSm3Oil,
    BillionSm3Gas,
    MillionTonnesNgl,
    MillionSm3Condensate,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EstimateValue {
    pub unit: Unit,
    pub point: Option<f64>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub preliminary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PublishedDiscoveryEstimate {
    pub discovery_id: u64,
    pub discovery_name: &'static str,
    pub estimate_date: &'static str,
    pub scope: EstimateScope,
    pub source_well_ids: &'static [u64],
    pub recoverable_oil: Option<EstimateValue>,
    pub recoverable_gas: Option<EstimateValue>,
    pub recoverable_ngl: Option<EstimateValue>,
    pub recoverable_condensate: Option<EstimateValue>,
    pub recoverable_oe: Option<EstimateValue>,
    pub source_url: &'static str,
    pub accessed_on: &'static str,
}

const GJOA_NORD_URL: &str = "https://www.sodir.no/en/whats-new/news/exploration-drilling-results/2022/delineation-of-oil-and-gas-discovery-near-the-gjoa-field-in-the-north-sea--359-16-s-and-359-16-a/";
const GJENGALUNDEN_URL: &str = "https://www.sodir.no/aktuelt/nyheter/resultat-av-leteboring/2023/oljefunn-nar-ivar-aasen-feltet-i-nordsjoen/";
const ROVER_SOR_URL: &str = "https://www.sodir.no/en/whats-new/news/Exploration-drilling-results/2023/oil-and-gas-discovery-near-the-troll-field-in-the-north-sea/";

pub const PUBLISHED_DISCOVERY_ESTIMATES: &[PublishedDiscoveryEstimate] = &[
    PublishedDiscoveryEstimate {
        discovery_id: 45651,
        discovery_name: "35/9-3 (Gjøa Nord)",
        estimate_date: "2022-05-12",
        scope: EstimateScope::WholeDiscovery,
        source_well_ids: &[9530, 9565],
        recoverable_oil: None,
        recoverable_gas: None,
        recoverable_ngl: None,
        recoverable_condensate: None,
        recoverable_oe: Some(EstimateValue {
            unit: Unit::MillionSm3OilEquivalent,
            point: None,
            minimum: Some(2.2),
            maximum: Some(3.4),
            preliminary: true,
        }),
        source_url: GJOA_NORD_URL,
        accessed_on: "2026-09-08",
    },
    PublishedDiscoveryEstimate {
        discovery_id: 42148344,
        discovery_name: "25/10-17 S",
        estimate_date: "2023-02-10",
        scope: EstimateScope::WholeDiscovery,
        source_well_ids: &[9717],
        recoverable_oil: None,
        recoverable_gas: None,
        recoverable_ngl: None,
        recoverable_condensate: None,
        recoverable_oe: Some(EstimateValue {
            unit: Unit::MillionSm3OilEquivalent,
            point: None,
            minimum: Some(0.5),
            maximum: Some(1.4),
            preliminary: true,
        }),
        source_url: GJENGALUNDEN_URL,
        accessed_on: "2026-09-08",
    },
    PublishedDiscoveryEstimate {
        discovery_id: 42148093,
        discovery_name: "31/1-3 S (Røver Sør)",
        estimate_date: "2023-02-09",
        scope: EstimateScope::WholeDiscovery,
        source_well_ids: &[9663, 9664],
        recoverable_oil: None,
        recoverable_gas: None,
        recoverable_ngl: None,
        recoverable_condensate: None,
        recoverable_oe: Some(EstimateValue {
            unit: Unit::MillionSm3OilEquivalent,
            point: None,
            minimum: Some(2.7),
            maximum: Some(7.4),
            preliminary: true,
        }),
        source_url: ROVER_SOR_URL,
        accessed_on: "2026-09-08",
    },
];

pub fn estimates_for_discovery(
    discovery_name: &str,
    discovery_id: u64,
) -> impl Iterator<Item = &'static PublishedDiscoveryEstimate> + '_ {
    PUBLISHED_DISCOVERY_ESTIMATES
        .iter()
        .filter(move |estimate| {
            estimate.discovery_name == discovery_name && estimate.discovery_id == discovery_id
        })
}

pub fn newest_applicable_estimate(
    discovery_name: &str,
    discovery_id: u64,
) -> Option<&'static PublishedDiscoveryEstimate> {
    newest_applicable(estimates_for_discovery(discovery_name, discovery_id))
}

fn newest_applicable<'a>(
    estimates: impl Iterator<Item = &'a PublishedDiscoveryEstimate>,
) -> Option<&'a PublishedDiscoveryEstimate> {
    estimates
        .filter(|estimate| estimate.scope == EstimateScope::WholeDiscovery)
        .filter(|estimate| estimate_values(estimate).any(valid_value))
        .max_by_key(|estimate| (estimate.estimate_date, estimate.source_url))
}

fn estimate_values(
    estimate: &PublishedDiscoveryEstimate,
) -> impl Iterator<Item = EstimateValue> + '_ {
    [
        estimate.recoverable_oil,
        estimate.recoverable_gas,
        estimate.recoverable_ngl,
        estimate.recoverable_condensate,
        estimate.recoverable_oe,
    ]
    .into_iter()
    .flatten()
}

fn valid_value(value: EstimateValue) -> bool {
    value.point.is_some_and(f64::is_finite)
        || matches!((value.minimum, value.maximum), (Some(min), Some(max)) if min.is_finite() && max.is_finite() && min <= max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gjoa_nord_is_an_exact_whole_discovery_range() {
        let estimate = newest_applicable_estimate("35/9-3 (Gjøa Nord)", 45651).unwrap();
        assert_eq!(estimate.source_well_ids, &[9530, 9565]);
        assert_eq!(estimate.recoverable_oe.unwrap().minimum, Some(2.2));
        assert_eq!(estimate.recoverable_oe.unwrap().maximum, Some(3.4));
        assert!(estimate.recoverable_oil.is_none());
        assert!(newest_applicable_estimate("35/9-3", 45651).is_none());
    }

    #[test]
    fn bounded_catalog_has_three_exact_primary_source_records() {
        assert_eq!(PUBLISHED_DISCOVERY_ESTIMATES.len(), 3);
        assert_eq!(
            newest_applicable_estimate("25/10-17 S", 42148344)
                .unwrap()
                .source_well_ids,
            &[9717]
        );
        assert_eq!(
            newest_applicable_estimate("31/1-3 S (Røver Sør)", 42148093)
                .unwrap()
                .source_well_ids,
            &[9663, 9664]
        );
    }

    #[test]
    fn newest_selection_ignores_newer_segment_estimate() {
        let mut whole = PUBLISHED_DISCOVERY_ESTIMATES[0];
        whole.estimate_date = "2024-01-01";
        let mut newer_whole = whole;
        newer_whole.estimate_date = "2025-01-01";
        let mut segment = whole;
        segment.estimate_date = "2026-01-01";
        segment.scope = EstimateScope::AppraisedSegment;
        let candidates = [whole, segment, newer_whole];
        assert_eq!(
            newest_applicable(candidates.iter()).unwrap().estimate_date,
            "2025-01-01"
        );
    }
}
