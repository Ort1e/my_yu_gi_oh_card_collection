//! Lot creation, shipment upload, cards added to a lot.

use super::*;

#[tokio::test]
async fn create_lot_with_new_seller_and_cards() {
    let t = TestApp::new().await;
    let (status, location) = t.post("/lots/add", LOT).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    let lot_id = t.scalar("SELECT max(id) FROM lot").await;
    assert_eq!(location, format!("/lots/{lot_id}"));

    assert_eq!(
        t.text("SELECT price FROM lot").await.as_deref(),
        Some("10.50")
    );
    assert_eq!(
        t.scalar("SELECT is_person FROM seller WHERE name = 'New Shop'")
            .await,
        0
    );
    assert_eq!(
        t.scalar("SELECT source_id FROM seller WHERE name = 'New Shop'")
            .await,
        1
    );
    assert_eq!(t.scalar("SELECT count(*) FROM card").await, 3);
    assert_eq!(
        t.scalar("SELECT count(*) FROM unite WHERE price = '2.25'")
            .await,
        2
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM card WHERE en_name = 'dark magician'")
            .await,
        2
    );

    // card data is linked in the background
    for _ in 0..50 {
        if t.scalar("SELECT count(*) FROM card WHERE card_data_id IS NOT NULL")
            .await
            == 3
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        t.scalar("SELECT count(*) FROM card WHERE card_data_id IS NOT NULL")
            .await,
        3
    );

    let (status, page) = t.get(&location).await;
    assert_eq!(status, StatusCode::OK);
    // the two Magicien Sombre copies on one row, the total counting both
    assert!(page.contains("Magicien Sombre"), "{page}");
    assert!(
        page.contains("<td>2</td>\n      <td>2.25</td>"),
        "same copies grouped: {page}"
    );
    assert!(page.contains("<td>4.50</td>"), "{page}");
}

#[tokio::test]
async fn invalid_lot_is_rerendered_with_errors() {
    let t = TestApp::new().await;
    let (status, page) = t
        .post(
            "/lots/add",
            &[
                ("lot_type", "purchase"),
                ("price", "abc"),
                ("buy_date", ""),
                ("cards-0-name", "Kuriboh"),
                ("cards-0-price", "1.00"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("is not a valid amount"), "{page}");
    assert!(page.contains("Buy date is required."));
    assert!(page.contains("Select a seller or enter a new seller name."));
    assert!(page.contains(r#"value="Kuriboh""#), "typed values are kept");
    assert_eq!(t.scalar("SELECT count(*) FROM lot").await, 0);
}

#[tokio::test]
async fn pending_shipment_file_is_attached() {
    let t = TestApp::new().await;
    std::fs::create_dir_all(&t.state.uploads_dir).unwrap();
    let pending = t.state.uploads_dir.join("pending-0123456789abcdef.pdf");
    std::fs::write(&pending, b"%PDF-fake").unwrap();

    let mut form = LOT.to_vec();
    form.push(("pending", "0123456789abcdef"));
    let (status, _) = t.post("/lots/add", &form).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let lot_id = t.scalar("SELECT max(id) FROM lot").await;
    let file = t.text("SELECT shipment_file FROM lot").await.unwrap();
    assert_eq!(file, format!("shipment_files/shipment_{lot_id}.pdf"));
    assert!(t.state.media_dir.join(&file).exists());
    assert!(!pending.exists());

    // an unknown token is rejected, nothing is created
    let mut form = LOT.to_vec();
    form.push(("pending", "ffffffffffffffff"));
    let (status, page) = t.post("/lots/add", &form).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("expired"));
    assert_eq!(t.scalar("SELECT count(*) FROM lot").await, 1);
}

#[tokio::test]
async fn upload_rejects_a_non_pdf() {
    let t = TestApp::new().await;
    let boundary = "XBOUNDARYX";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"shipment_file\"; filename=\"a.pdf\"\r\n\
         Content-Type: application/pdf\r\n\r\nnot a pdf\r\n--{boundary}--\r\n"
    );
    let response = t
        .router()
        .oneshot(
            Request::post("/lots/add/shipment")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page) = read(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("Failed to process the shipment PDF"),
        "{page}"
    );
    assert!(!t.state.uploads_dir.exists(), "nothing is saved");
}

#[tokio::test]
async fn add_cards_to_existing_lot() {
    let t = TestApp::new().await;
    t.post("/lots/add", LOT).await;
    let lot_id = t.scalar("SELECT max(id) FROM lot").await;

    let uri = format!("/lots/{lot_id}/add-cards");
    // the lot page only links to the form, on its own page
    let (_, page) = t.get(&format!("/lots/{lot_id}")).await;
    assert!(page.contains(&format!("href=\"/lots/{lot_id}/add-cards\"")));
    assert!(!page.contains("card-rows"), "no form on the lot page");
    let (status, page) = t.get(&uri).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("card-rows") && page.contains("quick-add"),
        "{page}"
    );
    assert_eq!(t.get("/lots/999/add-cards").await.0, StatusCode::NOT_FOUND);

    let (status, _) = t
        .post(
            &uri,
            &[("cards-0-name", "Kuriboh"), ("cards-0-quantity", "2")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM unite").await, 5);

    let (status, page) = t.post(&uri, &[("cards-0-name", "")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("name is required"));

    // several rows (quick add): one invalid row keeps all of them on the page
    let (status, page) = t
        .post(
            &uri,
            &[
                ("cards-0-name", "Dark Magician"),
                ("cards-3-name", "Kuriboh"),
                ("cards-3-price", "abc"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("is not a valid price"), "{page}");
    assert!(
        page.contains("Dark Magician") && page.contains("abc"),
        "{page}"
    );
    assert_eq!(t.scalar("SELECT count(*) FROM unite").await, 5);

    assert_eq!(
        t.post("/lots/999/add-cards", &[("cards-0-name", "X")])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

// ---------- the deck received in a lot ----------

/// A lot, and an Xyz (card data 3) next to the fixtures' two cards.
async fn lot_for_deck(t: &TestApp) -> i64 {
    t.exec(
        "INSERT INTO card_data (id, en_name, ygopro_id, card_type, json_data)
         VALUES (3, 'number 39: utopia', 84013237, 'XYZ Monster', '{}');
         INSERT INTO seller (id, name, is_person, source_id) VALUES (1, 'Jeanne', 1, 1);
         INSERT INTO lot (id, lot_type, price, buy_date, seller_id, no_card_price)
             VALUES (1, 'purchase', '10.00', '2025-06-12', 1, '0.00');",
    )
    .await;
    1
}

const DECK_ROWS: &[(&str, &str)] = &[
    ("deck_name", "Received deck"),
    ("version_name", "From lot 1"),
    ("cards-0-name", "Dark Magician"),
    ("cards-0-quantity", "2"),
    ("cards-0-status", "P"),
    ("cards-1-name", "Number 39: Utopia"),
    ("cards-2-name", "Kuriboh"),
    ("cards-2-zone", "side"),
];

#[tokio::test]
async fn the_lot_deck_first_time() {
    let t = TestApp::new().await;
    let lot = lot_for_deck(&t).await;
    // better copies owned already: they must not take the lot's places
    t.exec(
        "INSERT INTO card (id, name, en_name, last_known_status, card_data_id)
             VALUES (50, 'Dark Magician', 'dark magician', 'MT', 1);",
    )
    .await;

    let (_, page) = t.get(&format!("/lots/{lot}")).await;
    assert!(page.contains(&format!("/lots/{lot}/deck")), "{page}");
    let (status, page) = t.get(&format!("/lots/{lot}/deck")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("name=\"deck_name\"") && page.contains("quick-add"));

    let (status, location) = t.post(&format!("/lots/{lot}/deck"), DECK_ROWS).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(location, format!("/lots/{lot}"));

    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 1);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version WHERE is_built = 1 AND is_public = 1")
            .await,
        1
    );
    let snapshot = t
        .scalar("SELECT id FROM deck_snapshot WHERE lot_id = 1")
        .await;
    assert_eq!(
        t.scalar(&format!(
            "SELECT sum(count) FROM deck_snapshot_card WHERE snapshot_id = {snapshot}"
        ))
        .await,
        4
    );
    // the copies are in the lot, linked at once
    assert_eq!(
        t.scalar("SELECT count(*) FROM unite WHERE lot_id = 1")
            .await,
        4
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM card WHERE id != 50 AND card_data_id IS NOT NULL")
            .await,
        4
    );
    assert_eq!(
        t.text("SELECT last_known_status FROM card WHERE en_name = 'dark magician' AND id != 50 LIMIT 1")
            .await
            .as_deref(),
        Some("P")
    );
    // zones: the Xyz in the extra deck, the Kuriboh asked for the side
    let zone = |cd: i64| format!("SELECT zone FROM deck_version_card WHERE card_data_id = {cd}");
    assert_eq!(t.text(&zone(1)).await.as_deref(), Some("main"));
    assert_eq!(t.text(&zone(3)).await.as_deref(), Some("extra"));
    assert_eq!(t.text(&zone(2)).await.as_deref(), Some("side"));
    // the slots hold exactly the new copies, not the better one owned
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version_copy WHERE card_id = 50")
            .await,
        0
    );
    assert_eq!(
        t.scalar(
            "SELECT count(*) FROM deck_version_copy c JOIN unite u ON u.card_id = c.card_id
             WHERE u.lot_id = 1"
        )
        .await,
        4
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version_copy WHERE card_id IS NULL")
            .await,
        0
    );

    // the lot page and the snapshot page name each other
    let (_, page) = t.get(&format!("/lots/{lot}")).await;
    assert!(page.contains("Decks received:") && page.contains("Received deck"));
    assert!(page.contains(">Add a deck<"), "{page}");
    assert!(
        page.contains(&format!("/lots/{lot}/deck/{snapshot}")),
        "{page}"
    );
    let (_, page) = t.get(&format!("/deck-snapshots/{snapshot}")).await;
    assert!(page.contains("Received in"), "{page}");
}

#[tokio::test]
async fn the_lot_deck_grows_with_more_cards() {
    let t = TestApp::new().await;
    let lot = lot_for_deck(&t).await;
    t.post(&format!("/lots/{lot}/deck"), DECK_ROWS).await;
    let version = t.scalar("SELECT id FROM deck_version").await;
    let snapshot = t.scalar("SELECT id FROM deck_snapshot").await;
    let uri = format!("/lots/{lot}/deck/{snapshot}");

    // a builder save in between doesn't touch the lot's list
    t.set_main(version, &[(Some(1), "Dark Magician", 1)]).await;
    assert_eq!(
        t.scalar(&format!(
            "SELECT sum(count) FROM deck_snapshot_card WHERE snapshot_id = {snapshot}"
        ))
        .await,
        4
    );

    let (status, _) = t
        .post(
            &uri,
            &[("cards-0-name", "Dark Magician"), ("cards-0-quantity", "2")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM deck_version").await, 1);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_snapshot WHERE lot_id = 1")
            .await,
        1
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT count FROM deck_snapshot_card WHERE snapshot_id = {snapshot} AND card_data_id = 1"
        ))
        .await,
        4
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM unite WHERE lot_id = 1")
            .await,
        6
    );
    let (count, slots): (i64, i64) = sqlx::query_as(
        "SELECT l.count, (SELECT count(*) FROM deck_version_copy c
            WHERE c.deck_version_id = l.deck_version_id AND c.zone = l.zone AND c.position = l.position)
         FROM deck_version_card l WHERE l.card_data_id = 1",
    )
    .fetch_one(&t.state.pool)
    .await
    .unwrap();
    assert_eq!(count, slots);
}

#[tokio::test]
async fn the_lot_deck_in_an_existing_deck() {
    let t = TestApp::new().await;
    let lot = lot_for_deck(&t).await;
    t.post("/decks/add", &[("name", "Mine"), ("version_name", "v1")])
        .await;
    let (status, _) = t
        .post(
            &format!("/lots/{lot}/deck"),
            &[
                ("deck_id", "1"),
                ("version_name", "Received"),
                ("cards-0-name", "Kuriboh"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 1);
    assert_eq!(t.scalar("SELECT count(*) FROM deck_version").await, 2);
    assert_eq!(
        t.text("SELECT version_name FROM deck_version WHERE is_built = 1")
            .await
            .as_deref(),
        Some("Received")
    );
}

#[tokio::test]
async fn the_lot_deck_refuses_what_it_cant_keep() {
    let t = TestApp::new().await;
    let lot = lot_for_deck(&t).await;
    let uri = format!("/lots/{lot}/deck");

    // no deck chosen nor named (a name ygoprodeck doesn't know is refused
    // the same way, but asking it is not for a test)
    let (status, page) = t.post(&uri, &[("cards-0-name", "Kuriboh")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("name is required"), "{page}");
    assert_eq!(t.scalar("SELECT count(*) FROM card").await, 0);

    let (status, page) = t
        .post(
            &uri,
            &[
                ("deck_name", "X"),
                ("cards-0-name", "Kuriboh"),
                ("cards-0-zone", "bin"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("unknown zone"), "{page}");
    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 0);

    t.post(&uri, DECK_ROWS).await;
    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 1);
    assert_eq!(t.get("/lots/999/deck").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_lot_holds_several_decks() {
    let t = TestApp::new().await;
    let lot = lot_for_deck(&t).await;
    let uri = format!("/lots/{lot}/deck");
    let second: &[(&str, &str)] = &[
        ("deck_name", "Second deck"),
        ("cards-0-name", "Kuriboh"),
        ("cards-0-quantity", "3"),
    ];
    assert_eq!(t.post(&uri, DECK_ROWS).await.0, StatusCode::SEE_OTHER);

    // the second deck's form proposes the numbered name
    let (_, page) = t.get(&uri).await;
    assert!(page.contains("From lot 1 (2)"), "{page}");
    assert_eq!(t.post(&uri, second).await.0, StatusCode::SEE_OTHER);
    // ... and a form without a version name gets it too
    assert_eq!(
        t.text("SELECT version_name FROM deck_version WHERE deck_id = 2")
            .await
            .as_deref(),
        Some("From lot 1 (2)")
    );

    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 2);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_snapshot WHERE lot_id = 1")
            .await,
        2
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version WHERE is_built = 1")
            .await,
        2
    );
    // each version holds only its own copies
    let held = |deck: i64| {
        format!(
            "SELECT count(*) FROM deck_version_copy c
             JOIN deck_version v ON v.id = c.deck_version_id
             WHERE v.deck_id = {deck} AND c.card_id IS NOT NULL"
        )
    };
    assert_eq!(t.scalar(&held(1)).await, 4);
    assert_eq!(t.scalar(&held(2)).await, 3);

    let (_, page) = t.get(&format!("/lots/{lot}")).await;
    assert!(page.contains("Received deck") && page.contains("Second deck"));
    assert!(page.contains(">Add a deck<"), "{page}");

    // more cards go to the chosen one only
    let snapshot = t
        .scalar("SELECT id FROM deck_snapshot WHERE deck_id = 2")
        .await;
    let (status, _) = t
        .post(
            &format!("/lots/{lot}/deck/{snapshot}"),
            &[("cards-0-name", "Kuriboh")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar(&held(1)).await, 4);
    assert_eq!(t.scalar(&held(2)).await, 4);
}

#[tokio::test]
async fn a_lot_deck_of_another_lot_is_not_found() {
    let t = TestApp::new().await;
    let lot = lot_for_deck(&t).await;
    t.exec(
        "INSERT INTO lot (id, lot_type, price, buy_date, seller_id, no_card_price)
         VALUES (2, 'purchase', '5.00', '2025-06-13', 1, '0.00');",
    )
    .await;
    t.post(&format!("/lots/{lot}/deck"), DECK_ROWS).await;
    let snapshot = t.scalar("SELECT id FROM deck_snapshot").await;

    for uri in [
        format!("/lots/2/deck/{snapshot}"),
        format!("/lots/{lot}/deck/999"),
    ] {
        assert_eq!(t.get(&uri).await.0, StatusCode::NOT_FOUND, "{uri}");
        let (status, _) = t.post(&uri, &[("cards-0-name", "Kuriboh")]).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    assert_eq!(
        t.scalar("SELECT count(*) FROM unite WHERE lot_id = 2")
            .await,
        0
    );
}

#[tokio::test]
async fn the_lot_deck_needs_a_login() {
    let t = TestApp::with_password(Some("secret")).await;
    let lot = lot_for_deck(&t).await;
    let uri = format!("/lots/{lot}/deck");
    assert_eq!(t.get(&uri).await.0, StatusCode::SEE_OTHER);
    assert_eq!(t.post(&uri, DECK_ROWS).await.0, StatusCode::SEE_OTHER);
    let more = format!("{uri}/1");
    assert_eq!(t.get(&more).await.0, StatusCode::SEE_OTHER);
    assert_eq!(t.post(&more, DECK_ROWS).await.0, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 0);
}
