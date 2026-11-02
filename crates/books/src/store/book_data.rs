//! ISBN lookups: what Open Library, Google Books and the BnF know about a
//! book, remembered in `book_data` (migration 0007) like `card_translation`:
//! a book found is never asked again, a missing one only after
//! [`RECHECK_MISSING_DAYS`] (catalogues add books).
//!
//! Open Library first: free, no key; `jscmd=data` has the authors,
//! `jscmd=details` the language, series and format. Then, for what it lacks
//! (or when it knows nothing), Google Books: with the app's key
//! (`AppState::google_books_key`, 1000 lookups a day: the app stops at
//! [`GOOGLE_DAILY_LIMIT`], counted in memory); without one its
//! shared keyless quota is nearly always used up (429). Then the BnF (the
//! French national library's catalogue, free, no key), which knows the
//! French editions best: for a French ISBN (`Isbn::is_french`) it is asked
//! first. When nothing is found and a catalogue couldn't answer, nothing is
//! remembered: the answer is [`Found::Unavailable`], asked again next time.
//! Parsing the answers is `books_domain::book`'s.
//!
//! The key goes in the address: the errors are logged without it
//! (`reqwest::Error::without_url`).

use std::sync::Mutex;
use std::time::Duration;

use books_domain::book::{self, BookInfo};
use books_domain::isbn::Isbn;
use chrono::{NaiveDate, Utc};
use serde_json::Value;

use common::db::{begin_write, is_fresh, today, Pool};
use common::http::Throttle;
use common::web::error::{AppError, AppResult};
use common::AppState;

const OPEN_LIBRARY_URL: &str = "https://openlibrary.org/api/books";
const GOOGLE_BOOKS_URL: &str = "https://www.googleapis.com/books/v1/volumes";
const BNF_URL: &str = "https://catalogue.bnf.fr/api/SRU";
/// Open Library asks API users to say who they are.
const USER_AGENT: &str = "my-books/0.1 (a personal book collection app)";
const TIMEOUT: Duration = Duration::from_secs(10);

/// How long "no catalogue knows this ISBN" is believed.
const RECHECK_MISSING_DAYS: i64 = 7;

/// Google Books' lookups a day with our key: its free quota is 1000, so
/// this leaves a margin for the lookups not counted here (the count is in
/// memory: a restart starts it again; a dev server with the same key has
/// its own).
const GOOGLE_DAILY_LIMIT: u32 = 800;
/// Google's day (`google_day`) and its lookups so far.
static GOOGLE_LOOKUPS: Mutex<(NaiveDate, u32)> = Mutex::new((NaiveDate::MIN, 0));

/// Books are added one at a time; this only keeps a burst polite.
static CATALOGUES: Throttle = Throttle::new(Duration::from_millis(250));
/// What a lookup found.
#[derive(Debug)]
pub enum Found {
    Book(BookInfo),
    /// No catalogue knows it (or gives no title).
    Unknown,
    /// Nothing found, and a catalogue couldn't answer (Google Books' quota,
    /// the network): nothing remembered, to ask again later.
    Unavailable,
}

/// What the catalogues know about an ISBN. Asks the network only when
/// `book_data` has no fresh answer: call it **before** opening a write
/// transaction.
pub async fn lookup(state: &AppState, isbn: &Isbn) -> AppResult<Found> {
    let pool = &state.pool;
    let known: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT json_data, checked_at FROM book_data WHERE isbn = ?")
            .bind(isbn.as_str())
            .fetch_optional(pool)
            .await?;
    if let Some((json, checked_at)) = known {
        if let Some(info) = json.and_then(|j| serde_json::from_str::<BookInfo>(&j).ok()) {
            return Ok(Found::Book(info));
        }

        if is_fresh(&checked_at, RECHECK_MISSING_DAYS, today()) {
            return Ok(Found::Unknown);
        }
    }

    let (found, failed) = fetch(state, isbn).await;
    if found.is_none() && failed {
        return Ok(Found::Unavailable);
    }
    remember(pool, isbn, found.as_ref()).await?;
    Ok(found.map_or(Found::Unknown, |(info, _)| Found::Book(info)))
}

async fn remember(pool: &Pool, isbn: &Isbn, found: Option<&(BookInfo, String)>) -> AppResult<()> {
    let json = match found {
        Some((info, _)) => Some(
            serde_json::to_string(info)
                .map_err(|e| AppError::Internal(format!("book data: {e}")))?,
        ),
        None => None,
    };

    let mut tx = begin_write(pool).await?;
    sqlx::query(
        "INSERT INTO book_data (isbn, source, json_data, checked_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (isbn) DO UPDATE
             SET source = excluded.source, json_data = excluded.json_data,
                 checked_at = excluded.checked_at",
    )
    .bind(isbn.as_str())
    .bind(found.map(|(_, source)| source.as_str()))
    .bind(json)
    .bind(today().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Whether an answer has what the form needs most: no other catalogue is
/// asked then.
fn complete(info: &Option<BookInfo>) -> bool {
    info.as_ref()
        .is_some_and(|i| !i.is_empty() && !i.authors.is_empty() && i.language.is_some())
}

/// What the catalogues asked so far found, and which ones (`openlibrary+bnf`).
#[derive(Default)]
struct Answer {
    info: Option<BookInfo>,
    sources: Vec<&'static str>,
    /// A catalogue couldn't answer.
    failed: bool,
}

impl Answer {
    fn add(&mut self, source: &'static str, found: Result<Option<BookInfo>, String>) {
        let theirs = match found {
            Ok(Some(theirs)) => theirs,
            Ok(None) => return,
            Err(why) => {
                tracing::warn!("{source}: {why}");
                self.failed = true;
                return;
            }
        };

        match &mut self.info {
            Some(mine) if !mine.is_empty() => mine.fill_from(theirs),
            _ if theirs.is_empty() => return,
            _ => {
                self.info = Some(theirs);
                self.sources.clear();
            }
        }
        self.sources.push(source);
    }
}

/// Ask the catalogues, in turn while the answer isn't complete (the BnF
/// first for a French ISBN, last for the others): the answer and where it
/// came from, and whether one of them failed.
async fn fetch(state: &AppState, isbn: &Isbn) -> (Option<(BookInfo, String)>, bool) {
    let http = &state.http;
    let mut answer = Answer::default();
    let french = isbn.is_french();
    if french {
        answer.add("bnf", bnf(http, isbn).await);
    }

    if !complete(&answer.info) {
        let found = open_library(http, isbn).await.map_err(|e| e.to_string());
        answer.add("openlibrary", found);
    }
    if !complete(&answer.info) {
        let found = google_books(http, state.google_books_key.as_deref(), isbn).await;
        answer.add("googlebooks", found);
    }
    if !french && !complete(&answer.info) {
        answer.add("bnf", bnf(http, isbn).await);
    }

    let source = answer.sources.join("+");
    let found = answer.info.filter(|i| !i.is_empty()).map(|i| (i, source));
    (found, answer.failed)
}

/// A catalogue's answer (errors without the address: it may hold the key).
async fn get(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<reqwest::Response, String> {
    CATALOGUES.wait().await;
    let response = http
        .get(url)
        .query(query)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    Ok(response)
}

async fn get_json(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    let response = get(http, url, query).await?;
    response
        .json()
        .await
        .map_err(|e| e.without_url().to_string())
}

/// Open Library, asked by ISBN-13 and ISBN-10 (some editions are only
/// known by one).
async fn open_library(http: &reqwest::Client, isbn: &Isbn) -> AppResult<Option<BookInfo>> {
    let mut keys = vec![format!("ISBN:{isbn}")];
    keys.extend(isbn.isbn10().map(|i| format!("ISBN:{i}")));
    let bibkeys = keys.join(",");

    let ask = |jscmd: &'static str| {
        let bibkeys = bibkeys.clone();
        async move {
            get_json(
                http,
                OPEN_LIBRARY_URL,
                &[("bibkeys", &bibkeys), ("jscmd", jscmd), ("format", "json")],
            )
            .await
            .map_err(|why| AppError::Internal(format!("Open Library: {why}")))
        }
    };
    let data = ask("data").await?;
    let details = ask("details").await?;

    let first = |answer: &Value| keys.iter().find_map(|k| answer.get(k)).cloned();
    let data = first(&data);
    let details = first(&details).and_then(|d| d.get("details").cloned());
    Ok(book::from_open_library(data.as_ref(), details.as_ref()))
}

/// Google's quota day: it starts at midnight Pacific time, taken as UTC−8
/// (in summer the count starts again an hour after Google's: never early).
fn google_day() -> NaiveDate {
    (Utc::now() - chrono::Duration::hours(8)).date_naive()
}

/// Count one more lookup on `day`; `false` when the day's limit is reached.
fn take_lookup(count: &mut (NaiveDate, u32), day: NaiveDate, limit: u32) -> bool {
    if count.0 != day {
        *count = (day, 0);
    }
    if count.1 >= limit {
        return false;
    }

    count.1 += 1;
    true
}

async fn google_books(
    http: &reqwest::Client,
    key: Option<&str>,
    isbn: &Isbn,
) -> Result<Option<BookInfo>, String> {
    if key.is_some() {
        let mut count = GOOGLE_LOOKUPS.lock().unwrap_or_else(|e| e.into_inner());
        if !take_lookup(&mut count, google_day(), GOOGLE_DAILY_LIMIT) {
            return Err(format!(
                "{GOOGLE_DAILY_LIMIT} lookups today already: not asked, to stay under its quota"
            ));
        }
    }

    let q = format!("isbn:{isbn}");
    let mut query = vec![("q", q.as_str())];
    query.extend(key.map(|key| ("key", key)));
    let answer = get_json(http, GOOGLE_BOOKS_URL, &query).await?;
    Ok(book::from_google_books(&answer))
}

/// The BnF's catalogue (SRU), its Dublin Core record of the ISBN.
async fn bnf(http: &reqwest::Client, isbn: &Isbn) -> Result<Option<BookInfo>, String> {
    let query = format!("bib.isbn adj \"{isbn}\"");
    let response = get(
        http,
        BNF_URL,
        &[
            ("version", "1.2"),
            ("operation", "searchRetrieve"),
            ("query", &query),
            ("recordSchema", "dublincore"),
            ("maximumRecords", "1"),
        ],
    )
    .await?;
    let xml = response
        .text()
        .await
        .map_err(|e| e.without_url().to_string())?;
    Ok(book::from_bnf(&xml))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_lookups_stop_at_the_daily_limit() {
        let monday = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let tuesday = monday.succ_opt().unwrap();
        let mut count = (NaiveDate::MIN, 0);
        assert!(take_lookup(&mut count, monday, 2));
        assert!(take_lookup(&mut count, monday, 2));
        assert!(!take_lookup(&mut count, monday, 2));
        assert_eq!(count, (monday, 2));
        // a new day, a new count
        assert!(take_lookup(&mut count, tuesday, 2));
        assert_eq!(count, (tuesday, 1));
    }
}
