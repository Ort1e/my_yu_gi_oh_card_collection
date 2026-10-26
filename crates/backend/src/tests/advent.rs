//! The advent calendar page.

use super::*;

/// A 2025 calendar with Dark Magician behind door 3.
async fn calendar(t: &TestApp) {
    sqlx::query(
        "INSERT INTO card (id, name, en_name, code, rarity, card_data_id, is_proxy)
             VALUES (7, 'Magicien Sombre', 'dark magician', 'LOB-FR005', 'Ultra Rare', 1, 0);
         INSERT INTO advent_calendar (id, year) VALUES (1, 2025);
         INSERT INTO advent_calendar_entry (calendar_id, day, card_id, photo)
             VALUES (1, 3, 7, 'advent_calendar_photos/2025/03.jpg');",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_calendar_shows_its_doors_and_cards() {
    let t = TestApp::new().await;
    calendar(&t).await;

    let (status, list) = t.get("/advent-calendar").await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.contains("Advent calendar 2025") && list.contains("1 / 24 days filled"));
    assert!(
        list.contains("/media/advent_calendar_photos/2025/03.jpg"),
        "its first photo"
    );

    let (status, page) = t.get("/advent-calendar/2025").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Magicien Sombre") && page.contains("LOB-FR005"));
    assert!(page.contains("/media/advent_calendar_photos/2025/03.jpg"));
    assert!(page.contains("/cards/7"));
    assert_eq!(page.matches(r#"class="door-number""#).count(), 24);
    assert_eq!(page.matches("door door-empty").count(), 23);

    assert_eq!(
        t.get("/advent-calendar/1999").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn calendars_are_added_from_the_list() {
    let t = TestApp::new().await;
    let (status, list) = t.get("/advent-calendar").await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.contains("No calendar yet.") && list.contains(r#"href="/advent-calendar/add""#));

    let (status, page) = t.get("/advent-calendar/add").await;
    assert_eq!(status, StatusCode::OK);
    let this_year = chrono::Datelike::year(&chrono::Local::now()).to_string();
    assert!(
        page.contains(&format!(r#"value="{this_year}""#)),
        "this year suggested"
    );

    let (status, location) = t.post("/advent-calendar/add", &[("year", "2026")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, "/advent-calendar/2026/edit");
    assert_eq!(t.get("/advent-calendar/2026/edit").await.0, StatusCode::OK);
    // then the next year is suggested
    assert!(t
        .get("/advent-calendar/add")
        .await
        .1
        .contains(r#"value="2027""#));

    let (status, page) = t.post("/advent-calendar/add", &[("year", "2026")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("There is already a calendar for 2026"),
        "{page}"
    );
    let (_, page) = t.post("/advent-calendar/add", &[("year", "soon")]).await;
    assert!(page.contains("The year must be a year like 2026."));
    assert_eq!(t.scalar("SELECT count(*) FROM advent_calendar").await, 1);
}

#[tokio::test]
async fn the_calendar_and_its_photos_need_the_login() {
    let t = TestApp::with_password(Some("secret")).await;
    calendar(&t).await;
    let photos = t.state.media_dir.join("advent_calendar_photos/2025");
    std::fs::create_dir_all(&photos).unwrap();
    std::fs::write(photos.join("03.jpg"), "jpg").unwrap();

    assert_eq!(
        t.get("/advent-calendar/2025").await,
        (
            StatusCode::SEE_OTHER,
            "/login?next=%2Fadvent-calendar%2F2025".into()
        )
    );
    assert_eq!(
        t.get("/media/advent_calendar_photos/2025/03.jpg").await.0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        t.get("/advent-calendar/2025/edit").await.0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        t.post("/advent-calendar/add", &[("year", "2026")]).await.0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(t.scalar("SELECT count(*) FROM advent_calendar").await, 1);
    // the rest of /media stays public
    std::fs::create_dir_all(t.state.media_dir.join("card_images")).unwrap();
    std::fs::write(t.state.media_dir.join("card_images/x.jpg"), "jpg").unwrap();
    assert_eq!(t.get("/media/card_images/x.jpg").await.0, StatusCode::OK);
}

// ---------- filling the days ----------

/// POST a multipart form: text fields, then an optional file `photo`.
pub(super) async fn multipart(
    t: &TestApp,
    uri: &str,
    fields: &[(&str, &str)],
    photo: Option<&[u8]>,
) -> (StatusCode, String) {
    let boundary = "XyZboundary";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    }
    if let Some(photo) = photo {
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"p.png\"\r\n\
                 Content-Type: image/png\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend(photo);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").as_bytes());
    let response = t
        .router()
        .oneshot(
            Request::post(uri)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    read(response).await
}

fn photo_of(t: &TestApp, day: i64) -> impl std::future::Future<Output = Option<String>> + '_ {
    let sql = format!("SELECT photo FROM advent_calendar_entry WHERE day = {day}");
    async move { t.text(&sql).await }
}

#[tokio::test]
async fn a_day_gets_its_card_and_photo() {
    let t = TestApp::new().await;
    calendar(&t).await;
    let png = crate::store::photos::tests::png(400, 300);

    // by code, with a photo
    let (status, location) = multipart(
        &t,
        "/advent-calendar/2025/days/5",
        &[("card", "lob-fr005")],
        Some(&png),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(location, "/advent-calendar/2025/edit#day-5");
    assert_eq!(
        t.scalar("SELECT card_id FROM advent_calendar_entry WHERE day = 5")
            .await,
        7
    );
    let photo = photo_of(&t, 5).await.unwrap();
    assert!(photo.starts_with("advent_calendar_photos/2025/05-") && photo.ends_with(".jpg"));
    let copy = t.state.media_dir.join(&photo);
    assert!(copy.exists());
    let stem = copy.file_stem().unwrap().to_str().unwrap().to_string();
    let original = copy.with_file_name("originals").join(format!("{stem}.png"));
    assert_eq!(
        std::fs::read(&original).unwrap(),
        png,
        "the original is kept as uploaded"
    );

    // a quarter turn: a new copy, the old one deleted, the original renamed with it
    let (status, _) = t
        .post(
            "/advent-calendar/2025/days/5/rotate",
            &[("direction", "right")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let turned = photo_of(&t, 5).await.unwrap();
    assert_ne!(turned, photo);
    assert!(!copy.exists());
    let img = image::open(t.state.media_dir.join(&turned)).unwrap();
    assert_eq!((img.width(), img.height()), (300, 400));
    let new_stem = std::path::Path::new(&turned)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(original.with_file_name(format!("{new_stem}.png")).exists());

    // the card only: the photo stays
    let (status, _) = multipart(&t, "/advent-calendar/2025/days/5", &[("card", "#7")], None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(photo_of(&t, 5).await.as_deref(), Some(turned.as_str()));

    // a new photo replaces the old files
    multipart(&t, "/advent-calendar/2025/days/5", &[], Some(&png)).await;
    assert!(!t.state.media_dir.join(&turned).exists());

    // emptied: the entry and its files go, the card stays
    let last = photo_of(&t, 5).await.unwrap();
    let (status, _) = t.post("/advent-calendar/2025/days/5/delete", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.scalar("SELECT count(*) FROM advent_calendar_entry WHERE day = 5")
            .await,
        0
    );
    assert!(!t.state.media_dir.join(&last).exists());
    assert_eq!(t.scalar("SELECT count(*) FROM card WHERE id = 7").await, 1);
}

#[tokio::test]
async fn wrong_days_are_explained() {
    let t = TestApp::new().await;
    calendar(&t).await;

    let (status, page) = multipart(
        &t,
        "/advent-calendar/2025/days/6",
        &[("card", "XXXX-FR001")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("there is no card XXXX-FR001 in the collection"),
        "{page}"
    );

    let (_, page) = multipart(&t, "/advent-calendar/2025/days/6", &[], None).await;
    assert!(page.contains("Day 6: choose its card"));

    let (_, page) = multipart(
        &t,
        "/advent-calendar/2025/days/6",
        &[("card", "LOB-FR005")],
        Some(b"not an image"),
    )
    .await;
    assert!(page.contains("Day 6: this image format isn"), "{page}");
    assert_eq!(
        t.scalar("SELECT count(*) FROM advent_calendar_entry WHERE day = 6")
            .await,
        0
    );

    assert_eq!(
        multipart(&t, "/advent-calendar/2025/days/30", &[("card", "#7")], None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_calendar_gets_its_own_lot() {
    let t = TestApp::new().await;
    calendar(&t).await;

    let (status, location) = t.post("/advent-calendar/2025/lot", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let lot = t
        .scalar("SELECT lot_id FROM advent_calendar WHERE year = 2025")
        .await;
    assert_eq!(location, format!("/lots/{lot}"));
    assert_eq!(
        t.text(&format!(
            "SELECT price || ' ' || buy_date || ' ' || seller.name FROM lot
             JOIN seller ON seller.id = lot.seller_id WHERE lot.id = {lot}"
        ))
        .await
        .as_deref(),
        Some("0.00 2025-12-01 Advent calendar")
    );
    // a second click opens the same lot
    assert_eq!(t.post("/advent-calendar/2025/lot", &[]).await.1, location);
    assert_eq!(t.scalar("SELECT count(*) FROM lot").await, 1);

    // linked both ways
    let (_, lot_page) = t.get(&location).await;
    assert!(lot_page.contains(r#"href="/advent-calendar/2025""#));
    assert!(t
        .get("/advent-calendar/2025")
        .await
        .1
        .contains(&format!(r#"href="/lots/{lot}""#)));

    // its cards are suggested for the days, until they are behind a door
    sqlx::query(&format!(
        "INSERT INTO card (id, name, en_name, code, is_proxy) VALUES (8, 'Kuriboh', 'kuriboh', 'MRD-FR071', 0);
         INSERT INTO unite (price, lot_id, card_id) VALUES ('0.00', {lot}, 8);"
    ))
    .execute(&t.state.pool)
    .await
    .unwrap();
    assert!(t
        .get("/advent-calendar/2025/edit")
        .await
        .1
        .contains(r##"value="#8""##));
    sqlx::query("INSERT INTO advent_calendar_entry (calendar_id, day, card_id) VALUES (1, 4, 8)")
        .execute(&t.state.pool)
        .await
        .unwrap();
    assert!(!t
        .get("/advent-calendar/2025/edit")
        .await
        .1
        .contains(r##"value="#8""##));
}
