//! Konami's EU Forbidden & Limited lists (Django: `scrap_ban_list` command).
//!
//! `options.json` maps a list number to a human date (`{"1": "August 1999", …}`);
//! each list is `<number>.json`, the latest one also being `current.json`:
//! `{"from": "18/05/2026", "0": [banned…], "1": [limited…], "2": [semi-limited…]}`.

use std::collections::HashMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use serde::Deserialize;

pub const OPTIONS_URL: &str = "https://www.yugioh-card.com/eu/_data/fllists/options.json";
pub const LISTS_URL: &str = "https://img.yugioh-card.com/eu/_data/fllists/";

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
}

impl std::fmt::Display for BanListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BanListError::Json(e) => write!(f, "invalid ban list JSON: {e}"),
            BanListError::Date(d) => write!(f, "invalid ban list date {d:?}"),
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
        assert_eq!(list.entries, vec![("Raigeki".to_string(), BanStatus::Limited)]);
    }

    #[test]
    fn rejects_bad_dates() {
        assert!(matches!(parse_list(r#"{"from": "2026-05-18"}"#), Err(BanListError::Date(_))));
        assert!(matches!(parse_list("[]"), Err(BanListError::Json(_))));
    }

    #[test]
    fn cleans_double_encoded_entities() {
        assert_eq!(clean_name("A &amp;amp; B"), "A & B");
    }
}
