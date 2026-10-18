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
    assert_eq!(
        t.scalar("SELECT is_proxy FROM card WHERE name = 'Kuriboh'")
            .await,
        1
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
    assert!(
        page.contains("Magicien Sombre") && page.contains("4.50 €"),
        "{page}"
    );
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

    let uri = format!("/lots/{lot_id}/cards");
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
        t.post("/lots/999/cards", &[("cards-0-name", "X")]).await.0,
        StatusCode::NOT_FOUND
    );
}
