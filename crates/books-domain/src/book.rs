//! Books (new, not in Django): what an ISBN lookup answers, cleaned up for
//! the add form, and the small rules of the book pages (author sort names,
//! formats, missing volumes of a series).
//!
//! Lookups: Open Library (`jscmd=data` has the authors, `jscmd=details` the
//! language, series and format), Google Books as a fallback. Their answers
//! are free text of varying quality: everything here is a best guess that
//! the form shows before saving.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a lookup found about one ISBN. Every field may be missing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookInfo {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub year: Option<i64>,
    /// Two letters when known (`en`), else as given (`/languages/xyz` → `xyz`).
    pub language: Option<String>,
    pub pages: Option<i64>,
    pub series: Option<String>,
    pub volume: Option<i64>,
    pub format: Option<BookFormat>,
}

impl BookInfo {
    /// Nothing useful: no title (the one field the form requires).
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
    }

    /// Fill the fields this answer lacks from another one.
    pub fn fill_from(&mut self, other: BookInfo) {
        fn fill<T>(mine: &mut Option<T>, theirs: Option<T>) {
            if mine.is_none() {
                *mine = theirs;
            }
        }

        fill(&mut self.title, other.title);
        if self.authors.is_empty() {
            self.authors = other.authors;
        }
        fill(&mut self.publisher, other.publisher);
        fill(&mut self.year, other.year);
        fill(&mut self.language, other.language);
        fill(&mut self.pages, other.pages);
        if self.series.is_none() {
            self.series = other.series;
            self.volume = other.volume;
        }
        fill(&mut self.format, other.format);
    }
}

/// `book.format` choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BookFormat {
    Paperback,
    Hardcover,
    Ebook,
    Audiobook,
}

impl BookFormat {
    pub const ALL: [BookFormat; 4] = [
        BookFormat::Paperback,
        BookFormat::Hardcover,
        BookFormat::Ebook,
        BookFormat::Audiobook,
    ];

    /// Value stored in the database, and shown.
    pub fn label(self) -> &'static str {
        match self {
            BookFormat::Paperback => "Paperback",
            BookFormat::Hardcover => "Hardcover",
            BookFormat::Ebook => "Ebook",
            BookFormat::Audiobook => "Audiobook",
        }
    }

    pub fn from_label(label: &str) -> Option<BookFormat> {
        BookFormat::ALL.into_iter().find(|f| f.label() == label)
    }

    /// A catalogue's free text: "Mass Market Paperback", "hardback"…
    pub fn guess(text: &str) -> Option<BookFormat> {
        let text = text.to_lowercase();
        if text.contains("audio") {
            Some(BookFormat::Audiobook)
        } else if text.contains("ebook") || text.contains("e-book") || text.contains("kindle") {
            Some(BookFormat::Ebook)
        } else if text.contains("hardcover") || text.contains("hardback") {
            Some(BookFormat::Hardcover)
        } else if text.contains("paperback") || text.contains("softcover") {
            Some(BookFormat::Paperback)
        } else {
            None
        }
    }
}

// ---------- names ----------

/// An author's name as stored: trimmed, inner spaces collapsed, so the same
/// author isn't created twice from a typo in spacing.
pub fn clean_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The key authors are sorted by: surname first, lower case
/// (`Robin Hobb` → `hobb robin`). A name typed "Surname, First" is kept in
/// that order. Particles stay with the first names (`Ursula K. Le Guin` →
/// `guin ursula k. le`): good enough for a home shelf.
pub fn author_sort_name(name: &str) -> String {
    let name = clean_name(name).to_lowercase();
    if let Some((surname, first)) = name.split_once(',') {
        return clean_name(&format!("{surname} {first}"));
    }

    match name.rsplit_once(' ') {
        Some((first, surname)) => format!("{surname} {first}"),
        None => name,
    }
}

/// The authors typed in the form, comma-separated (`Neil Gaiman, Terry
/// Pratchett`), cleaned, without duplicates, in order.
pub fn split_authors(text: &str) -> Vec<String> {
    let mut authors: Vec<String> = Vec::new();
    for name in text.split([',', ';']).map(clean_name) {
        if !name.is_empty() && !authors.iter().any(|a| a.eq_ignore_ascii_case(&name)) {
            authors.push(name);
        }
    }
    authors
}

// ---------- series ----------

static VOLUME_AFTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(.*?)[\s,;:(]*(?:#|\bno\.?|\bbk\.?|\bbook|\bvol\.?|\bvolume|\btome|\bpart)\s*(\d+|[a-z]+)\)?\.?$",
    )
    .unwrap()
});
/// A bare number after a comma or a spaced dash: `Red Rising, 3`,
/// `Robert Langdon -- 5` (not `Catch-22`: no space before its dash).
static VOLUME_TRAILING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+?)(?:\s*[,;]|\s+[-–—#]+)\s*(\d{1,3})$").unwrap());
static VOLUME_BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:book|volume|part)\s+(\d+|[a-z]+)\s+(?:of|in)\s+(?:the\s+)?(.+)$").unwrap()
});

/// A volume number written in digits or words (`3`, `three`, `third`).
fn volume_number(text: &str) -> Option<i64> {
    if let Ok(n) = text.parse::<i64>() {
        return (1..=999).contains(&n).then_some(n);
    }

    const WORDS: [(&str, &str); 12] = [
        ("one", "first"),
        ("two", "second"),
        ("three", "third"),
        ("four", "fourth"),
        ("five", "fifth"),
        ("six", "sixth"),
        ("seven", "seventh"),
        ("eight", "eighth"),
        ("nine", "ninth"),
        ("ten", "tenth"),
        ("eleven", "eleventh"),
        ("twelve", "twelfth"),
    ];
    let text = text.to_lowercase();
    WORDS
        .iter()
        .position(|(cardinal, ordinal)| text == *cardinal || text == *ordinal)
        .map(|i| i as i64 + 1)
}

/// A catalogue's series text, split into the series name and the volume:
/// `The Farseer ; bk. 1`, `Discworld #3`, `Book One of A Song of Ice and
/// Fire`, `Stormlight Archive (Book 2)`, `Red Rising, 3`, `Robert Langdon
/// -- 5`. Without a recognisable volume, the whole text is the name; the
/// separators left at its end (`--`, `,`) are dropped.
pub fn parse_series(text: &str) -> (String, Option<i64>) {
    let text = clean_name(text);

    if let Some(c) = VOLUME_BEFORE.captures(&text) {
        if let Some(volume) = volume_number(&c[1]) {
            return (clean_name(&c[2]), Some(volume));
        }
    }

    for pattern in [&VOLUME_AFTER, &VOLUME_TRAILING] {
        if let Some(c) = pattern.captures(&text) {
            let name = trim_separators(&c[1]);
            if let (false, Some(volume)) = (name.is_empty(), volume_number(&c[2])) {
                return (name.to_string(), Some(volume));
            }
        }
    }

    (trim_separators(&text).to_string(), None)
}

/// A series name without the separators a catalogue left at its end:
/// `Robert Langdon --` → `Robert Langdon`.
fn trim_separators(name: &str) -> &str {
    name.trim_end_matches([',', ';', ':', ' ', '-', '–', '—', '#', '/', '|'])
        .trim()
}

/// The volumes missing from a series: the gaps between 1 and the highest
/// volume owned (`[1, 2, 4, 7]` → `[3, 5, 6]`). Volumes after the last one
/// owned aren't known.
pub fn missing_volumes(owned: &[i64]) -> Vec<i64> {
    let Some(&last) = owned.iter().max() else {
        return Vec::new();
    };

    (1..last).filter(|v| !owned.contains(v)).collect()
}

/// A language code as stored (`en`, from the lookups) in words, for the
/// pages: `English`; `None` for one not listed (shown as it is).
pub fn language_name(code: &str) -> Option<&'static str> {
    Some(match code.trim().to_ascii_lowercase().as_str() {
        "en" => "English",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        "ru" => "Russian",
        "la" => "Latin",
        _ => return None,
    })
}

// ---------- lookup answers ----------

static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(1[5-9]\d\d|20\d\d)\b").unwrap());

/// The year in a free-text date: `1995`, `April 10th 2018`, `2012-05-01`.
pub fn year_of(date: &str) -> Option<i64> {
    YEAR.captures(date).and_then(|c| c[1].parse().ok())
}

/// MARC language codes (Open Library) to the two letters Google uses.
fn language_code(code: &str) -> String {
    let code = code.rsplit('/').next().unwrap_or(code).to_lowercase();
    let two = match code.as_str() {
        "eng" => "en",
        "fre" | "fra" => "fr",
        "ger" | "deu" => "de",
        "spa" => "es",
        "ita" => "it",
        "jpn" => "ja",
        "por" => "pt",
        "dut" | "nld" => "nl",
        _ => return code,
    };
    two.to_string()
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(clean_name).filter(|s| !s.is_empty())
}

fn positive(value: &Value) -> Option<i64> {
    value.as_i64().filter(|n| *n > 0)
}

/// Open Library's answer for one ISBN: the value under `ISBN:<isbn>` of
/// `api/books?jscmd=data` and of `jscmd=details` (its `details` object).
/// `None` when both are missing.
pub fn from_open_library(data: Option<&Value>, details: Option<&Value>) -> Option<BookInfo> {
    if data.is_none() && details.is_none() {
        return None;
    }

    let mut info = BookInfo::default();

    if let Some(data) = data {
        info.title = text(&data["title"]);
        info.authors = data["authors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| text(&a["name"]))
            .collect();
        info.publisher = data["publishers"].get(0).and_then(|p| text(&p["name"]));
        info.year = data["publish_date"].as_str().and_then(year_of);
        info.pages = positive(&data["number_of_pages"]);
    }

    if let Some(details) = details {
        let mut more = BookInfo {
            title: text(&details["title"]),
            authors: details["authors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| text(&a["name"]))
                .collect(),
            publisher: details["publishers"].get(0).and_then(text),
            year: details["publish_date"].as_str().and_then(year_of),
            language: details["languages"]
                .get(0)
                .and_then(|l| l["key"].as_str())
                .map(language_code),
            pages: positive(&details["number_of_pages"]),
            format: details["physical_format"]
                .as_str()
                .and_then(BookFormat::guess),
            ..BookInfo::default()
        };
        if let Some(series) = details["series"].get(0).and_then(text) {
            let (name, volume) = parse_series(&series);
            more.series = Some(name);
            more.volume = volume;
        }
        info.fill_from(more);
    }

    Some(info)
}

/// Google Books' answer: the `volumeInfo` of the first item of
/// `volumes?q=isbn:<isbn>`, `None` when there is no item.
pub fn from_google_books(answer: &Value) -> Option<BookInfo> {
    let volume = answer["items"].get(0)?.get("volumeInfo")?;
    Some(BookInfo {
        title: text(&volume["title"]),
        authors: volume["authors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(text)
            .collect(),
        publisher: text(&volume["publisher"]),
        year: volume["publishedDate"].as_str().and_then(year_of),
        language: text(&volume["language"]).map(|l| language_code(&l)),
        pages: positive(&volume["pageCount"]),
        ..BookInfo::default()
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn author_names() {
        assert_eq!(clean_name("  Robin   Hobb "), "Robin Hobb");
        assert_eq!(author_sort_name("Robin Hobb"), "hobb robin");
        assert_eq!(author_sort_name("Hobb, Robin"), "hobb robin");
        assert_eq!(
            author_sort_name("George R. R. Martin"),
            "martin george r. r."
        );
        assert_eq!(author_sort_name("Homer"), "homer");
        assert_eq!(
            split_authors("Neil Gaiman, Terry  Pratchett;neil gaiman, "),
            ["Neil Gaiman", "Terry Pratchett"]
        );
        assert!(split_authors(" , ").is_empty());
    }

    #[test]
    fn series_and_volume() {
        let cases = [
            ("The Farseer ; bk. 1", "The Farseer", Some(1)),
            ("Discworld #3", "Discworld", Some(3)),
            ("Discworld, no. 12", "Discworld", Some(12)),
            ("Stormlight Archive (Book 2)", "Stormlight Archive", Some(2)),
            ("The Expanse, Vol. 4", "The Expanse", Some(4)),
            (
                "Book One of A Song of Ice and Fire",
                "A Song of Ice and Fire",
                Some(1),
            ),
            ("Book Three of the Broken Earth", "Broken Earth", Some(3)),
            ("The Locked Tomb", "The Locked Tomb", None),
            ("The Farseer", "The Farseer", None),
            // "Book" is part of the name, no number after it
            ("The Book of the New Sun", "The Book of the New Sun", None),
            // Open Library's (seen 2026-10-30)
            ("Robert Langdon -- bk. 5", "Robert Langdon", Some(5)),
            ("Robert Langdon -- 5", "Robert Langdon", Some(5)),
            ("Red Rising, 3", "Red Rising", Some(3)),
            ("Robert Langdon --", "Robert Langdon", None),
            // a number that is part of the name
            ("Catch-22", "Catch-22", None),
            ("Fahrenheit 451", "Fahrenheit 451", None),
        ];
        for (text, name, volume) in cases {
            assert_eq!(parse_series(text), (name.to_string(), volume), "{text}");
        }
    }

    #[test]
    fn missing_volumes_are_the_gaps() {
        assert_eq!(missing_volumes(&[1, 2, 4, 7]), [3, 5, 6]);
        assert_eq!(missing_volumes(&[3]), [1, 2]);
        assert!(missing_volumes(&[1, 2, 3]).is_empty());
        assert!(missing_volumes(&[]).is_empty());
    }

    #[test]
    fn formats_and_years() {
        assert_eq!(
            BookFormat::guess("Mass Market Paperback"),
            Some(BookFormat::Paperback)
        );
        assert_eq!(BookFormat::guess("Hardback"), Some(BookFormat::Hardcover));
        assert_eq!(BookFormat::guess("Audio CD"), Some(BookFormat::Audiobook));
        assert_eq!(BookFormat::guess("Board book"), None);
        assert_eq!(BookFormat::from_label("Ebook"), Some(BookFormat::Ebook));
        assert_eq!(year_of("April 10th 2018"), Some(2018));
        assert_eq!(year_of("2012-05-01"), Some(2012));
        assert_eq!(year_of("n.d."), None);
    }

    /// Synthetic answers shaped like Open Library's.
    #[test]
    fn reads_open_library() {
        let data = json!({
            "title": "A Game of Thrones",
            "authors": [{"url": "…", "name": "George R. R. Martin"}],
            "publishers": [{"name": "Harper Voyager"}],
            "publish_date": "2012",
            "number_of_pages": 801
        });
        let details = json!({
            "title": "A Game of Thrones",
            "publishers": ["Harper Voyager"],
            "languages": [{"key": "/languages/eng"}],
            "series": ["Book One of A Song of Ice and Fire"],
            "physical_format": "Paperback"
        });

        let info = from_open_library(Some(&data), Some(&details)).unwrap();
        assert_eq!(
            info,
            BookInfo {
                title: Some("A Game of Thrones".into()),
                authors: vec!["George R. R. Martin".into()],
                publisher: Some("Harper Voyager".into()),
                year: Some(2012),
                language: Some("en".into()),
                pages: Some(801),
                series: Some("A Song of Ice and Fire".into()),
                volume: Some(1),
                format: Some(BookFormat::Paperback),
            }
        );

        // details only (an edition `jscmd=data` doesn't know)
        let info = from_open_library(None, Some(&details)).unwrap();
        assert_eq!(info.title.as_deref(), Some("A Game of Thrones"));
        assert!(info.authors.is_empty());
        assert!(from_open_library(None, None).is_none());
    }

    #[test]
    fn reads_google_books() {
        let answer = json!({"totalItems": 1, "items": [{"volumeInfo": {
            "title": "Circe",
            "authors": ["Madeline Miller"],
            "publisher": "Little, Brown",
            "publishedDate": "2018-04-10",
            "pageCount": 393,
            "language": "en"
        }}]});
        let info = from_google_books(&answer).unwrap();
        assert_eq!(info.title.as_deref(), Some("Circe"));
        assert_eq!(info.authors, ["Madeline Miller"]);
        assert_eq!((info.year, info.pages), (Some(2018), Some(393)));
        assert_eq!(info.language.as_deref(), Some("en"));
        assert!(from_google_books(&json!({"totalItems": 0})).is_none());
    }

    #[test]
    fn answers_fill_each_other() {
        let mut info = BookInfo {
            title: Some("Circe".into()),
            ..BookInfo::default()
        };
        info.fill_from(BookInfo {
            title: Some("CIRCE".into()),
            authors: vec!["Madeline Miller".into()],
            pages: Some(393),
            ..BookInfo::default()
        });
        assert_eq!(
            info.title.as_deref(),
            Some("Circe"),
            "the first answer wins"
        );
        assert_eq!(info.authors, ["Madeline Miller"]);
        assert_eq!(info.pages, Some(393));
        assert!(!info.is_empty() && BookInfo::default().is_empty());
    }

    #[test]
    fn language_names() {
        assert_eq!(language_name("en"), Some("English"));
        assert_eq!(language_name(" FR "), Some("French"));
        assert_eq!(language_name("xyz"), None);
    }
}
