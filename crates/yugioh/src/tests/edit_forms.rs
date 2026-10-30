//! Edit pages of cards, sellers and lots.

use super::*;

impl TestApp {
    /// Wait for the background card data linking of card `id`.
    async fn wait_linked(&self, id: i64) -> Option<i64> {
        let sql = format!("SELECT card_data_id FROM card WHERE id = {id}");
        for _ in 0..50 {
            let linked: Option<i64> = sqlx::query_scalar(&sql)
                .fetch_one(&self.state.pool)
                .await
                .unwrap();
            if linked.is_some() {
                return linked;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        None
    }
}

#[tokio::test]
async fn edit_card() {
    let t = TestApp::new().await;
    t.post("/lots/add", LOT).await;
    let id = t.scalar("SELECT min(id) FROM card").await;
    assert_eq!(t.wait_linked(id).await, Some(1));

    let uri = format!("/cards/{id}/edit");
    let (status, page) = t.get(&uri).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Magicien Sombre") && page.contains("LOB-FR005"));

    // same English name: the card data link is kept
    let (status, location) = t
        .post(
            &uri,
            &[
                ("name", "Magicien Noir"),
                ("en_name", "Dark Magician"),
                ("code", ""),
                ("status", "EX"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, format!("/cards/{id}"));
    assert_eq!(
        t.text(&format!(
            "SELECT name || '/' || last_known_status FROM card WHERE id = {id}"
        ))
        .await
        .as_deref(),
        Some("Magicien Noir/EX")
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT code IS NULL AND is_proxy = 0 AND card_data_id = 1 FROM card WHERE id = {id}"
        ))
        .await,
        1
    );

    // new English name: relinked
    t.post(&uri, &[("name", "Kuriboh"), ("is_proxy", "on")])
        .await;
    assert_eq!(t.wait_linked(id).await, Some(2));
    assert_eq!(
        t.scalar(&format!("SELECT is_proxy FROM card WHERE id = {id}"))
            .await,
        1
    );

    let (status, page) = t.post(&uri, &[("name", ""), ("status", "ZZ")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Name is required.") && page.contains("Invalid status."));
    assert_eq!(t.get("/cards/999/edit").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        t.post("/cards/999/edit", &[("name", "X")]).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_card_takes_a_picture_of_its_printing_from_yugipedia() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;
    let (id, other) = (cards[0], cards[1]);
    let uri = format!("/cards/{id}/edit");
    t.exec(&format!(
        "UPDATE card SET rarity = 'Ultra Rare' WHERE id IN ({id}, {other})"
    ))
    .await;

    // no gallery cached: nothing asked, a link to ask
    let (_, page) = t.get(&uri).await;
    assert!(page.contains("haven&#x27;t been looked up yet"), "{page}");
    assert!(page.contains(&format!("/cards/{id}/edit?pictures=1")));

    t.exec(
        r#"INSERT INTO yugipedia_gallery (card_data_id, files, checked_at) VALUES (1,
             '["DarkMagician-LOB-FR-UR-1E.png", "DarkMagician-LOB-FR-UR-1E-AA.png",
               "DarkMagician-LOB-EN-UR-1E.png"]', date('now'))"#,
    )
    .await;
    let (_, page) = t.get(&uri).await;
    assert!(page.contains("FR · UR · 1st edition · AA"), "{page}");
    assert!(page.contains(&format!(r#"src="/cards/{id}/pictures/1""#)));
    assert!(!page.contains("LOB-EN-UR"), "not its language");
    assert_eq!(
        t.get(&format!("/cards/{id}/pictures/2")).await.0,
        StatusCode::NOT_FOUND
    );

    let form = |picture: &'static str, everywhere: bool| {
        let mut form = vec![
            ("name", "Magicien Sombre"),
            ("en_name", "dark magician"),
            ("code", "LOB-FR005"),
            ("rarity", "Ultra Rare"),
            ("picture", picture),
        ];
        if everywhere {
            form.push(("picture_everywhere", "on"));
        }
        form
    };
    let picture = |card: i64| {
        let sql = format!(
            "SELECT coalesce(picture, '-') || '/' || picture_manual FROM card WHERE id = {card}"
        );
        let t = &t;
        async move { t.text(&sql).await.unwrap() }
    };

    let (_, page) = t.post(&uri, &form("Other.png", false)).await;
    assert!(page.contains("Unknown picture."), "{page}");

    // the alternate art, for both copies
    let (status, _) = t
        .post(&uri, &form("DarkMagician-LOB-FR-UR-1E-AA.png", true))
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(picture(id).await, "DarkMagician-LOB-FR-UR-1E-AA.png/1");
    assert_eq!(picture(other).await, "DarkMagician-LOB-FR-UR-1E-AA.png/1");
    let (_, page) = t.get("/cards").await;
    assert!(
        page.contains(r#"src="/card-picture/DarkMagician-LOB-FR-UR-1E-AA.jpg""#),
        "{page}"
    );
    let (_, page) = t.get(&uri).await;
    assert!(page.contains(r#"value="DarkMagician-LOB-FR-UR-1E-AA.png" checked"#));
    // one address for both copies, served from the folder (already there:
    // nothing asked to Yugipedia); an unknown one is 404
    let file = crate::store::yugipedia::file_path(&t.state, "DarkMagician-LOB-FR-UR-1E-AA.png");
    common::media::save(&file, &common::photos::test_png(59, 86))
        .await
        .unwrap();
    let (status, _) = t
        .get("/card-picture/DarkMagician-LOB-FR-UR-1E-AA.jpg")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        t.get("/card-picture/Other.jpg").await.0,
        StatusCode::NOT_FOUND
    );

    // ygoprodeck's, kept by the automatic pass
    t.post(&uri, &form("ygoprodeck", false)).await;
    assert_eq!(picture(id).await, "-/1");
    let (_, page) = t.get("/cards").await;
    assert!(page.contains(r#"src="/card-image/1""#));

    // automatic again; a new code puts an automatic picture back to ygoprodeck's
    t.post(&uri, &form("", false)).await;
    assert_eq!(picture(id).await, "-/0");
    t.exec(&format!(
        "UPDATE card SET picture = 'DarkMagician-LOB-FR-UR-1E.png' WHERE id = {id}"
    ))
    .await;
    let mut moved = form("", false);
    moved[2] = ("code", "LOB-FR006");
    t.post(&uri, &moved).await;
    assert_eq!(picture(id).await, "-/0");
}

#[tokio::test]
async fn edit_seller() {
    let t = TestApp::new().await;
    t.post("/lots/add", LOT).await;
    sqlx::query("INSERT INTO seller (name) VALUES ('Other')")
        .execute(&t.state.pool)
        .await
        .unwrap();
    let id = t
        .scalar("SELECT id FROM seller WHERE name = 'New Shop'")
        .await;
    let uri = format!("/sellers/{id}/edit");
    assert_eq!(t.get(&uri).await.0, StatusCode::OK);

    let (status, page) = t.post(&uri, &[("name", "Other")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("already exists"), "{page}");

    let (status, _) = t
        .post(
            &uri,
            &[("name", "Renamed"), ("is_person", "on"), ("source", "")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.scalar(&format!("SELECT is_person = 1 AND source_id IS NULL FROM seller WHERE id = {id} AND name = 'Renamed'"))
            .await,
        1
    );
}

#[tokio::test]
async fn edit_lot() {
    let t = TestApp::new().await;
    t.post("/lots/add", LOT).await;
    let id = t.scalar("SELECT id FROM lot").await;
    let card = t.scalar("SELECT min(card_id) FROM unite").await;
    let other = t.scalar("SELECT max(card_id) FROM unite").await;
    let uri = format!("/lots/{id}/edit");
    let (status, page) = t.get(&uri).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("10.50") && page.contains("2.25"), "{page}");

    let price_field = format!("unite-{card}-price");
    let form = [
        ("lot_type", "sale"),
        ("price", "12"),
        ("buy_date", "2025-06-13"),
        ("received_date", "2025-06-20"),
        ("no_card_price", "1,5"),
        ("seller_select", ""),
        ("is_cancelled", "on"),
        (price_field.as_str(), "3,10"),
    ];
    let (status, location) = t.post(&uri, &form).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(
        t.text(&format!(
            "SELECT lot_type || ' ' || price || ' ' || buy_date || ' ' || received_date
                || ' ' || no_card_price || ' ' || is_cancelled || ' ' || coalesce(seller_id, '-')
             FROM lot WHERE id = {id}"
        ))
        .await
        .as_deref(),
        Some("sale 12 2025-06-13 2025-06-20 1.5 1 -")
    );
    assert_eq!(
        t.text(&format!("SELECT price FROM unite WHERE card_id = {card}"))
            .await
            .as_deref(),
        Some("3.10")
    );
    // a unite missing from the form loses its price
    assert_eq!(
        t.text(&format!("SELECT price FROM unite WHERE card_id = {other}"))
            .await,
        None
    );

    let (status, page) = t
        .post(
            &uri,
            &[
                ("lot_type", "sale"),
                ("price", "x"),
                (price_field.as_str(), "y"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("Buy date is required.") && page.contains("Price of Magicien Sombre"),
        "{page}"
    );
    assert_eq!(t.get("/lots/999/edit").await.0, StatusCode::NOT_FOUND);
}
