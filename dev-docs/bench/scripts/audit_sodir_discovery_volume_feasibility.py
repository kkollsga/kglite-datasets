"""Audit whether field reserve snapshots can support discovery-volume proxy nodes."""

import argparse
import csv
import io
import json
import statistics
import subprocess
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

csv.field_size_limit(10_000_000)
parser = argparse.ArgumentParser()
parser.add_argument("--archive", type=Path, required=True)
parser.add_argument("--out", type=Path, default=Path(__file__).parents[1] / "results" / "sodir-discovery-volume-feasibility-20260908.json")
args = parser.parse_args()
ARCHIVE = args.archive
PREFIX = "sodir-showcase/fresh-workdir/csv/"
OUT = args.out
MAJORS = {"STATFJORD", "OSEBERG", "ORMEN LANGE", "TROLL"}


def rows(stem):
    proc = subprocess.run(
        ["tar", "-xOf", str(ARCHIVE), f"{PREFIX}{stem}.csv"],
        capture_output=True,
        check=True,
    )
    return list(csv.DictReader(io.StringIO(proc.stdout.decode())))


def date(value):
    if not value:
        return None
    if "-" in value:
        return datetime.fromisoformat(value).date()
    return datetime.fromtimestamp(int(value) / 1000, tz=timezone.utc).date()


def quantiles(values):
    if not values:
        return None
    ordered = sorted(values)
    def at(p):
        pos = (len(ordered) - 1) * p
        lo = int(pos)
        hi = min(lo + 1, len(ordered) - 1)
        return round(ordered[lo] + (ordered[hi] - ordered[lo]) * (pos - lo), 6)
    return {"min": round(ordered[0], 6), "median": at(0.5), "p95": at(0.95), "max": round(ordered[-1], 6)}


fields = {r["fldNpdidField"]: r for r in rows("field")}
discoveries = {r["dscNpdidDiscovery"]: r for r in rows("discovery")}
history = rows("field_discoveries_incl_hst")
discovery_reserves = rows("discovery_reserves")
field_reserves = rows("field_reserves")

events_by_field = defaultdict(list)
for row in history:
    start = date(row["fldDiscoveryInclFromDate"])
    if start:
        events_by_field[row["fldNpdidField"]].append((start, row))

snapshots_by_field = defaultdict(list)
for row in field_reserves:
    when = date(row["fldDateOffResEstDisplay"])
    try:
        value = float(row["fldRecoverableOE"])
    except (TypeError, ValueError):
        continue
    if when:
        snapshots_by_field[row["fldNpdidField"]].append((when, value, row))
for values in snapshots_by_field.values():
    values.sort(key=lambda item: (item[0], int(item[2].get("fldVersion") or 0)))

reserve_ids = Counter(row["dscNpdidDiscovery"] for row in discovery_reserves)
founder_counts = Counter()
window_counts = Counter()
delta_signs = Counter()
control_deltas = []
event_deltas = []
founder_residuals = []
included_ids = set()
field_details = {}
founder_event_count = 0
bracketed_event_count = 0
after_latest_event_count = 0

for field_id, snapshots in snapshots_by_field.items():
    if not snapshots:
        continue
    events = sorted(events_by_field.get(field_id, []), key=lambda item: (item[0], item[1]["dscNpdidDiscovery"]))
    earliest_date, earliest_oe, _ = snapshots[0]
    founders = [row for when, row in events if when <= earliest_date]
    founder_event_count += len(founders)
    after_latest_event_count += sum(when > snapshots[-1][0] for when, _ in events)
    founder_counts[len(founders)] += 1
    founder_residuals.append(earliest_oe)
    included_ids.update(row["dscNpdidDiscovery"] for _, row in events)
    windows = []
    for (previous_date, previous_oe, _), (current_date, current_oe, _) in zip(snapshots, snapshots[1:]):
        additions = [row for when, row in events if previous_date < when <= current_date]
        bracketed_event_count += len(additions)
        window_counts[len(additions)] += 1
        delta = current_oe - previous_oe
        delta_signs["positive" if delta > 0 else "negative" if delta < 0 else "zero"] += 1
        (event_deltas if additions else control_deltas).append(delta)
        windows.append({
            "from": str(previous_date),
            "to": str(current_date),
            "delta_original_recoverable_oe_million_sm3": round(delta, 6),
            "inclusions": [{"discovery_id": r["dscNpdidDiscovery"], "name": r["dscName"]} for r in additions],
        })
    field_name = fields.get(field_id, {}).get("fldName", "")
    if field_name in MAJORS:
        field_details[field_name] = {
            "field_id": field_id,
            "earliest_snapshot": {"date": str(earliest_date), "original_recoverable_oe_million_sm3": earliest_oe},
            "founders_before_or_on_earliest_snapshot": [{"discovery_id": r["dscNpdidDiscovery"], "name": r["dscName"], "included_from": r["fldDiscoveryInclFromDate"]} for r in founders],
            "all_inclusions": [{"discovery_id": r["dscNpdidDiscovery"], "name": r["dscName"], "included_from": str(when), "present_in_current_discovery_csv": r["dscNpdidDiscovery"] in discoveries, "reserve_rows": reserve_ids[r["dscNpdidDiscovery"]], "redirect_target": discoveries.get(r["dscNpdidDiscovery"], {}).get("dscNpdidResInclInDisc", "")} for when, r in events],
            "windows_with_inclusions": [window for window in windows if window["inclusions"]],
            "no_inclusion_window_count": sum(not window["inclusions"] for window in windows),
        }

redirected_included = [d for d in included_ids if discoveries.get(d, {}).get("dscNpdidResInclInDisc")]
reported_included = [d for d in included_ids if reserve_ids[d]]
direct_rows_by_discovery = defaultdict(list)
for row in discovery_reserves:
    direct_rows_by_discovery[row["dscNpdidDiscovery"]].append(row)

sole_field_eligible = []
sole_field_rejections = Counter()
for field_id, field in fields.items():
    events = events_by_field.get(field_id, [])
    ids = {row["dscNpdidDiscovery"] for _, row in events}
    if len(ids) != 1:
        sole_field_rejections["history_not_exactly_one_distinct_discovery"] += 1
        continue
    discovery_id = next(iter(ids))
    discovery = discoveries.get(discovery_id)
    if discovery is None:
        sole_field_rejections["history_discovery_missing_current_csv"] += 1
        continue
    if discovery.get("fldNpdidField") != field_id:
        sole_field_rejections["current_discovery_field_mismatch"] += 1
        continue
    redirect = discovery.get("dscNpdidResInclInDisc", "")
    if redirect and redirect != discovery_id:
        sole_field_rejections["redirects_to_other_discovery"] += 1
        continue
    snapshots = snapshots_by_field.get(field_id, [])
    if not snapshots:
        sole_field_rejections["no_field_reserve_snapshot"] += 1
        continue
    latest = snapshots[-1]
    direct = direct_rows_by_discovery.get(discovery_id, [])
    sole_field_eligible.append({
        "field_id": field_id,
        "field": field["fldName"],
        "discovery_id": discovery_id,
        "discovery": discovery["dscName"],
        "latest_field_snapshot": {"date": str(latest[0]), "original_recoverable_oe_million_sm3": latest[1]},
        "direct_discovery_rows": [{"reserve_class": row["dscReservesRC"], "date": str(date(row["dscDateOffResEstDisplay"])), "original_recoverable_oe_million_sm3": float(row["dscRecoverableOe"])} for row in direct],
    })

field_linked_direct_rows = []
for discovery_id in reported_included:
    discovery = discoveries.get(discovery_id, {})
    for row in direct_rows_by_discovery[discovery_id]:
        field_linked_direct_rows.append({
            "field_id": discovery.get("fldNpdidField", ""),
            "discovery_id": discovery_id,
            "reserve_class": row["dscReservesRC"],
            "date": str(date(row["dscDateOffResEstDisplay"])),
            "original_recoverable_oe_million_sm3": float(row["dscRecoverableOe"]),
            "redirect_target": discovery.get("dscNpdidResInclInDisc", ""),
        })
output = {
    "source_archive": str(ARCHIVE),
    "scope": "Public field inclusion history and annual original recoverable OE snapshots; deltas are reporting changes, not measured discovery resources.",
    "source_counts": {
        "fields_csv_rows": len(fields),
        "discoveries_csv_rows": len(discoveries),
        "field_inclusion_history_rows": len(history),
        "fields_with_inclusion_history": len(events_by_field),
        "field_reserve_snapshot_rows": len(field_reserves),
        "fields_with_usable_reserve_snapshots": len(snapshots_by_field),
        "discovery_reserve_rows": len(discovery_reserves),
        "discoveries_with_reported_reserve_rows": len(reserve_ids),
    },
    "founders_at_earliest_snapshot": {
        "fields_by_founder_count": dict(sorted(founder_counts.items())),
        "founder_event_count": founder_event_count,
        "earliest_snapshot_oe_distribution_million_sm3": quantiles(founder_residuals),
    },
    "annual_windows": {
        "windows_by_inclusion_count": dict(sorted(window_counts.items())),
        "delta_signs_all": dict(delta_signs),
        "no_inclusion_control_count": len(control_deltas),
        "no_inclusion_delta_signs": {"positive": sum(value > 0 for value in control_deltas), "zero": sum(value == 0 for value in control_deltas), "negative": sum(value < 0 for value in control_deltas)},
        "no_inclusion_delta_distribution_million_sm3": quantiles(control_deltas),
        "inclusion_window_count": len(event_deltas),
        "inclusion_delta_distribution_million_sm3": quantiles(event_deltas),
        "positive_inclusion_windows": sum(value > 0 for value in event_deltas),
        "zero_inclusion_windows": sum(value == 0 for value in event_deltas),
        "negative_inclusion_windows": sum(value < 0 for value in event_deltas),
        "bracketed_inclusion_event_count": bracketed_event_count,
        "inclusion_events_after_latest_snapshot": after_latest_event_count,
    },
    "overlap_and_redirects": {
        "unique_discoveries_in_field_history": len(included_ids),
        "included_discoveries_with_direct_reserve_rows": len(reported_included),
        "their_direct_reserve_row_count": sum(reserve_ids[d] for d in reported_included),
        "included_discoveries_redirected_into_another_discovery": len(redirected_included),
        "included_discoveries_with_both_direct_reserves_and_redirect": len(set(reported_included) & set(redirected_included)),
        "included_discoveries_missing_from_current_discovery_csv": len([d for d in included_ids if d not in discoveries]),
        "field_linked_direct_rows": field_linked_direct_rows,
    },
    "sole_discovery_field_rule": {
        "eligible_count": len(sole_field_eligible),
        "eligible_latest_original_oe_sum_million_sm3": round(sum(row["latest_field_snapshot"]["original_recoverable_oe_million_sm3"] for row in sole_field_eligible), 6),
        "eligible_with_direct_discovery_rows": sum(bool(row["direct_discovery_rows"]) for row in sole_field_eligible),
        "rejections": dict(sole_field_rejections),
        "eligible_fields": sorted(sole_field_eligible, key=lambda row: row["field"]),
    },
    "major_examples": field_details,
    "allocation_contract": {
        "single_founder": "Earliest field original-OE may be attached as a field-derived founder proxy only when exactly one founder predates the first snapshot; preserve snapshot date and field provenance.",
        "multiple_founders": "Keep the earliest residual on a founder-group proxy. Do not equal-split without an independent source weight.",
        "single_inclusion_window": "A positive annual delta may become an inclusion-window proxy linked to the one discovery, labelled as a reporting delta rather than discovery resource. Zero/negative deltas remain explicit unresolved observations.",
        "multiple_inclusion_window": "Keep one group delta linked to every inclusion in the bracket. Do not allocate among discoveries without independent evidence.",
        "direct_discovery_reserves": "Prefer source-reported discovery snapshots and keep redirected/rolled-up discoveries distinct. Never add a direct discovery value to a field-derived proxy without overlap accounting.",
    },
}
OUT.write_text(json.dumps(output, indent=2, sort_keys=True) + "\n")
print(OUT)
print(json.dumps({"source_counts": output["source_counts"], "founders": output["founders_at_earliest_snapshot"], "annual_windows": output["annual_windows"], "overlap": output["overlap_and_redirects"]}, indent=2))
