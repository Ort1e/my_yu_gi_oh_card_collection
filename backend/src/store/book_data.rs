//! ISBN lookups: what Open Library (then Google Books) knows about a book,
//! remembered in `book_data` (migration 0007) like `card_translation`: a
//! book found is never asked again, a missing one only after
//! [`RECHECK_MISSING_DAYS`] (catalogues add books).
//!
//! Open Library: free, no key; `jscmd=data` has the authors, `jscmd=details`
//! the language, series and format. Google Books (no key) fills what Open
//! Library lacks or finds nothing for; its anonymous quota is often used up,
//! so its failures are only logged. Parsing the answers is
//! `domain::book`'s.

use std::time::{Duration, Instant};

use domain::book::{self, BookInfo};
use domain::isbn::Isbn;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::store::db::{begin_write, Pool};
use crate::web::error::{AppError, AppResult};

const OPEN_LIBRARY_URL: &str = "https://openlibrary.org/api/books";
const GOOGLE_BOOKS_URL: &str = "https://www.googleapis.com/books/v1/volumes";
/// Open Library asks API users to say who they are.
const USER_AGENT: &str = "my-books/0.1 (a personal book collection app)";
const TIMEOUT: Duration = Duration::from_secs(10);

/// How long "no catalogue knows this ISBN" is believed.
const RECHECK_MISSING_DAYS: i64 = 7;

/// Books are added one at a time; this only keeps a burst polite.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(250);
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::const_new(None);

async fn throttle() {
    let mut last = LAST_REQUEST.lock().await;
    if let Some(at) = *last {
        tokio::time::sleep_until((at + MIN_REQUEST_INTERVAL).into()).await;
    }
    *last = Some(Instant::now());
}

/// What the catalogues know about an ISBN, `None` when nothing (or no
/// title). Asks the network only when `book_data` has no fresh answer: call
/// it **before** opening a write transaction.
pub async fn lookup(
    pool: &Pool,
    http: &reqwest::Client,
    isbn: &Isbn,
) -> AppResult<Option<BookInfo>> {
    let known: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT json_data, checked_at FROM book_data WHERE isbn = ?")
            .bind(isbn.as_str())
            .fetch_optional(pool)
            .await?;
    if let Some((json, checked_at)) = known {
        if let Some(info) = json.and_then(|j| serde_json::from_str::<BookInfo>(&j).ok()) {
            return Ok(Some(info));
        }

        let today = chrono::Local::now().date_naive();
        let recent = checked_at
            .parse::<chrono::NaiveDate>()
            .is_ok_and(|d| (today - d).num_days() < RECHECK_MISSING_DAYS);
        if recent {
            return Ok(None);
        }
    }

    let found = fetch(http, isbn).await?;
    remember(pool, isbn, found.as_ref()).await?;
    Ok(found.map(|(info, _)| info))
}

async fn remember(pool: &Pool, isbn: &Isbn, found: Option<&(BookInfo, &str)>) -> AppResult<()> {
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
    .bind(found.map(|(_, source)| *source))
    .bind(json)
    .bind(chrono::Local::now().date_naive().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Ask the catalogues; the answer and where it came from.
async fn fetch(http: &reqwest::Client, isbn: &Isbn) -> AppResult<Option<(BookInfo, &'static str)>> {
    let mut info = open_library(http, isbn).await?;
    let mut source = "openlibrary";

    let complete = info
        .as_ref()
        .is_some_and(|i| !i.is_empty() && !i.authors.is_empty() && i.language.is_some());
    if !complete {
        match google_books(http, isbn).await {
            Ok(Some(google)) => match &mut info {
                Some(mine) if !mine.is_empty() => {
                    mine.fill_from(google);
                    source = "openlibrary+googlebooks";
                }
                _ => {
                    info = Some(google);
                    source = "googlebooks";
                }
            },
            Ok(None) => {}
            Err(why) => tracing::warn!(isbn = %isbn, "Google Books: {why}"),
        }
    }

    Ok(info.filter(|i| !i.is_empty()).map(|i| (i, source)))
}

async fn get_json(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    throttle().await;
    let response = http
        .get(url)
        .query(query)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    response.json().await.map_err(|e| e.to_string())
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

async fn google_books(http: &reqwest::Client, isbn: &Isbn) -> Result<Option<BookInfo>, String> {
    let answer = get_json(http, GOOGLE_BOOKS_URL, &[("q", &format!("isbn:{isbn}"))]).await?;
    Ok(book::from_google_books(&answer))
}
