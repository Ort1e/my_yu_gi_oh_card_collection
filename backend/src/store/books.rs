//! Book-row writes, shared by the book forms and the book advent calendar:
//! a book with its authors (in order) and its series, created on the way
//! when new. Authors and series left without books are deleted.

use domain::book::{author_sort_name, BookFormat, BookInfo};
use domain::isbn::Isbn;
use sqlx::SqliteConnection;

use crate::store::db::today;

/// The fields of a book, validated (`routes::books::form` builds them from
/// the form, [`BookFields::from_info`] from a lookup).
#[derive(Debug, Clone, Default)]
pub struct BookFields {
    pub isbn: Option<Isbn>,
    pub title: String,
    /// Cleaned (`domain::book::split_authors`), in order.
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub year: Option<i64>,
    pub language: Option<String>,
    pub pages: Option<i64>,
    pub series: Option<String>,
    pub volume: Option<i64>,
    pub format: Option<BookFormat>,
    pub notes: Option<String>,
}

impl BookFields {
    /// A book as an ISBN lookup describes it (the advent calendar creates
    /// books this way). `None` without a title.
    pub fn from_info(isbn: Isbn, info: BookInfo) -> Option<BookFields> {
        Some(BookFields {
            isbn: Some(isbn),
            title: info.title?,
            authors: info.authors,
            publisher: info.publisher,
            year: info.year,
            language: info.language,
            pages: info.pages,
            series: info.series,
            volume: info.volume,
            format: info.format,
            notes: None,
        })
    }
}

/// The book with this ISBN, if any.
pub async fn with_isbn(conn: &mut SqliteConnection, isbn: &Isbn) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT id FROM book WHERE isbn = ?")
        .bind(isbn.as_str())
        .fetch_optional(conn)
        .await
}

/// Insert a book (added today). Call inside a write transaction.
pub async fn insert(conn: &mut SqliteConnection, book: &BookFields) -> sqlx::Result<i64> {
    let series = series_id(conn, book.series.as_deref()).await?;
    let id = sqlx::query(
        "INSERT INTO book (isbn, title, publisher, published_year, language, page_count,
                           series_id, volume, format, notes, added_on)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book.isbn.as_ref().map(Isbn::as_str))
    .bind(&book.title)
    .bind(&book.publisher)
    .bind(book.year)
    .bind(&book.language)
    .bind(book.pages)
    .bind(series)
    .bind(series.and(book.volume))
    .bind(book.format.map(BookFormat::label))
    .bind(&book.notes)
    .bind(today().to_string())
    .execute(&mut *conn)
    .await?
    .last_insert_rowid();

    set_authors(conn, id, &book.authors).await?;
    Ok(id)
}

/// Update a book; `false` when there is none with this id.
pub async fn update(conn: &mut SqliteConnection, id: i64, book: &BookFields) -> sqlx::Result<bool> {
    let series = series_id(conn, book.series.as_deref()).await?;
    let updated = sqlx::query(
        "UPDATE book SET isbn = ?, title = ?, publisher = ?, published_year = ?, language = ?,
                         page_count = ?, series_id = ?, volume = ?, format = ?, notes = ?
         WHERE id = ?",
    )
    .bind(book.isbn.as_ref().map(Isbn::as_str))
    .bind(&book.title)
    .bind(&book.publisher)
    .bind(book.year)
    .bind(&book.language)
    .bind(book.pages)
    .bind(series)
    .bind(series.and(book.volume))
    .bind(book.format.map(BookFormat::label))
    .bind(&book.notes)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok(false);
    }

    set_authors(conn, id, &book.authors).await?;
    delete_orphans(conn).await?;
    Ok(true)
}

/// Delete a book. `false` when there is none with this id. Fails on the
/// foreign key while the book is behind an advent calendar door: check
/// first ([`calendar_days`]).
pub async fn delete(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<bool> {
    let deleted = sqlx::query("DELETE FROM book WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    delete_orphans(conn).await?;
    Ok(deleted > 0)
}

/// The advent calendar days a book is behind: `(year, day)`.
pub async fn calendar_days(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<Vec<(i64, i64)>> {
    sqlx::query_as(
        "SELECT c.year, e.day FROM book_advent_calendar_entry e
         JOIN book_advent_calendar c ON c.id = e.calendar_id
         WHERE e.book_id = ? ORDER BY c.year, e.day",
    )
    .bind(id)
    .fetch_all(conn)
    .await
}

/// The id of a series by name (case-insensitive), created when new.
async fn series_id(conn: &mut SqliteConnection, name: Option<&str>) -> sqlx::Result<Option<i64>> {
    let Some(name) = name else {
        return Ok(None);
    };

    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM book_series WHERE name = ? COLLATE NOCASE")
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?;
    if existing.is_some() {
        return Ok(existing);
    }

    let id = sqlx::query("INSERT INTO book_series (name) VALUES (?)")
        .bind(name)
        .execute(&mut *conn)
        .await?
        .last_insert_rowid();
    Ok(Some(id))
}

/// Replace the authors of a book, creating the new ones (an existing author
/// is matched case-insensitively and keeps its spelling).
async fn set_authors(
    conn: &mut SqliteConnection,
    book_id: i64,
    authors: &[String],
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM book_book_author WHERE book_id = ?")
        .bind(book_id)
        .execute(&mut *conn)
        .await?;

    for (position, name) in authors.iter().enumerate() {
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT id FROM book_author WHERE name = ? COLLATE NOCASE")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await?;
        let author = match existing {
            Some(id) => id,
            None => sqlx::query("INSERT INTO book_author (name, sort_name) VALUES (?, ?)")
                .bind(name)
                .bind(author_sort_name(name))
                .execute(&mut *conn)
                .await?
                .last_insert_rowid(),
        };
        sqlx::query(
            "INSERT OR IGNORE INTO book_book_author (book_id, author_id, position) VALUES (?, ?, ?)",
        )
        .bind(book_id)
        .bind(author)
        .bind(position as i64)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Delete the authors and series no book uses any more (a typo fixed, the
/// last book of an author deleted).
async fn delete_orphans(conn: &mut SqliteConnection) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM book_author WHERE id NOT IN (SELECT author_id FROM book_book_author);
         DELETE FROM book_series WHERE id NOT IN
             (SELECT series_id FROM book WHERE series_id IS NOT NULL);",
    )
    .execute(conn)
    .await?;
    Ok(())
}
