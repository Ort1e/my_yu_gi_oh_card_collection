//! `/collection/value`, the dashboard's value tile and the card page's market
//! price.

use super::*;

/// `deck_fixture` (2 Dark Magicians LOB-FR005 NM at 2.25 in a 10.50 lot, a
/// Kuriboh proxy) with both cards in the deck, and two price refreshes.
async fn priced_fixture(t: &TestApp) -> Vec<i64> {
    let (version, cards) = t.deck_fixture().await;
    sqlx::query(
        "INSERT INTO deck_version_main_deck (deck_version_id, card_id) VALUES (?1, ?2), (?1, ?3);
         INSERT INTO card_price (card_data_id, date, cardmarket_price, tcgplayer_price) VALUES
             (1, '2026-09-01', '4.00', '5.00'), (1, '2026-10-01', '6.00', '7.00'),
             (2, '2026-09-01', '0.10', NULL), (2, '2026-10-01', '1.00', NULL);",
    )
    .bind(version)
    .bind(cards[0])
    .bind(cards[2])
    .execute(&t.state.pool)
    .await
    .unwrap();
    cards
}

#[tokio::test]
async fn the_page_says_when_there_is_no_price_yet() {
    let t = TestApp::new().await;
    t.deck_fixture().await;

    let (status, body) = t.get("/collection/value").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("No price yet"), "{body}");
    assert!(!body.contains("value-chart-data"));

    let (_, dashboard) = t.get("/").await;
    assert!(dashboard.contains("No price yet"), "{dashboard}");
}

#[tokio::test]
async fn values_the_collection_from_lot_prices() {
    let t = TestApp::new().await;
    priced_fixture(&t).await;

    let (status, body) = t.get("/collection/value").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("last refresh 2026-10-01"), "{body}");
    // 2 French NM copies at 6.00 × 0.9; the proxy isn't owned
    assert!(body.contains("10.80 €"), "{body}");
    assert!(body.contains("2 copies"), "{body}");
    // gain: value − the lot's price (10.50)
    assert!(body.contains("10.50 €"), "{body}");
    assert!(body.contains("0.30 €"), "{body}");
    // copies with a unit price: 10.80 against 2 × 2.25
    assert!(body.contains("paid 4.50 €"), "{body}");
    assert!(body.contains("6.30 €"), "{body}");
    assert!(body.contains("(+140.0%)"), "{body}");
    // Dark Magician moved +50%; Kuriboh more, but it's only a proxy
    assert!(body.contains(">+50.0%</td>"), "{body}");
    assert!(!body.contains("+900.0%"), "{body}");
    // the deck: one real card (5.40), the proxy would cost 1.00
    assert!(body.contains("<td>5.40</td>"), "{body}");
    assert!(body.contains("<td>1.00</td>"), "{body}");
    assert!(body.contains(r#""value":[7.2,10.8]"#), "{body}");
}

#[tokio::test]
async fn the_dashboard_shows_the_value_and_its_change() {
    let t = TestApp::new().await;
    priced_fixture(&t).await;

    let (status, body) = t.get("/").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("10.80 €"), "{body}");
    assert!(body.contains("3.60 €</span> over 4 weeks"), "{body}");
}

#[tokio::test]
async fn the_card_page_shows_its_market_price() {
    let t = TestApp::new().await;
    let cards = priced_fixture(&t).await;

    let (status, body) = t.get(&format!("/cards/{}", cards[0])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("Market: 6.00 € on 2026-10-01 (this copy ≈ 5.40 €)"),
        "{body}"
    );
    assert!(body.contains("<polyline"), "{body}");
}
