//! Konami's Forbidden & Limited lists (Django: `scrap_ban_list` command).
//!
//! Two sources:
//! - the EU site's JSON (Django's only source, stopped at the May 18, 2026
//!   list): `options.json` maps a list number to a human date
//!   (`{"1": "August 1999", …}`); each list is `<number>.json`, the latest one
//!   also being `current.json`:
//!   `{"from": "18/05/2026", "0": [banned…], "1": [limited…], "2": [semi-limited…]}`.
//! - the US site's HTML pages (added in the Rust app): [`US_INDEX_URL`] links
//!   the current list as `list_YYYY-MM-DD/` (older and newer lists are served
//!   at the same kind of address, but not always linked). A list page is a
//!   few tables, one row per card:
//!   `<tr class="cardlist_effect"><td>Monster/Effect</td><td>MAXX    &quot;C&quot;</td>`
//!   `<td>Forbidden</td><td>Limited</td><td>New</td></tr>` (type, name in upper
//!   case with runs of spaces, Advanced Format, Traditional Format, remarks),
//!   each table starting with a `cardlist_atitle` header row. A list that
//!   doesn't exist answers 200 with a page redirecting home (no card rows).

use std::collections::HashMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use serde::Deserialize;

pub const OPTIONS_URL: &str = "https://www.yugioh-card.com/eu/_data/fllists/options.json";
pub const LISTS_URL: &str = "https://img.yugioh-card.com/eu/_data/fllists/";
pub const US_INDEX_URL: &str = "https://www.yugioh-card.com/en/limited/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanStatus {
    Banned,
    Limited,
    SemiLimited,
}

impl BanStatus {
    /// Value stored in `ban_list_entry.status`.
    pub fn code(self) -> &'static str {
        match self {
            BanStatus::Banned => "Banned",
            BanStatus::Limited => "Limited",
            BanStatus::SemiLimited => "Semi-Limited",
        }
    }
}

/// The list numbers of `options.json`, in order (the keys are strings, so
/// they are sorted numerically, not as text).
pub fn parse_options(json: &str) -> Result<Vec<(u32, String)>, serde_json::Error> {
    let options: HashMap<String, String> = serde_json::from_str(json)?;
    let mut lists: Vec<(u32, String)> = options
        .into_iter()
        .filter_map(|(number, label)| Some((number.parse().ok()?, label)))
        .collect();
    lists.sort_by_key(|(number, _)| *number);
    Ok(lists)
}

/// URL of list `number`; the last one is served as `current.json`.
pub fn list_url(number: u32, is_last: bool) -> String {
    if is_last {
        format!("{LISTS_URL}current.json")
    } else {
        format!("{LISTS_URL}{number}.json")
    }
}

#[derive(Debug, Deserialize)]
struct RawList {
    from: String,
    #[serde(rename = "0", default)]
    banned: Vec<RawEntry>,
    #[serde(rename = "1", default)]
    limited: Vec<RawEntry>,
    #[serde(rename = "2", default)]
    semi_limited: Vec<RawEntry>,
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    nameeng: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanList {
    /// Date the list became effective.
    pub date: NaiveDate,
    /// (English card name, status)
    pub entries: Vec<(String, BanStatus)>,
}

#[derive(Debug)]
pub enum BanListError {
    Json(serde_json::Error),
    Date(String),
    /// A US page without any card row (the site's answer for a missing list).
    NotAList,
    /// A US Advanced Format status that isn't Forbidden, Limited,
    /// Semi-Limited or Unlimited.
    Status(String),
}

impl std::fmt::Display for BanListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BanListError::Json(e) => write!(f, "invalid ban list JSON: {e}"),
            BanListError::Date(d) => write!(f, "invalid ban list date {d:?}"),
            BanListError::NotAList => write!(f, "no ban list on this page"),
            BanListError::Status(s) => write!(f, "unknown ban list status {s:?}"),
        }
    }
}

impl std::error::Error for BanListError {}

pub fn parse_list(json: &str) -> Result<BanList, BanListError> {
    let raw: RawList = serde_json::from_str(json).map_err(BanListError::Json)?;
    let date = NaiveDate::parse_from_str(raw.from.trim(), "%d/%m/%Y")
        .map_err(|_| BanListError::Date(raw.from.clone()))?;

    let sections = [
        (raw.banned, BanStatus::Banned),
        (raw.limited, BanStatus::Limited),
        (raw.semi_limited, BanStatus::SemiLimited),
    ];
    let entries = sections
        .into_iter()
        .flat_map(|(entries, status)| {
            entries
                .into_iter()
                .filter_map(|e| e.nameeng)
                .map(|name| clean_name(&name))
                .filter(|name| !name.is_empty())
                .map(move |name| (name, status))
        })
        .collect();

    Ok(BanList { date, entries })
}

/// Address of the US list that took effect on `date`.
pub fn us_list_url(date: NaiveDate) -> String {
    format!("{US_INDEX_URL}list_{}/", date.format("%Y-%m-%d"))
}

/// The dates of the US lists linked from a page (the index links the
/// current list as `href="/en/limited/list_2026-09-21/"`), sorted, without
/// duplicates. Links whose date isn't a real date are ignored.
pub fn parse_us_list_dates(index_html: &str) -> Vec<NaiveDate> {
    let mut dates: Vec<NaiveDate> = US_LIST_LINK
        .captures_iter(index_html)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    dates.sort();
    dates.dedup();
    dates
}

/// A US list page into a [`BanList`] effective on `date` (the date of its
/// address). Only the Advanced Format column counts: Forbidden → Banned,
/// Limited, Semi-Limited (any case); Unlimited rows (cards just released)
/// are skipped; the type, Traditional Format and remarks columns are ignored.
/// Names go through [`clean_name`] (entities decoded, runs of spaces
/// collapsed) and stay in upper case: card data lookups ignore case.
///
/// Errors: [`BanListError::NotAList`] when the page has no card row,
/// [`BanListError::Status`] for an unknown Advanced Format status.
pub fn parse_us_list(date: NaiveDate, html: &str) -> Result<BanList, BanListError> {
    let mut entries = Vec::new();
    let mut rows = 0;
    for row in US_ROW.captures_iter(html) {
        if &row[1] == "atitle" {
            continue;
        }
        rows += 1;
        let cells: Vec<String> = US_CELL
            .captures_iter(&row[2])
            .map(|c| clean_name(&c[1]))
            .collect();
        let [_, name, advanced, ..] = cells.as_slice() else {
            continue;
        };
        let status = match advanced.to_lowercase().as_str() {
            "forbidden" => BanStatus::Banned,
            "limited" => BanStatus::Limited,
            "semi-limited" => BanStatus::SemiLimited,
            "unlimited" => continue,
            _ => return Err(BanListError::Status(advanced.clone())),
        };
        if !name.is_empty() {
            entries.push((name.clone(), status));
        }
    }

    if rows == 0 {
        return Err(BanListError::NotAList);
    }
    Ok(BanList { date, entries })
}

/// `href="…/limited/list_2026-09-21/"` (absolute or not).
static US_LIST_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="[^"]*/limited/list_(\d{4}-\d{2}-\d{2})/""#).unwrap());
/// A table row: `cardlist_effect`, `cardlist_spell`… or `cardlist_atitle`
/// (the header).
static US_ROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<tr class="cardlist_(\w+)"[^>]*>(.*?)</tr>"#).unwrap());
static US_CELL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<td[^>]*>(.*?)</td>").unwrap());

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Names come with HTML entities and sometimes tags (`Droll &amp; Lock Bird`).
pub fn clean_name(name: &str) -> String {
    let name = html_escape::decode_html_entities(name);
    let name = TAG.replace_all(&name, "");
    let name = SPACES.replace_all(&name, " ");
    // entities can be double-encoded (`&amp;amp;`)
    html_escape::decode_html_entities(name.trim()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_sorted_numerically() {
        let json = r#"{"10": "October 2001", "2": "April 2000", "1": "August 1999"}"#;
        let lists = parse_options(json).unwrap();
        let numbers: Vec<u32> = lists.iter().map(|(n, _)| *n).collect();
        assert_eq!(numbers, vec![1, 2, 10]);
        assert_eq!(list_url(10, true), format!("{LISTS_URL}current.json"));
        assert_eq!(list_url(2, false), format!("{LISTS_URL}2.json"));
    }

    #[test]
    fn parses_a_list() {
        let json = r#"{
            "from": "18/05/2026", "to": "a few months from now",
            "0": [{"nameeng": "Abyss Dweller", "namefra": "x"}, {"frame": "y"}],
            "1": [{"nameeng": "Droll &amp; Lock Bird"}],
            "2": [{"nameeng": " <b>Maxx \"C\"</b>  "}]
        }"#;
        let list = parse_list(json).unwrap();
        assert_eq!(list.date, NaiveDate::from_ymd_opt(2026, 5, 18).unwrap());
        assert_eq!(
            list.entries,
            vec![
                ("Abyss Dweller".to_string(), BanStatus::Banned),
                ("Droll & Lock Bird".to_string(), BanStatus::Limited),
                ("Maxx \"C\"".to_string(), BanStatus::SemiLimited),
            ]
        );
    }

    #[test]
    fn sections_are_optional() {
        // the 1999 list only has limited cards
        let list = parse_list(r#"{"from": "01/08/1999", "1": [{"nameeng": "Raigeki"}]}"#).unwrap();
        assert_eq!(
            list.entries,
            vec![("Raigeki".to_string(), BanStatus::Limited)]
        );
    }

    #[test]
    fn rejects_bad_dates() {
        assert!(matches!(
            parse_list(r#"{"from": "2026-05-18"}"#),
            Err(BanListError::Date(_))
        ));
        assert!(matches!(parse_list("[]"), Err(BanListError::Json(_))));
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn us_list_url_uses_the_iso_date() {
        assert_eq!(
            us_list_url(date(2026, 9, 1)),
            "https://www.yugioh-card.com/en/limited/list_2026-09-01/"
        );
    }

    #[test]
    fn finds_the_us_list_dates() {
        let html = r#"
            <li><a href="/en/limited/" aria-current="page">Deck Construction Rules</a></li>
            <a class="wp-block-button__link" href="/en/limited/list_2026-09-21/">View the list here</a>
            <a href="https://www.yugioh-card.com/en/limited/list_2026-05-18/">May</a>
            <a href="/en/limited/list_2026-09-21/">again</a>
            <a href="/en/limited/list_2026-13-01/">not a date</a>
            <a href="/en/events/list_2026-01-01-results/">not a list</a>
        "#;
        assert_eq!(
            parse_us_list_dates(html),
            vec![date(2026, 5, 18), date(2026, 9, 21)]
        );
        assert_eq!(parse_us_list_dates("<p>nothing</p>"), vec![]);
    }

    /// The shape of a real list page (two of its tables), trimmed.
    const US_LIST: &str = r#"
        <p>PLEASE NOTE: The changes to "Some Card" from Forbidden to Unlimited do not take effect until later.</p>
        <table class="cardlist centertable">
          <tr align="center" class="cardlist_atitle">
            <td width="137" height="40"><strong>Card Type</strong></td>
            <td width="282"><strong>Card Name</strong></td>
            <td width="139" align="center"><strong>Advanced Format</strong></td>
            <td width="140" align="center"><strong>Traditional Format</strong></td>
            <td width="180"><strong>Remarks</strong></td>
            </tr>
          <tr class="cardlist_effect">
            <td>Monster/Effect</td>
            <td>AGIDO THE    ANCIENT SENTINEL</td>
            <td>Forbidden</td>
            <td>Limited</td>
            <td>&nbsp;</td>
          </tr>  <tr class="cardlist_effect">
            <td>Monster/Effect</td>
            <td>MAXX    &quot;C&quot;</td>
            <td>Limited</td>
            <td>Limited</td>
            <td>Was Forbidden</td>
          </tr>  <tr class="cardlist_effect">
            <td>Monster/Effect</td>
            <td>MALISS    &lt;P&gt; DORMOUSE</td>
            <td>Unlimited</td>
            <td>Limited</td>
            <td>Was Limited</td>
          </tr>
        </table>
        <table class="cardlist centertable">
          <tr align="center" class="cardlist_atitle">
            <td><strong>Card Type</strong></td>
            <td><strong>Card Name</strong></td>
            <td><strong>Advanced Format</strong></td>
            <td><strong>Traditional Format</strong></td>
            <td><strong>Remarks</strong></td>
            </tr>
          <tr class="cardlist_spell">
            <td>Spell</td>
            <td>DROLL &amp; LOCK BIRD</td>
            <td>Semi-limited</td>
            <td>Unlimited</td>
            <td>New</td>
          </tr>  <tr class="cardlist_trap">
            <td>Trap</td>
            <td>&quot;A    CASE FOR K9&quot;</td>
            <td>Semi-Limited</td>
            <td>Semi-Limited</td>
            <td>&nbsp;</td>
          </tr>
        </table>
    "#;

    #[test]
    fn parses_a_us_list() {
        let list = parse_us_list(date(2026, 9, 21), US_LIST).unwrap();
        assert_eq!(list.date, date(2026, 9, 21));
        assert_eq!(
            list.entries,
            vec![
                ("AGIDO THE ANCIENT SENTINEL".to_string(), BanStatus::Banned),
                ("MAXX \"C\"".to_string(), BanStatus::Limited),
                ("DROLL & LOCK BIRD".to_string(), BanStatus::SemiLimited),
                ("\"A CASE FOR K9\"".to_string(), BanStatus::SemiLimited),
            ]
        );
    }

    #[test]
    fn a_page_without_cards_is_not_a_list() {
        // what the site answers for a list that doesn't exist
        let redirect = r#"<html><head><meta http-equiv="refresh" content="0; url=https://www.yugioh-card.com/"></head></html>"#;
        assert!(matches!(
            parse_us_list(date(2026, 2, 2), redirect),
            Err(BanListError::NotAList)
        ));
    }

    #[test]
    fn rejects_an_unknown_us_status() {
        let html = r#"<table><tr class="cardlist_spell">
            <td>Spell</td><td>POT OF GREED</td><td>Banished</td><td>Limited</td><td>&nbsp;</td>
        </tr></table>"#;
        assert!(matches!(
            parse_us_list(date(2026, 9, 21), html),
            Err(BanListError::Status(s)) if s == "Banished"
        ));
    }

    #[test]
    fn cleans_double_encoded_entities() {
        assert_eq!(clean_name("A &amp;amp; B"), "A & B");
    }
}
