//! ISBN lookups: what Open Library (then Google Books) knows about a book,
//! remembered in `book_data` (migration 0007) like `card_translation`: a
//! book found is never asked again, a missing one only after
//! [`RECHECK_MISSING_DAYS`] (catalogues add books).
//!
//! Open Library: free, no key; `jscmd=data` has the authors, `jscmd=details`
//! the language, series and format. Google Books (no key) fills what Open
//! Library lacks or finds nothing for; its anonymous quota is often used up
//! (429 Too Many Requests). When Open Library knows nothing and Google Books
//! couldn't answer, nothing is remembered: the answer is
//! [`Found::Unavailable`], asked again next time. Parsing the answers is
//! `books_domain::book`'s.

use std::time::Duration;

use books_domain::book::{self, BookInfo};
use books_domain::isbn::Isbn;
use serde_json::Value;

use common::db::{begin_write, is_fresh, today, Pool};
use common::http::Throttle;
use common::web::error::{AppError, AppResult};

const OPEN_LIBRARY_URL: &str = "https://openlibrary.org/api/books";
const GOOGLE_BOOKS_URL: &str = "https://www.googleapis.com/books/v1/volumes";
/// Open Library asks API users to say who they are.
const USER_AGENT: &str = "my-books/0.1 (a personal book collection app)";
const TIMEOUT: Duration = Duration::from_secs(10);

/// How long "no catalogue knows this ISBN" is believed.
const RECHECK_MISSING_DAYS: i64 = 7;

/// Books are added one at a time; this only keeps a burst polite.
static CATALOGUES: Throttle = Throttle::new(Duration::from_millis(250));
/// What a lookup found.
#[derive(Debug)]
pub enum Found {
    Book(BookInfo),
    /// No catalogue knows it (or gives no title).
    Unknown,
    /// Open Library doesn't know it and Google Books couldn't answer (its
    /// quota): nothing remembered, to ask again later.
    Unavailable,
}

/// What the catalogues know about an ISBN. Asks the network only when
/// `book_data` has no fresh answer: call it **before** opening a write
/// transaction.
pub async fn lookup(pool: &Pool, http: &reqwest::Client, isbn: &Isbn) -> AppResult<Found> {
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

    let (found, google_failed) = fetch(http, isbn).await?;
    if found.is_none() && google_failed {
        return Ok(Found::Unavailable);
    }
    remember(pool, isbn, found.as_ref()).await?;
    Ok(found.map_or(Found::Unknown, |(info, _)| Found::Book(info)))
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
    .bind(today().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Ask the catalogues: the answer and where it came from, and whether
/// Google Books failed.
async fn fetch(
    http: &reqwest::Client,
    isbn: &Isbn,
) -> AppResult<(Option<(BookInfo, &'static str)>, bool)> {
    let mut info = open_library(http, isbn).await?;
    let mut source = "openlibrary";
    let mut google_failed = false;

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
            Err(why) => {
                tracing::warn!(isbn = %isbn, "Google Books: {why}");
                google_failed = true;
            }
        }
    }

    Ok((
        info.filter(|i| !i.is_empty()).map(|i| (i, source)),
        google_failed,
    ))
}

async fn get_json(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    CATALOGUES.wait().await;
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
