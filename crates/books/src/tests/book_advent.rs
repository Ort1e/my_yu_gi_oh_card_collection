//! The book advent calendars (the twin of `advent.rs`).

use super::books::cache_lookup;
use super::*;

/// POST a multipart form: text fields, then an optional file `photo`.
async fn multipart(
    t: &TestApp,
    uri: &str,
    fields: &[(&str, &str)],
    photo: Option<&[u8]>,
) -> (StatusCode, String) {
    t.multipart(uri, fields, photo).await
}

/// A 2025 book calendar with Circe (book 1) behind door 3.
async fn calendar(t: &TestApp) {
    sqlx::query(
        "INSERT INTO book (id, isbn, title, added_on) VALUES (1, '9780316556347', 'Circe', '2025-12-01');
         INSERT INTO book_author (id, name, sort_name) VALUES (1, 'Madeline Miller', 'miller madeline');
         INSERT INTO book_book_author (book_id, author_id, position) VALUES (1, 1, 0);
         INSERT INTO book_advent_calendar (id, year) VALUES (1, 2025);
         INSERT INTO book_advent_calendar_entry (calendar_id, day, book_id, photo)
             VALUES (1, 3, 1, 'book_advent_calendar_photos/2025/03.jpg');",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_calendar_shows_its_doors_and_books() {
    let t = TestApp::new().await;
    calendar(&t).await;

    let (status, list) = t.get("/books/advent-calendar").await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.contains("Book advent calendar 2025") && list.contains("1 / 24 days filled"));

    let (status, page) = t.get("/books/advent-calendar/2025").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Circe") && page.contains("Madeline Miller"));
    assert!(page.contains("/media/book_advent_calendar_photos/2025/03.jpg"));
    assert!(page.contains(r#"href="/books/1""#));
    assert!(page.contains(r#"data-storage-key="book-advent-calendar-2025-opened""#));
    assert_eq!(page.matches("door door-empty").count(), 23);

    // the book page links back to its day
    assert!(t.get("/books/1").await.1.contains("Day 3 of the"));
    assert_eq!(
        t.get("/books/advent-calendar/1999").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn calendars_are_added_from_the_list() {
    let t = TestApp::new().await;
    let this_year = chrono::Datelike::year(&chrono::Local::now()).to_string();
    let (_, page) = t.get("/books/advent-calendar/add").await;
    assert!(page.contains(&format!(r#"value="{this_year}""#)));

    let (status, location) = t
        .post("/books/advent-calendar/add", &[("year", "2026")])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, "/books/advent-calendar/2026/edit");
    assert_eq!(t.get(&location).await.0, StatusCode::OK);

    let (_, page) = t
        .post("/books/advent-calendar/add", &[("year", "2026")])
        .await;
    assert!(page.contains("There is already a calendar for 2026"));
    // independent of the card calendars
    assert_eq!(t.scalar("SELECT count(*) FROM advent_calendar").await, 0);
}

#[tokio::test]
async fn a_day_gets_its_book_and_photo() {
    let t = TestApp::new().await;
    calendar(&t).await;
    cache_lookup(
        &t,
        "9780553573398",
        serde_json::json!({"title": "Assassin's Apprentice", "authors": ["Robin Hobb"],
                           "series": "The Farseer", "volume": 1}),
    )
    .await;
    let png = common::photos::test_png(400, 300);

    // an ISBN not in the list: looked up and added
    let (status, location) = multipart(
        &t,
        "/books/advent-calendar/2025/days/5",
        &[("book", "978-0-553-57339-8")],
        Some(&png),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(location, "/books/advent-calendar/2025/edit#day-5");
    let added = t
        .scalar("SELECT id FROM book WHERE isbn = '9780553573398'")
        .await;
    assert_eq!(
        t.scalar("SELECT book_id FROM book_advent_calendar_entry WHERE day = 5")
            .await,
        added
    );
    assert_eq!(
        t.text(&format!(
            "SELECT book_series.name || ' #' || volume FROM book
             JOIN book_series ON book_series.id = series_id WHERE book.id = {added}"
        ))
        .await
        .as_deref(),
        Some("The Farseer #1")
    );
    let photo = t
        .text("SELECT photo FROM book_advent_calendar_entry WHERE day = 5")
        .await
        .unwrap();
    assert!(photo.starts_with("book_advent_calendar_photos/2025/05-"));
    assert!(t.state.media_dir.join(&photo).exists());

    // turned
    let (status, _) = t
        .post(
            "/books/advent-calendar/2025/days/5/rotate",
            &[("direction", "left")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let turned = t
        .text("SELECT photo FROM book_advent_calendar_entry WHERE day = 5")
        .await
        .unwrap();
    assert!(!t.state.media_dir.join(&photo).exists());
    let img = image::open(t.state.media_dir.join(&turned)).unwrap();
    assert_eq!((img.width(), img.height()), (300, 400));

    // a book already in the list, by ISBN or #id: no new book
    multipart(
        &t,
        "/books/advent-calendar/2025/days/6",
        &[("book", "9780316556347")],
        None,
    )
    .await;
    multipart(
        &t,
        "/books/advent-calendar/2025/days/7",
        &[("book", "#1")],
        None,
    )
    .await;
    assert_eq!(
        t.scalar("SELECT count(*) FROM book_advent_calendar_entry WHERE book_id = 1")
            .await,
        3
    );
    assert_eq!(t.scalar("SELECT count(*) FROM book").await, 2);

    // emptied: the entry and its files go, the book stays
    let (status, _) = t
        .post("/books/advent-calendar/2025/days/5/delete", &[])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(!t.state.media_dir.join(&turned).exists());
    assert_eq!(
        t.scalar(&format!("SELECT count(*) FROM book WHERE id = {added}"))
            .await,
        1
    );
}

#[tokio::test]
async fn wrong_days_are_explained() {
    let t = TestApp::new().await;
    calendar(&t).await;
    // known to be unknown (checked today): no network call
    sqlx::query("INSERT INTO book_data (isbn, json_data, checked_at) VALUES ('9780007448036', NULL, date('now'))")
        .execute(&t.state.pool)
        .await
        .unwrap();

    let cases = [
        (
            "9780007448036",
            "Day 6: no catalogue knows the ISBN 9780007448036",
        ),
        (
            "9780007448037",
            "Day 6: &quot;9780007448037&quot;: its last digit",
        ),
        ("#42", "Day 6: there is no book #42 in the list."),
        ("", "Day 6: choose its book"),
    ];
    for (book, error) in cases {
        let (status, page) = multipart(
            &t,
            "/books/advent-calendar/2025/days/6",
            &[("book", book)],
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains(error), "{book}: {page}");
    }
    assert_eq!(
        t.scalar("SELECT count(*) FROM book_advent_calendar_entry")
            .await,
        1
    );
    assert_eq!(
        multipart(
            &t,
            "/books/advent-calendar/2025/days/26",
            &[("book", "#1")],
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_calendar_and_its_photos_need_the_login() {
    let t = TestApp::with_password(Some("secret")).await;
    calendar(&t).await;
    let photos = t.state.media_dir.join("book_advent_calendar_photos/2025");
    std::fs::create_dir_all(&photos).unwrap();
    std::fs::write(photos.join("03.jpg"), "jpg").unwrap();

    for page in [
        "/books/advent-calendar",
        "/books/advent-calendar/2025",
        "/books/advent-calendar/2025/edit",
        "/books/advent-calendar/add",
        "/media/book_advent_calendar_photos/2025/03.jpg",
    ] {
        assert_eq!(t.get(page).await.0, StatusCode::SEE_OTHER, "{page}");
    }
    assert_eq!(
        t.post("/books/advent-calendar/2025/days/3/delete", &[])
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM book_advent_calendar_entry")
            .await,
        1
    );
}
