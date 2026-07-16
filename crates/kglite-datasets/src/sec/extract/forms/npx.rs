//! Form N-PX — Annual Report of Proxy Voting Record.
//!
//! Filed by mutual funds + ETFs to disclose how they voted every
//! proxy. XML format with one entry per (security, meeting, proposal).
//!
//! ## Emits
//!
//! - `processed/fund_vote.csv` — one row per vote (manager_cik,
//!   security_cusip, meeting_date, proposal_description, shares_voted,
//!   vote_for / against / abstain / withhold, management_recommendation).
//!
//! ## Goalpost section
//!
//! Coverage plan §4 — N-PX.

use crate::sec::error::Result;
use crate::sec::layout::Workdir;
use crate::sec::slicing::SliceSpec;

use super::super::identity::Identities;
use super::super::sinks::Sinks;
use super::FormReport;

/// TODO(F19): new XML parser for N-PX. Defer until insider /
/// ownership / corporate-event extractors are complete — fund-vote
/// tracking is a separate user persona.
pub fn extract(
    _workdir: &Workdir,
    _slice: &SliceSpec,
    _sinks: &mut Sinks,
    _identities: &mut Identities,
    _extracted_at: &str,
) -> Result<FormReport> {
    Ok(FormReport::default())
}
