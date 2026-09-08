//! Fetch and normalize the press releases referenced by Sodir wellbores.
//!
//! A release is identified by its source URL because one announcement often
//! covers several wellbores. The raw response is cached under the workdir;
//! graph-facing CSVs contain Markdown, wellbore links, and conservative volume
//! mentions. A mention remains evidence from a document, not an automatically
//! accepted discovery reserve.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use scraper::{Html, Selector};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::sodir::client::ArcGISClient;
use crate::sodir::error::{Result, SodirError};
use crate::sodir::layout::Workdir;

const RELEASE_OUTPUT: &str = "_derived_press_release.csv";
const LINK_OUTPUT: &str = "_derived_wellbore_press_release.csv";
const VOLUME_OUTPUT: &str = "_derived_press_release_volume.csv";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PressReleaseReport {
    pub selected: usize,
    pub fetched: usize,
    pub cached: usize,
    pub parsed: usize,
    pub failed: usize,
    pub documents: usize,
    pub wellbore_links: usize,
    pub volume_mentions: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseRow {
    press_release_id: String,
    title: String,
    source_url: String,
    source_format: String,
    markdown: String,
    content_sha256: String,
    fetch_status: String,
    fetch_error: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkRow {
    press_release_id: String,
    wlb_npdid_wellbore: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct VolumeMention {
    volume_mention_id: String,
    press_release_id: String,
    mention_index: usize,
    minimum: Option<f64>,
    maximum: Option<f64>,
    value: Option<f64>,
    bound: String,
    unit: String,
    commodity: String,
    scope: String,
    estimate_status: String,
    source_text: String,
}

struct RawVolumeMatch {
    start: usize,
    end: usize,
    minimum: Option<f64>,
    maximum: Option<f64>,
    value: Option<f64>,
    bound: String,
    unit: String,
}

/// Create empty graph-facing outputs when document enrichment has not been
/// requested. This keeps the packaged blueprint usable without a network pass.
pub fn ensure_outputs(csv_dir: &Path) -> Result<()> {
    if !csv_dir.join(RELEASE_OUTPUT).is_file() {
        write_release_rows(&csv_dir.join(RELEASE_OUTPUT), &[])?;
    }
    if !csv_dir.join(LINK_OUTPUT).is_file() {
        write_link_rows(&csv_dir.join(LINK_OUTPUT), &[])?;
    }
    if !csv_dir.join(VOLUME_OUTPUT).is_file() {
        write_volume_rows(&csv_dir.join(VOLUME_OUTPUT), &[])?;
    }
    Ok(())
}

/// Fetch up to `limit` distinct release URLs referenced by `wellbore.csv`.
/// `None` processes every distinct URL. Individual fetch or parse failures are
/// recorded as document rows and do not discard the rest of the batch.
pub fn fetch(workdir: &Workdir, limit: Option<usize>) -> Result<PressReleaseReport> {
    workdir.ensure_dirs()?;
    let references = release_references(&workdir.csv_path("wellbore"))?;
    let selected: Vec<_> = references
        .into_iter()
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    let raw_dir = workdir.root().join("press_releases").join("raw");
    fs::create_dir_all(&raw_dir)?;
    let client = ArcGISClient::new()?;
    let mut report = PressReleaseReport {
        selected: selected.len(),
        ..Default::default()
    };
    let mut releases = Vec::new();
    let mut links = Vec::new();
    let mut volumes = Vec::new();

    for (url, well_ids) in selected {
        let release_id = digest(&url);
        let raw_path = raw_dir.join(format!("{release_id}.source"));
        let bytes = if raw_path.is_file() {
            report.cached += 1;
            fs::read(&raw_path)?
        } else {
            match client.fetch_bytes(&url) {
                Ok(bytes) => {
                    fs::write(&raw_path, &bytes)?;
                    report.fetched += 1;
                    bytes
                }
                Err(error) => {
                    report.failed += 1;
                    releases.push(ReleaseRow {
                        press_release_id: release_id.clone(),
                        title: title_from_url(&url),
                        source_url: url.clone(),
                        source_format: format_hint(&url).into(),
                        markdown: String::new(),
                        content_sha256: String::new(),
                        fetch_status: "fetch_failed".into(),
                        fetch_error: error.to_string(),
                    });
                    append_links(&mut links, &release_id, well_ids);
                    continue;
                }
            }
        };

        let checksum = digest_bytes(&bytes);
        match document_to_markdown(&url, &bytes) {
            Ok((format, markdown)) => {
                report.parsed += 1;
                let title = markdown_title(&markdown).unwrap_or_else(|| title_from_url(&url));
                let mut found = extract_volume_mentions(&release_id, &markdown);
                volumes.append(&mut found);
                releases.push(ReleaseRow {
                    press_release_id: release_id.clone(),
                    title,
                    source_url: url.clone(),
                    source_format: format.into(),
                    markdown,
                    content_sha256: checksum,
                    fetch_status: "parsed".into(),
                    fetch_error: String::new(),
                });
            }
            Err(error) => {
                report.failed += 1;
                releases.push(ReleaseRow {
                    press_release_id: release_id.clone(),
                    title: title_from_url(&url),
                    source_url: url.clone(),
                    source_format: format_hint(&url).into(),
                    markdown: String::new(),
                    content_sha256: checksum,
                    fetch_status: "parse_failed".into(),
                    fetch_error: error,
                });
            }
        }
        append_links(&mut links, &release_id, well_ids);
    }

    releases.sort_by(|a, b| a.press_release_id.cmp(&b.press_release_id));
    links.sort_by(|a, b| {
        (&a.press_release_id, &a.wlb_npdid_wellbore)
            .cmp(&(&b.press_release_id, &b.wlb_npdid_wellbore))
    });
    volumes.sort_by(|a, b| a.volume_mention_id.cmp(&b.volume_mention_id));
    write_release_rows(&workdir.csv_dir().join(RELEASE_OUTPUT), &releases)?;
    write_link_rows(&workdir.csv_dir().join(LINK_OUTPUT), &links)?;
    write_volume_rows(&workdir.csv_dir().join(VOLUME_OUTPUT), &volumes)?;
    report.documents = releases.len();
    report.wellbore_links = links.len();
    report.volume_mentions = volumes.len();
    Ok(report)
}

fn release_references(path: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    if !path.is_file() {
        return Err(SodirError::Malformed(format!(
            "press-release enrichment requires {}",
            path.display()
        )));
    }
    let mut reader = csv::Reader::from_path(path)
        .map_err(|error| SodirError::Csv(format!("open {}: {error}", path.display())))?;
    let headers = reader
        .headers()
        .map_err(|error| SodirError::Csv(format!("headers {}: {error}", path.display())))?
        .clone();
    let url_index = headers
        .iter()
        .position(|name| name == "wlbPressReleaseUrl")
        .ok_or_else(|| SodirError::Malformed("wellbore.csv lacks wlbPressReleaseUrl".into()))?;
    let well_index = headers
        .iter()
        .position(|name| name == "wlbNpdidWellbore")
        .ok_or_else(|| SodirError::Malformed("wellbore.csv lacks wlbNpdidWellbore".into()))?;
    let mut references: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for record in reader.records() {
        let record = record.map_err(|error| SodirError::Csv(error.to_string()))?;
        let url = record.get(url_index).unwrap_or("").trim();
        let well_id = record.get(well_index).unwrap_or("").trim();
        if !url.is_empty() && !well_id.is_empty() {
            references
                .entry(url.to_string())
                .or_default()
                .insert(well_id.to_string());
        }
    }
    Ok(references)
}

fn append_links(links: &mut Vec<LinkRow>, release_id: &str, well_ids: BTreeSet<String>) {
    links.extend(well_ids.into_iter().map(|well_id| LinkRow {
        press_release_id: release_id.to_string(),
        wlb_npdid_wellbore: well_id,
    }));
}

fn document_to_markdown(
    url: &str,
    bytes: &[u8],
) -> std::result::Result<(&'static str, String), String> {
    if bytes.starts_with(b"%PDF") || url.to_ascii_lowercase().ends_with(".pdf") {
        let extracted = catch_unwind(AssertUnwindSafe(|| {
            pdf_extract::extract_text_from_mem(bytes)
        }))
        .map_err(|_| "PDF text extraction panicked".to_string())?
        .map_err(|error| format!("PDF text extraction failed: {error}"))?;
        if extracted.trim().is_empty() {
            return Err("PDF contains no extractable text".into());
        }
        return Ok(("pdf", plain_text_to_markdown(&extracted)));
    }
    let html = std::str::from_utf8(bytes).map_err(|error| format!("HTML is not UTF-8: {error}"))?;
    let document = Html::parse_document(html);
    let article = Selector::parse("main .article, main article, article, main")
        .expect("static article selector is valid");
    let body = document
        .select(&article)
        .next()
        .map(|element| element.html())
        .ok_or_else(|| "HTML has no article or main element".to_string())?;
    let markdown = quick_html2md::html_to_markdown(&body);
    if markdown.trim().is_empty() {
        return Err("HTML article converted to empty Markdown".into());
    }
    Ok(("html", normalize_markdown(&markdown)))
}

fn plain_text_to_markdown(text: &str) -> String {
    let normalized = text.replace('\r', "").replace('\u{c}', "\n\n");
    let mut paragraphs = Vec::new();
    for block in normalized.split("\n\n") {
        let joined = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            paragraphs.push(joined);
        }
    }
    let Some((title, body)) = paragraphs.split_first() else {
        return String::new();
    };
    let title = title.trim_start_matches('#').trim();
    if body.is_empty() {
        format!("# {title}\n")
    } else {
        format!("# {title}\n\n{}\n", body.join("\n\n"))
    }
}

fn normalize_markdown(markdown: &str) -> String {
    let mut out = markdown
        .replace("Sm^{3}", "Sm3")
        .replace("Sm 3", "Sm3")
        .replace("m 3", "m3");
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn extract_volume_mentions(release_id: &str, markdown: &str) -> Vec<VolumeMention> {
    let text = extraction_text(markdown);
    let mut matches = Vec::new();
    for captures in range_regex().captures_iter(&text) {
        let full = captures.get(0).expect("range match");
        let Some(minimum) = parse_number(&captures["minimum"]) else {
            continue;
        };
        let Some(maximum) = parse_number(&captures["maximum"]) else {
            continue;
        };
        if minimum > maximum {
            continue;
        }
        matches.push(RawVolumeMatch {
            start: full.start(),
            end: full.end(),
            minimum: Some(minimum),
            maximum: Some(maximum),
            value: None,
            bound: "range".into(),
            unit: unit_name(&captures["scale"], &captures["unit"]),
        });
    }
    for captures in point_regex().captures_iter(&text) {
        let full = captures.get(0).expect("point match");
        if matches
            .iter()
            .any(|item| full.start() < item.end && full.end() > item.start)
        {
            continue;
        }
        let Some(value) = parse_number(&captures["value"]) else {
            continue;
        };
        let qualifier = captures
            .name("qualifier")
            .map(|item| item.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let bound = if qualifier.contains("more") || qualifier.contains("over") || qualifier == ">"
        {
            "more_than"
        } else if qualifier.contains("less") || qualifier.contains("under") || qualifier == "<" {
            "less_than"
        } else {
            "point"
        };
        matches.push(RawVolumeMatch {
            start: full.start(),
            end: full.end(),
            minimum: None,
            maximum: None,
            value: Some(value),
            bound: bound.into(),
            unit: unit_name(&captures["scale"], &captures["unit"]),
        });
    }
    matches.sort_by_key(|item| item.start);

    let mut mentions = Vec::new();
    for matched in matches {
        let RawVolumeMatch {
            start,
            end,
            minimum,
            maximum,
            value,
            bound,
            unit,
        } = matched;
        let sentence = sentence_around(&text, start, end);
        let lower = sentence.to_lowercase();
        if is_rate(&lower) || !is_resource_context(&lower) {
            continue;
        }
        let commodity = commodity_near(&text, start, end);
        let scope = scope(&lower);
        let estimate_status = if lower.contains("preliminary") || lower.contains("foreløpig") {
            "preliminary"
        } else {
            "unspecified"
        };
        let index = mentions.len();
        let identity = format!("{release_id}|{index}|{start}|{end}");
        mentions.push(VolumeMention {
            volume_mention_id: digest(&identity),
            press_release_id: release_id.to_string(),
            mention_index: index,
            minimum,
            maximum,
            value,
            bound,
            unit,
            commodity: commodity.into(),
            scope: scope.into(),
            estimate_status: estimate_status.into(),
            source_text: sentence.to_string(),
        });
    }
    mentions
}

fn extraction_text(markdown: &str) -> String {
    markdown
        .replace("Sm^{3}", "Sm3")
        .replace("Sm 3", "Sm3")
        .split("\n\n")
        .filter_map(|paragraph| {
            let text = paragraph
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let text = text.trim_start_matches('#').trim();
            (!text.is_empty()).then(|| text.to_string())
        })
        .map(|paragraph| {
            if paragraph.ends_with(['.', '!', '?']) {
                paragraph
            } else {
                format!("{paragraph}.")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?ix)(?P<minimum>\d+(?:[.,]\d+)?)\s*(?:-|–|—|\bto\b|\band\b|\bog\b)\s*(?P<maximum>\d+(?:[.,]\d+)?)\s*(?P<scale>million(?:er)?|millions?|mill\.?|billion(?:er)?|billions?|bn\.?|milliard(?:er)?|mrd\.?)\s*(?P<unit>(?:standard\s+)?cubic\s+met(?:re|er)s?(?:\s*\(\s*sm3\s*\))?|(?:standard\s+)?kubikkmeter(?:\s*\(\s*sm3\s*\))?|sm3|scm|fat|barrels?|bbl)").expect("static range regex is valid")
    })
}

fn point_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?ix)(?P<qualifier>more\s+than|less\s+than|over|under|>|<)?\s*(?P<value>\d+(?:[.,]\d+)?)\s*(?P<scale>million(?:er)?|millions?|mill\.?|billion(?:er)?|billions?|bn\.?|milliard(?:er)?|mrd\.?)\s*(?P<unit>(?:standard\s+)?cubic\s+met(?:re|er)s?(?:\s*\(\s*sm3\s*\))?|(?:standard\s+)?kubikkmeter(?:\s*\(\s*sm3\s*\))?|sm3|scm|fat|barrels?|bbl)").expect("static point regex is valid")
    })
}

fn parse_number(value: &str) -> Option<f64> {
    value.replace(',', ".").parse().ok()
}

fn unit_name(scale: &str, unit: &str) -> String {
    let scale = scale.to_ascii_lowercase();
    let unit = unit.to_ascii_lowercase();
    let magnitude = if scale.starts_with("bill")
        || scale.starts_with("bn")
        || scale.starts_with("milliard")
        || scale.starts_with("mrd")
    {
        "billion"
    } else {
        "million"
    };
    if unit.contains("barrel") || unit.contains("bbl") || unit == "fat" {
        format!("{magnitude}_barrels")
    } else {
        format!("{magnitude}_sm3")
    }
}

fn sentence_around(text: &str, start: usize, end: usize) -> &str {
    let before = text[..start]
        .rfind(['.', '!', '?'])
        .map_or(0, |index| index + 1);
    let after = text[end..]
        .find(['.', '!', '?'])
        .map_or(text.len(), |index| end + index + 1);
    text[before..after].trim()
}

fn is_resource_context(text: &str) -> bool {
    [
        "recoverable",
        "reserve",
        "resource",
        "size of the discovery",
        "size of this discovery",
        "størrelsen på funnet",
        "størrelse på funnet",
        "utvinnbar",
        "proved more than",
        "påviste mer enn",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn is_rate(text: &str) -> bool {
    [
        "per flow day",
        "per day",
        "per døgn",
        "per dag",
        "/d",
        "daily rate",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn commodity_near(text: &str, start: usize, end: usize) -> &'static str {
    let sentence_start = text[..start]
        .rfind(['.', '!', '?'])
        .map_or(0, |index| index + 1);
    let sentence_end = text[end..]
        .find(['.', '!', '?'])
        .map_or(text.len(), |index| end + index);
    first_commodity(&text[end..sentence_end])
        .or_else(|| last_commodity(&text[sentence_start..start]))
        .unwrap_or("petroleum")
}

fn first_commodity(text: &str) -> Option<&'static str> {
    commodity_terms()
        .iter()
        .filter_map(|(term, label)| {
            text.to_lowercase()
                .find(term)
                .map(|index| (index, term.len(), *label))
        })
        .min_by_key(|(index, length, _)| (*index, std::cmp::Reverse(*length)))
        .map(|(_, _, label)| label)
}

fn last_commodity(text: &str) -> Option<&'static str> {
    commodity_terms()
        .iter()
        .filter_map(|(term, label)| {
            text.to_lowercase()
                .rfind(term)
                .map(|index| (index, term.len(), *label))
        })
        .max_by_key(|(index, length, _)| (*index, *length))
        .map(|(_, _, label)| label)
}

fn commodity_terms() -> &'static [(&'static str, &'static str)] {
    &[
        ("oil equivalents", "oil_equivalent"),
        ("oil equivalent", "oil_equivalent"),
        ("oljeekvivalent", "oil_equivalent"),
        ("condensate", "condensate"),
        ("kondensat", "condensate"),
        ("gas", "gas"),
        ("oil", "oil"),
        ("olje", "oil"),
    ]
}

fn scope(text: &str) -> &'static str {
    if (text.contains("total") || text.contains("samlet"))
        && (text.contains("vicinity")
            || text.contains("other")
            || text.contains("connection")
            || text.contains("området"))
    {
        "combined_area"
    } else if text.contains("size of the discovery")
        || text.contains("size of this discovery")
        || text.contains("størrelsen på funnet")
        || text.contains("størrelse på funnet")
    {
        "whole_discovery"
    } else {
        "unspecified"
    }
}

fn markdown_title(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_string)
}

fn title_from_url(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("Press release")
        .trim_end_matches(".pdf")
        .replace('-', " ")
}

fn format_hint(url: &str) -> &'static str {
    if url.to_ascii_lowercase().ends_with(".pdf") {
        "pdf"
    } else {
        "html"
    }
}

fn digest(value: &str) -> String {
    digest_bytes(value.as_bytes())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_release_rows(path: &Path, rows: &[ReleaseRow]) -> Result<()> {
    write_rows(
        path,
        &[
            "pressReleaseId",
            "title",
            "sourceUrl",
            "sourceFormat",
            "markdown",
            "contentSha256",
            "fetchStatus",
            "fetchError",
        ],
        rows,
    )
}

fn write_link_rows(path: &Path, rows: &[LinkRow]) -> Result<()> {
    write_rows(path, &["pressReleaseId", "wlbNpdidWellbore"], rows)
}

fn write_volume_rows(path: &Path, rows: &[VolumeMention]) -> Result<()> {
    write_rows(
        path,
        &[
            "volumeMentionId",
            "pressReleaseId",
            "mentionIndex",
            "minimum",
            "maximum",
            "value",
            "bound",
            "unit",
            "commodity",
            "scope",
            "estimateStatus",
            "sourceText",
        ],
        rows,
    )
}

fn write_rows<T: Serialize>(path: &Path, headers: &[&str], rows: &[T]) -> Result<()> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_path(path)
        .map_err(|error| SodirError::Csv(format!("open {}: {error}", path.display())))?;
    writer
        .write_record(headers)
        .map_err(|error| SodirError::Csv(format!("header {}: {error}", path.display())))?;
    for row in rows {
        writer
            .serialize(row)
            .map_err(|error| SodirError::Csv(format!("write {}: {error}", path.display())))?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_resource_ranges_and_rejects_flow_rates() {
        let markdown = "Preliminary calculations of the size of the discovery are between 10 and 23 million standard cubic metres (Sm3) of recoverable oil and between 8 and 15 billion standard cubic metres of recoverable gas. The maximum production rate was 683 million Sm3 oil per flow day.";
        let mentions = extract_volume_mentions("release", markdown);
        assert_eq!(mentions.len(), 2);
        assert_eq!(
            (mentions[0].minimum, mentions[0].maximum),
            (Some(10.0), Some(23.0))
        );
        assert_eq!(mentions[0].commodity, "oil");
        assert_eq!(mentions[0].unit, "million_sm3");
        assert_eq!(mentions[0].scope, "whole_discovery");
        assert_eq!(mentions[1].commodity, "gas");
        assert_eq!(mentions[1].unit, "billion_sm3");
    }

    #[test]
    fn keeps_discovery_and_combined_area_estimates_distinct() {
        let markdown = "Preliminary estimates place the size of the discovery between 30 and 65 million Sm3 of recoverable oil. If appraisal wells confirm the connection with other discoveries, the total resources could be between 80 and 190 million Sm3 of recoverable oil.";
        let mentions = extract_volume_mentions("release", markdown);
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].scope, "whole_discovery");
        assert_eq!(mentions[1].scope, "combined_area");
    }

    #[test]
    fn cached_html_builds_one_release_two_links_and_volume_rows() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = Workdir::new(temp.path());
        workdir.ensure_dirs().unwrap();
        let url = "https://example.invalid/release";
        fs::write(
            workdir.csv_path("wellbore"),
            format!("wlbNpdidWellbore,wlbPressReleaseUrl\n10,{url}\n11,{url}\n"),
        )
        .unwrap();
        let raw_dir = workdir.root().join("press_releases/raw");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(
            raw_dir.join(format!("{}.source", digest(url))),
            b"<html><body><main><article><h1>Discovery</h1><p>Preliminary calculations of the size of the discovery are between 6.1 and 11.8 million standard cubic metres of recoverable oil equivalent.</p></article></main></body></html>",
        )
        .unwrap();

        let report = fetch(&workdir, Some(1)).unwrap();
        assert_eq!(report.cached, 1);
        assert_eq!(report.documents, 1);
        assert_eq!(report.wellbore_links, 2);
        assert_eq!(report.volume_mentions, 1);
        let release_csv = fs::read_to_string(workdir.csv_dir().join(RELEASE_OUTPUT)).unwrap();
        assert!(release_csv.contains("# Discovery"));
        assert!(release_csv.contains("6.1 and 11.8"));
    }
}
