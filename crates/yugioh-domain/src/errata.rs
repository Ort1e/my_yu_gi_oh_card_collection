//! Errata (new, not in Django): a card whose text changed after a format's
//! cutoff. Buying a printing printed after the change gets the wrong text
//! for that format, so `/decks/built` warns about it and says which
//! printings have the text in force at the cutoff.
//!
//! The source is Yugipedia's `Card Errata:<name>` page: its `== English ==`
//! section holds a `{{Errata table}}` with one `loreN` (the text) and one
//! `capN` (the printing that first carried it, then its set, then perhaps
//! the product that reprinted it in italics) per version, oldest first.
//! The store fetches and caches the page; everything here is pure.
//!
//! Deliberate choices (written by Claude on the owner's request,
//! 2026-11-06, like `format`):
//! - **any** text change after the cutoff is reported, not only the ones
//!   that change how the card plays: the old and new texts are shown and
//!   the owner judges;
//! - a version's date is its `reprint_in` set's date when there is one, else
//!   its set's (found by the set code, then by the set name); a version
//!   whose set has no date is ignored;
//! - a printing has the text in force at the cutoff when it came out in
//!   `[in_force_from, right_until)`: not before the version that was in
//!   force (older text), not from the first change after the cutoff.

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One version of a card's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextVersion {
    /// The text as printed (wiki markup cleaned).
    pub text: String,
    /// The printing that first carried it (`DPRP-EN038`).
    pub code: String,
    /// That printing's set (`Duelist Pack: Rivals of the Pharaoh`).
    pub set: String,
    /// The product that reprinted this text, when the table says so.
    pub reprint_in: Option<String>,
}

/// Wiki markup as plain text: `<del>…</del>` dropped, `<ins>`, bold and
/// italics removed, `[[a|b]]` → `b`, `<br />` → a space.
fn clean(markup: &str) -> String {
    let mut text = markup.to_string();
    while let (Some(start), Some(end)) = (text.find("<del>"), text.find("</del>")) {
        if end < start {
            break;
        }
        text.replace_range(start..end + "</del>".len(), "");
    }

    for tag in ["<ins>", "</ins>"] {
        text = text.replace(tag, "");
    }
    for tag in ["<br />", "<br/>", "<br>"] {
        text = text.replace(tag, " ");
    }
    text = text.replace("'''", "").replace("''", "");

    // links, innermost first: `[[target|shown]]` shows `shown`
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let link = &after[..end];
        out.push_str(link.rsplit('|').next().unwrap_or(link));
        rest = &after[end + 2..];
    }
    out.push_str(rest);

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `{{Errata table …}}` of the English section: its parameters
/// (`lore1`, `cap1`…) as written, `None` when there is none.
fn english_table(wikitext: &str) -> Option<HashMap<String, String>> {
    // from `== English ==` to the next `== …` heading of the same level
    let start = wikitext
        .lines()
        .scan(0usize, |offset, line| {
            let at = *offset;
            *offset += line.len() + 1;
            Some((at, line))
        })
        .find(|(_, line)| {
            let line = line.trim();
            line.starts_with("==")
                && !line.starts_with("===")
                && line.trim_matches('=').trim() == "English"
        })
        .map(|(at, line)| at + line.len())?;
    let section = &wikitext[start.min(wikitext.len())..];
    let section = section
        .match_indices("\n==")
        .find(|(at, _)| !section[at + 1..].starts_with("==="))
        .map_or(section, |(at, _)| &section[..at]);

    let begin = section.find("{{Errata table")?;
    let body = &section[begin + 2..];

    // split on the `|` of the table itself, not those of links and templates
    let (mut depth, mut part) = (0usize, String::new());
    let mut parts: Vec<String> = Vec::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('{' | '[', Some(next)) if *next == c => {
                depth += 1;
                part.push(c);
                part.push(chars.next().unwrap_or(c));
            }
            ('}' | ']', Some(next)) if *next == c => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                part.push(c);
                part.push(chars.next().unwrap_or(c));
            }
            ('|', _) if depth == 0 => parts.push(std::mem::take(&mut part)),
            _ => part.push(c),
        }
    }
    parts.push(part);

    Some(
        parts
            .into_iter()
            .skip(1) // the template's name
            .filter_map(|p| {
                let (key, value) = p.split_once('=')?;
                Some((key.trim().to_lowercase(), value.trim().to_string()))
            })
            .collect(),
    )
}

/// The text versions of an errata page's English section, oldest first
/// (`loreN` with its `capN`, in the order of `N`). The Japanese table and
/// the other fields are ignored; a version with no text or no printing is
/// left out.
pub fn parse(wikitext: &str) -> Vec<TextVersion> {
    let Some(fields) = english_table(wikitext) else {
        return Vec::new();
    };

    let mut numbers: Vec<u32> = fields
        .keys()
        .filter_map(|k| k.strip_prefix("lore")?.parse().ok())
        .collect();
    numbers.sort_unstable();

    numbers
        .into_iter()
        .filter_map(|n| {
            let text = clean(fields.get(&format!("lore{n}"))?);
            let cap = fields.get(&format!("cap{n}"))?;
            let mut lines = cap
                .split("<br />")
                .flat_map(|l| l.split("<br/>"))
                .flat_map(|l| l.split("<br>"))
                .map(clean)
                .filter(|l| !l.is_empty());
            let code = lines.next()?;
            let set = lines.next().unwrap_or_default();
            let reprint_in = lines.next();
            (!text.is_empty()).then_some(TextVersion {
                text,
                code,
                set,
                reprint_in,
            })
        })
        .collect()
}

/// The TCG dates of the sets, by code and by name (ygoprodeck's
/// `cardsets.php`, `ygo_set`).
#[derive(Debug, Default)]
pub struct SetDates {
    by_code: HashMap<String, NaiveDate>,
    by_name: HashMap<String, NaiveDate>,
}

impl SetDates {
    /// From `(name, code, date)` rows; the rows with no date are skipped.
    pub fn new(
        sets: impl IntoIterator<Item = (String, Option<String>, Option<NaiveDate>)>,
    ) -> Self {
        let mut dates = SetDates::default();
        for (name, code, date) in sets {
            let Some(date) = date else { continue };
            if let Some(code) = code.filter(|c| !c.is_empty()) {
                dates.by_code.insert(code.to_uppercase(), date);
            }
            dates.by_name.insert(name.to_lowercase(), date);
        }
        dates
    }

    /// The date of a printing's set from its code (`DPRP-EN038`, `DPRP-FR038`
    /// → `DPRP`).
    pub fn of_code(&self, code: &str) -> Option<NaiveDate> {
        let prefix = code.split('-').next()?.trim().to_uppercase();
        self.by_code.get(&prefix).copied()
    }

    pub fn of_name(&self, name: &str) -> Option<NaiveDate> {
        self.by_name.get(&name.trim().to_lowercase()).copied()
    }

    /// When a text version came out: its reprint's set, else its printing's
    /// set (by code, then by name).
    pub fn of_version(&self, version: &TextVersion) -> Option<NaiveDate> {
        if let Some(date) = version.reprint_in.as_deref().and_then(|r| self.of_name(r)) {
            return Some(date);
        }
        self.of_code(&version.code)
            .or_else(|| self.of_name(&version.set))
    }
}

/// A card's text changed after a cutoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Errata {
    /// The text in force at the cutoff.
    pub in_force: TextVersion,
    /// When it came out.
    pub in_force_from: NaiveDate,
    /// The first change after the cutoff.
    pub changed: TextVersion,
    /// When it came out: printings from this day have another text.
    pub right_until: NaiveDate,
    /// Today's text.
    pub latest: TextVersion,
}

/// The errata that matter for a format with this `cutoff`: `None` when no
/// (dated) version came out after it, or none was in force at it (the card
/// isn't in the pool). A version with no date (`dates[i]` for `versions[i]`)
/// is ignored.
pub fn at_cutoff(
    versions: &[TextVersion],
    dates: &[Option<NaiveDate>],
    cutoff: NaiveDate,
) -> Option<Errata> {
    let dated: Vec<(&TextVersion, NaiveDate)> = versions
        .iter()
        .zip(dates)
        .filter_map(|(v, d)| Some((v, (*d)?)))
        .collect();

    // the dates may not follow the list's order (a reprint product): the
    // text in force is the latest one out by the cutoff, the change the
    // earliest one out after it
    let (in_force, in_force_from) = dated
        .iter()
        .filter(|(_, d)| *d <= cutoff)
        .max_by_key(|(_, d)| *d)?;
    let (changed, right_until) = dated
        .iter()
        .filter(|(_, d)| *d > cutoff)
        .min_by_key(|(_, d)| *d)?;
    let (latest, _) = dated.last()?;
    Some(Errata {
        in_force: (*in_force).clone(),
        in_force_from: *in_force_from,
        changed: (*changed).clone(),
        right_until: *right_until,
        latest: (*latest).clone(),
    })
}

impl Errata {
    /// Does a printing of this date have the text in force at the cutoff?
    pub fn is_right(&self, printed: NaiveDate) -> bool {
        printed >= self.in_force_from && printed < self.right_until
    }
}

/// Does a copy coded `code` (`DPRP-FR038`) have the right text? `None`
/// when its set's date is unknown (no code, an unknown set): not judged.
pub fn copy_is_right(code: &str, dates: &SetDates, errata: &Errata) -> Option<bool> {
    Some(errata.is_right(dates.of_code(code)?))
}

/// A printing with the right text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printing {
    pub code: String,
    pub set: String,
}

/// The printings of a card data's `card_sets` (`(code, set name)`) with the
/// right text, most recent first, one per set.
pub fn right_printings(
    card_sets: &[(String, String)],
    dates: &SetDates,
    errata: &Errata,
) -> Vec<Printing> {
    let mut found: Vec<(NaiveDate, Printing)> = Vec::new();
    for (code, set) in card_sets {
        let Some(date) = dates.of_name(set).or_else(|| dates.of_code(code)) else {
            continue;
        };
        if errata.is_right(date) && !found.iter().any(|(_, p)| p.set == *set) {
            found.push((
                date,
                Printing {
                    code: code.clone(),
                    set: set.clone(),
                },
            ));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.set.cmp(&b.1.set)));
    found.into_iter().map(|(_, p)| p).collect()
}

/// A card data's printings: `(code, set name)` of its `card_sets`.
pub fn card_printings(card: &Value) -> Vec<(String, String)> {
    card["card_sets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            Some((
                s["set_code"].as_str()?.to_string(),
                html_escape::decode_html_entities(s["set_name"].as_str()?.trim()).into_owned(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SANGAN: &str = "{{Card Errata
|name = Sangan
}}
== English ==
{{Errata table
|lore1 = If this card is sent from the field to the [[Graveyard]], you can add 1 monster with 1500 or less [[ATK]] from your [[Deck]] to your hand.
|cap1 = [[MRD-069]]<br />[[Metal Raiders]]
|lore2 = If this card is sent from the field to the Graveyard: <del>You can</del><ins>Add</ins> 1 monster with 1500 or less ATK from your Deck to your hand.<br />You can only use this effect of [[Sangan|'''Sangan''']] once per turn.
|cap2 = [[DPRP-EN038]]<br />[[Duelist Pack: Rivals of the Pharaoh]]
|lore3 = Sent from the field to the Graveyard: add 1 monster with 1500 or less ATK.
|cap3 = [[DPRP-EN038]]<br />[[Duelist Pack: Rivals of the Pharaoh]]<br />''[[Legendary Collection]]''
|note = ignored
}}
== Japanese ==
{{Errata table
|lore1 = Japanese text
|cap1 = [[SD1-JP001]]<br />[[Starter Deck]]
}}
";

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn dates() -> SetDates {
        SetDates::new([
            (
                "Metal Raiders".into(),
                Some("MRD".into()),
                Some(date(2002, 6, 26)),
            ),
            (
                "Duelist Pack: Rivals of the Pharaoh".into(),
                Some("DPRP".into()),
                Some(date(2016, 1, 29)),
            ),
            (
                "Legendary Collection".into(),
                Some("LC01".into()),
                Some(date(2013, 9, 17)),
            ),
            (
                "Pharaoh's Servant".into(),
                Some("PSV".into()),
                Some(date(2002, 10, 1)),
            ),
            ("Undated".into(), Some("UND".into()), None),
        ])
    }

    #[test]
    fn parses_the_english_table_only() {
        let versions = parse(SANGAN);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].code, "MRD-069");
        assert_eq!(versions[0].set, "Metal Raiders");
        assert_eq!(versions[0].reprint_in, None);
        assert_eq!(
            versions[2].reprint_in.as_deref(),
            Some("Legendary Collection")
        );
        assert!(!versions.iter().any(|v| v.text.contains("Japanese")));
    }

    #[test]
    fn cleans_the_markup() {
        let versions = parse(SANGAN);
        assert!(versions[0]
            .text
            .contains("Graveyard, you can add 1 monster"));
        assert!(!versions[0].text.contains("[["));
        assert_eq!(
            versions[1].text,
            "If this card is sent from the field to the Graveyard: Add 1 monster with 1500 or less ATK \
             from your Deck to your hand. You can only use this effect of Sangan once per turn."
        );
    }

    #[test]
    fn no_english_section_no_version() {
        assert!(parse(
            "== Japanese ==\n{{Errata table\n|lore1 = x\n|cap1 = [[A-1]]<br />[[A]]\n}}"
        )
        .is_empty());
        assert!(parse("== English ==\nno table here").is_empty());
        assert!(parse("").is_empty());
    }

    fn versions_and_dates() -> (Vec<TextVersion>, Vec<Option<NaiveDate>>) {
        let versions = parse(SANGAN);
        let d = dates();
        let dated = versions.iter().map(|v| d.of_version(v)).collect();
        (versions, dated)
    }

    #[test]
    fn a_version_is_dated_by_its_reprint_first() {
        let (_, dated) = versions_and_dates();
        assert_eq!(
            dated,
            vec![
                Some(date(2002, 6, 26)),
                Some(date(2016, 1, 29)),
                Some(date(2013, 9, 17)) // Legendary Collection, not DPRP
            ]
        );
    }

    #[test]
    fn a_cutoff_before_the_change_warns() {
        let (versions, dated) = versions_and_dates();
        // versions 2 and 3 are dated 2016 and 2013 here: both after 2010
        let errata = at_cutoff(&versions, &dated, date(2010, 3, 1)).unwrap();
        assert_eq!(errata.in_force.code, "MRD-069");
        assert_eq!(errata.in_force_from, date(2002, 6, 26));
        // the third version was reprinted (2013) before the second came out
        assert_eq!(errata.changed.text, versions[2].text);
        assert_eq!(errata.right_until, date(2013, 9, 17));
        assert_eq!(
            errata.latest.reprint_in.as_deref(),
            Some("Legendary Collection")
        );
    }

    #[test]
    fn a_cutoff_after_the_changes_has_nothing_to_warn() {
        let (versions, dated) = versions_and_dates();
        assert_eq!(at_cutoff(&versions, &dated, date(2020, 1, 1)), None);
    }

    #[test]
    fn a_cutoff_between_the_changes_warns_about_the_later_one() {
        let (versions, dated) = versions_and_dates();
        let errata = at_cutoff(&versions, &dated, date(2014, 1, 1)).unwrap();
        assert_eq!(errata.in_force.code, versions[2].code);
        assert_eq!(errata.in_force_from, date(2013, 9, 17));
        assert_eq!(errata.right_until, date(2016, 1, 29));
    }

    #[test]
    fn a_cutoff_before_the_card_existed_has_nothing_to_warn() {
        let (versions, dated) = versions_and_dates();
        assert_eq!(at_cutoff(&versions, &dated, date(2001, 1, 1)), None);
    }

    #[test]
    fn undated_versions_are_ignored() {
        let (versions, mut dated) = versions_and_dates();
        dated[1] = None;
        dated[2] = None;
        assert_eq!(at_cutoff(&versions, &dated, date(2010, 3, 1)), None);
    }

    #[test]
    fn the_right_printings_are_those_with_the_text_in_force() {
        let (versions, dated) = versions_and_dates();
        let errata = at_cutoff(&versions, &dated, date(2010, 3, 1)).unwrap();
        let sets = vec![
            ("MRD-069".to_string(), "Metal Raiders".to_string()),
            ("MRD-E069".to_string(), "Metal Raiders".to_string()),
            ("PSV-001".to_string(), "Pharaoh's Servant".to_string()),
            ("LC01-EN001".to_string(), "Legendary Collection".to_string()),
            (
                "DPRP-EN038".to_string(),
                "Duelist Pack: Rivals of the Pharaoh".to_string(),
            ),
            ("UND-001".to_string(), "Undated".to_string()),
        ];
        let right = right_printings(&sets, &dates(), &errata);
        let names: Vec<&str> = right.iter().map(|p| p.set.as_str()).collect();
        // the legendary collection (2013) and the 2016 pack carry a newer
        // text, the undated set is unknown; most recent first
        assert_eq!(names, ["Pharaoh's Servant", "Metal Raiders"]);
    }

    #[test]
    fn a_copy_is_judged_by_its_code() {
        let (versions, dated) = versions_and_dates();
        let errata = at_cutoff(&versions, &dated, date(2010, 3, 1)).unwrap();
        let d = dates();
        assert_eq!(copy_is_right("DPRP-FR038", &d, &errata), Some(false));
        assert_eq!(copy_is_right("MRD-069", &d, &errata), Some(true));
        assert_eq!(copy_is_right("ZZZ-FR001", &d, &errata), None);
        assert_eq!(copy_is_right("", &d, &errata), None);
    }

    #[test]
    fn reads_card_sets_from_card_data() {
        let card: Value = serde_json::from_str(
            r#"{"card_sets":[{"set_name":"Metal Raiders","set_code":"MRD-069","set_rarity":"Common"},{"set_name":"Legendary 5D&apos;s Decks","set_code":"LC5D-EN001"}]}"#,
        )
        .unwrap();
        assert_eq!(
            card_printings(&card),
            vec![
                ("MRD-069".to_string(), "Metal Raiders".to_string()),
                ("LC5D-EN001".to_string(), "Legendary 5D's Decks".to_string())
            ]
        );
    }
}
