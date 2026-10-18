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

    assert_eq!(
        t.get("/advent-calendar").await,
        (StatusCode::SEE_OTHER, "/advent-calendar/2025".into()),
        "the latest calendar"
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
async fn no_calendar_yet() {
    let t = TestApp::new().await;
    assert_eq!(t.get("/advent-calendar").await.0, StatusCode::NOT_FOUND);
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
    // the rest of /media stays public
    std::fs::create_dir_all(t.state.media_dir.join("card_images")).unwrap();
    std::fs::write(t.state.media_dir.join("card_images/x.jpg"), "jpg").unwrap();
    assert_eq!(t.get("/media/card_images/x.jpg").await.0, StatusCode::OK);
}
