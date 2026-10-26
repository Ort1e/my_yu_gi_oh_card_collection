//! The books' page of the public export (`common::export`): title,
//! authors, series, format. Not the ISBNs, notes or advent calendars.

use common::db::Pool;
use common::export::{Content, Section};

use crate::routes::series_text;

#[derive(sqlx::FromRow)]
struct PublicBook {
    title: String,
    authors: Option<String>,
    series: Option<String>,
    volume: Option<i64>,
    format: Option<String>,
}

pub async fn section(pool: &Pool) -> sqlx::Result<Section> {
    let books: Vec<PublicBook> = sqlx::query_as(
        "SELECT book.title,
                    (SELECT group_concat(name, ', ') FROM
                        (SELECT a.name FROM book_book_author ba
                         JOIN book_author a ON a.id = ba.author_id
                         WHERE ba.book_id = book.id ORDER BY ba.position)) AS authors,
                    book_series.name AS series, book.volume, book.format
             FROM book LEFT JOIN book_series ON book_series.id = book.series_id
             ORDER BY (SELECT min(a.sort_name) FROM book_book_author ba
                       JOIN book_author a ON a.id = ba.author_id
                       WHERE ba.book_id = book.id AND ba.position = 0) COLLATE NOCASE,
                      book_series.name COLLATE NOCASE, book.volume,
                      book.title COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;

    let count = books.len();
    let rows = books
        .into_iter()
        .map(|book| {
            vec![
                book.title,
                book.authors.unwrap_or_default(),
                series_text(book.series.as_deref(), book.volume),
                book.format.unwrap_or_default(),
            ]
        })
        .collect();

    Ok(Section {
        slug: "books",
        title: "Books",
        body_class: "books",
        summary: format!("{count} books"),
        content: Content::Table {
            headings: vec!["Title", "Authors", "Series", "Format"],
            rows,
        },
    })
}
