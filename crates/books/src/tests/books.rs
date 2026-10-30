//! The book pages: list, detail, add/edit/delete, authors, series, and the
//! ISBN lookup (from the `book_data` cache: no network in tests).

use super::*;

const GAME_OF_THRONES: &[(&str, &str)] = &[
    ("isbn", "978-0-00-744803-6"),
    ("title", "A Game of  Thrones"),
    ("authors", "George R. R. Martin"),
    ("series", "A Song of Ice and Fire"),
    ("volume", "1"),
    ("publisher", "Harper Voyager"),
    ("year", "2012"),
    ("language", "EN"),
    ("pages", "801"),
    ("format", "Paperback"),
    ("notes", "Signed!"),
];

async fn add(t: &TestApp, form: &[(&str, &str)]) -> i64 {
    let (status, location) = t.post("/books/add", form).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    location
        .strip_prefix("/books/add?added=")
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn a_book_is_added_and_shown() {
    let t = TestApp::new().await;
    let (status, list) = t.get("/books").await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.contains("No book yet"));

    let id = add(&t, GAME_OF_THRONES).await;
    let (status, page) = t.get(&format!("/books/add?added={id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Added: <a href=\"/books/1\">A Game of Thrones</a>"));
    assert_eq!(
        t.text("SELECT isbn || ' ' || language || ' ' || format FROM book")
            .await
            .as_deref(),
        Some("9780007448036 en Paperback"),
        "ISBN without hyphens, cleaned title, language lower case"
    );

    let (_, page) = t.get(&format!("/books/{id}")).await;
    assert!(page.contains("A Game of Thrones") && page.contains("George R. R. Martin"));
    assert!(page.contains("Volume 1 of") && page.contains("A Song of Ice and Fire"));
    assert!(
        page.contains("Signed!") && page.contains("<dd>English</dd>"),
        "{page}"
    );
    // its cover from Open Library, by ISBN (not stored)
    assert!(page.contains("https://covers.openlibrary.org/b/isbn/9780007448036-M.jpg"));
    assert!(page.contains("/books/authors/1") && page.contains("/books/series/1"));

    let (_, list) = t.get("/books").await;
    assert!(list.contains("A Game of Thrones") && list.contains("George R. R. Martin"));
    assert_eq!(t.get("/books/99").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_fields_are_explained() {
    let t = TestApp::new().await;
    add(&t, GAME_OF_THRONES).await;

    let (status, page) = t
        .post(
            "/books/add",
            &[
                ("isbn", "9780007448037"),
                ("title", " "),
                ("year", "soon"),
                ("volume", "2"),
                ("format", "Scroll"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    for error in [
        "ISBN: its last digit doesn",
        "Title is required.",
        "Year: &quot;soon&quot; should be a number",
        "Volume: give the series",
        "Format: invalid choice.",
    ] {
        assert!(page.contains(error), "{error}: {page}");
    }
    assert!(
        page.contains(r#"value="9780007448037""#),
        "typed values kept"
    );

    // the same edition twice
    let (_, page) = t
        .post("/books/add", &[("isbn", "0007448031"), ("title", "AGOT")])
        .await;
    assert!(
        page.contains("This ISBN is already in the list: &quot;A Game of Thrones&quot; (book #1)"),
        "{page}"
    );
    assert_eq!(t.scalar("SELECT count(*) FROM book").await, 1);
}

#[tokio::test]
async fn editing_moves_authors_and_series() {
    let t = TestApp::new().await;
    let id = add(&t, GAME_OF_THRONES).await;
    let (status, page) = t.get(&format!("/books/{id}/edit")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(r#"value="George R. R. Martin""#));
    assert!(page.contains(r#"<option value="Paperback" selected>"#));

    let (status, location) = t
        .post(
            &format!("/books/{id}/edit"),
            &[
                ("isbn", "9780007448036"),
                ("title", "A Game of Thrones"),
                ("authors", "G. R. R. Martin, Someone Else"),
                ("series", "a song of ice and fire"),
                ("volume", "1"),
            ],
        )
        .await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/books/1")
    );
    // the old author is gone with its last book; the series matched case-insensitively
    assert_eq!(
        t.text("SELECT group_concat(name, '|') FROM (SELECT name FROM book_author ORDER BY id)")
            .await
            .as_deref(),
        Some("G. R. R. Martin|Someone Else")
    );
    assert_eq!(t.scalar("SELECT count(*) FROM book_series").await, 1);
    assert_eq!(
        t.text("SELECT sort_name FROM book_author WHERE name = 'Someone Else'")
            .await
            .as_deref(),
        Some("else someone")
    );
    assert_eq!(
        t.text("SELECT format FROM book").await,
        None,
        "emptied fields are cleared"
    );

    assert_eq!(
        t.post("/books/42/edit", &[("title", "X")]).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn deleting_a_book() {
    let t = TestApp::new().await;
    let id = add(&t, GAME_OF_THRONES).await;
    sqlx::query(
        "INSERT INTO book_advent_calendar (id, year) VALUES (1, 2025);
         INSERT INTO book_advent_calendar_entry (calendar_id, day, book_id) VALUES (1, 3, 1);",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();

    // behind a door: refused, with the day
    let (status, page) = t.post(&format!("/books/{id}/delete"), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("behind door 3 of the 2025 advent calendar"),
        "{page}"
    );
    assert_eq!(t.scalar("SELECT count(*) FROM book").await, 1);

    sqlx::query("DELETE FROM book_advent_calendar_entry")
        .execute(&t.state.pool)
        .await
        .unwrap();
    let (status, location) = t.post(&format!("/books/{id}/delete"), &[]).await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/books")
    );
    assert_eq!(t.scalar("SELECT count(*) FROM book").await, 0);
    assert_eq!(
        t.scalar("SELECT (SELECT count(*) FROM book_author) + (SELECT count(*) FROM book_series)")
            .await,
        0,
        "its author and series went with it"
    );
    assert_eq!(
        t.post("/books/1/delete", &[]).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_list_filters_and_sorts() {
    let t = TestApp::new().await;
    add(&t, &[("title", "Circe"), ("authors", "Madeline Miller")]).await;
    add(
        &t,
        &[
            ("title", "Assassin's Apprentice"),
            ("authors", "Robin Hobb"),
            ("series", "Farseer"),
            ("volume", "1"),
        ],
    )
    .await;
    add(
        &t,
        &[
            ("title", "Royal Assassin"),
            ("authors", "Robin Hobb"),
            ("series", "Farseer"),
            ("volume", "2"),
        ],
    )
    .await;
    add(&t, &[("title", "Anonymous Tales")]).await;

    // the titles of the table's rows, in order
    let titles = |page: &str| -> Vec<String> {
        page.split(r#"<td><a href="/books/"#)
            .skip(1)
            .map(|s| {
                s.split('>')
                    .nth(1)
                    .unwrap()
                    .split('<')
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect()
    };

    let (_, page) = t.get("/books").await;
    assert_eq!(
        titles(&page),
        [
            "Anonymous Tales",
            "Assassin&#x27;s Apprentice",
            "Circe",
            "Royal Assassin"
        ]
    );
    let (_, page) = t.get("/books?sort=author").await;
    assert_eq!(
        titles(&page),
        [
            "Assassin&#x27;s Apprentice",
            "Royal Assassin",
            "Circe",
            "Anonymous Tales"
        ],
        "by surname (Hobb, Miller), the books without author last"
    );
    let (_, page) = t.get("/books?sort=added").await;
    assert_eq!(titles(&page)[0], "Anonymous Tales");

    let (_, page) = t.get("/books?author=hobb&title=royal").await;
    assert_eq!(titles(&page), ["Royal Assassin"]);
    assert!(page.contains("1 of 4 books, by the filters of the address"));
    let (_, page) = t.get("/books?series=far").await;
    assert_eq!(titles(&page).len(), 2);
}

#[tokio::test]
async fn authors_and_series_pages() {
    let t = TestApp::new().await;
    add(
        &t,
        &[
            ("title", "Assassin's Apprentice"),
            ("authors", "Robin Hobb"),
            ("series", "Farseer"),
            ("volume", "1"),
        ],
    )
    .await;
    add(
        &t,
        &[
            ("title", "Assassin's Quest"),
            ("authors", "robin hobb"),
            ("series", "Farseer"),
            ("volume", "4"),
        ],
    )
    .await;
    assert_eq!(
        t.scalar("SELECT count(*) FROM book_author").await,
        1,
        "matched case-insensitively"
    );

    let (status, page) = t.get("/books/authors").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("Robin Hobb") && page.contains("<td>2</td>"),
        "{page}"
    );
    assert!(
        page.contains(r#"data-sort-value="hobb robin""#),
        "by surname"
    );
    let (_, page) = t.get("/books/authors/1").await;
    assert!(page.contains("Assassin&#x27;s Apprentice") && page.contains("Farseer #4"));
    // a book's page: the other volumes of its series, and the gaps
    let (_, page) = t.get("/books/1").await;
    assert!(
        page.contains(r#"<a href="/books/2">Assassin&#x27;s Quest</a>"#),
        "{page}"
    );
    assert!(page.contains("Missing: #2, #3"));

    let (_, page) = t.get("/books/series").await;
    assert!(
        page.contains("Farseer") && page.contains(r#"<td class="text missing">2, 3</td>"#),
        "{page}"
    );
    let (_, page) = t.get("/books/series/1").await;
    assert!(page.contains("Missing volumes: 2, 3."));
    assert!(
        page.contains(r#"href="/books/add?series=Farseer&amp;volume=2""#),
        "{page}"
    );
    // the add page, ready for the missing volume
    let (_, page) = t.get("/books/add?series=Farseer&volume=2").await;
    assert!(page.contains(r#"value="Farseer""#) && page.contains(r#"value="2""#));

    // the books in no series: none here, then one
    let (status, page) = t.get("/books/standalone").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Every book is in a series."));
    add(&t, &[("title", "Circe"), ("authors", "Madeline Miller")]).await;
    let (_, page) = t.get("/books/standalone").await;
    assert!(
        page.contains("Circe") && !page.contains("Assassin&#x27;s Quest"),
        "{page}"
    );

    assert_eq!(t.get("/books/authors/9").await.0, StatusCode::NOT_FOUND);
    assert_eq!(t.get("/books/series/9").await.0, StatusCode::NOT_FOUND);
}

/// A lookup answer, cached as `store::book_data` stores it.
pub(super) async fn cache_lookup(t: &TestApp, isbn: &str, info: serde_json::Value) {
    sqlx::query("INSERT INTO book_data (isbn, source, json_data, checked_at) VALUES (?, 'test', ?, date('now'))")
        .bind(isbn)
        .bind(info.to_string())
        .execute(&t.state.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_isbn_lookup() {
    let t = TestApp::new().await;
    cache_lookup(
        &t,
        "9780553573398",
        serde_json::json!({
            "title": "Assassin's Apprentice", "authors": ["Robin Hobb"], "publisher": "Bantam",
            "year": 1995, "language": "en", "pages": 435, "series": "The Farseer", "volume": 1,
            "format": "Paperback"
        }),
    )
    .await;
    // nothing found, checked today: not asked again
    sqlx::query("INSERT INTO book_data (isbn, json_data, checked_at) VALUES ('9780316556347', NULL, date('now'))")
        .execute(&t.state.pool)
        .await
        .unwrap();

    let (status, answer) = t
        .json(
            "GET",
            "/books/api/isbn/0-553-57339-X",
            serde_json::Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["isbn"], "9780553573398");
    assert_eq!(answer["title"], "Assassin's Apprentice");
    assert_eq!(answer["authors"][0], "Robin Hobb");
    assert_eq!(answer["format"], "Paperback");
    assert_eq!(answer["owned_book_id"], serde_json::Value::Null);

    add(
        &t,
        &[
            ("isbn", "9780553573398"),
            ("title", "Assassin's Apprentice"),
        ],
    )
    .await;
    let (_, answer) = t
        .json(
            "GET",
            "/books/api/isbn/9780553573398",
            serde_json::Value::Null,
        )
        .await;
    assert_eq!(answer["owned_book_id"], 1);

    let (status, answer) = t
        .json(
            "GET",
            "/books/api/isbn/9780316556347",
            serde_json::Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(answer["error"]
        .as_str()
        .unwrap()
        .contains("No catalogue knows"));

    let (status, answer) = t
        .json("GET", "/books/api/isbn/12345", serde_json::Value::Null)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(answer["error"].as_str().unwrap().contains("13 digits"));
}

#[tokio::test]
async fn reading_is_public_writing_needs_the_login() {
    let t = TestApp::with_password(Some("secret")).await;
    sqlx::query("INSERT INTO book (id, title, added_on) VALUES (1, 'Circe', '2026-10-18')")
        .execute(&t.state.pool)
        .await
        .unwrap();

    for page in ["/books", "/books/1", "/books/authors", "/books/series"] {
        assert_eq!(t.get(page).await.0, StatusCode::OK, "{page}");
    }
    for page in ["/books/add", "/books/1/edit", "/books/advent-calendar"] {
        assert_eq!(t.get(page).await.0, StatusCode::SEE_OTHER, "{page}");
    }
    assert_eq!(
        t.post("/books/add", &[("title", "X")]).await.0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        t.post("/books/1/delete", &[]).await.0,
        StatusCode::SEE_OTHER
    );
    let (status, answer) = t
        .json(
            "GET",
            "/books/api/isbn/9780553573398",
            serde_json::Value::Null,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the API answers 401, not a redirect"
    );
    assert!(answer["error"].is_string());
    assert_eq!(
        t.multipart("/books/api/isbn-scan", &[], Some(b"x")).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(t.scalar("SELECT count(*) FROM book").await, 1);
}

#[tokio::test]
async fn the_scan_reads_the_isbn_on_the_nas() {
    use crate::barcode::tests::barcode_photo;
    use image::ImageFormat;

    let t = TestApp::new().await;
    let scan = |photo: Option<Vec<u8>>| {
        let t = &t;
        async move {
            let (status, body) = t
                .multipart("/books/api/isbn-scan", &[], photo.as_deref())
                .await;
            (
                status,
                serde_json::from_str::<serde_json::Value>(&body).unwrap(),
            )
        }
    };

    let photo = barcode_photo("9780553573398", 2000, 1500, ImageFormat::Jpeg);
    let (status, answer) = scan(Some(photo)).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["isbn"], "9780553573398");

    // a barcode, but not a book's
    let photo = barcode_photo("3017620422003", 1200, 900, ImageFormat::Png);
    let (status, answer) = scan(Some(photo)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(answer["error"]
        .as_str()
        .unwrap()
        .contains("No ISBN barcode"));

    let (status, answer) = scan(Some(b"not a photo".to_vec())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        answer["error"]
            .as_str()
            .unwrap()
            .contains("isn't supported"),
        "{answer}"
    );
    assert_eq!(scan(None).await.0, StatusCode::BAD_REQUEST);
}
