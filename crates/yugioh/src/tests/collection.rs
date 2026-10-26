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
    assert!(body.contains("Market prices of 2026-10-01"), "{body}");
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
    // nothing lost
    assert!(body.contains("No copy is worth less"), "{body}");
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
        body.contains("6.00 € on 2026-10-01 (this copy ≈ 5.40 €)"),
        "{body}"
    );
    assert!(body.contains("Market (estimate):"), "{body}");
    assert!(body.contains("<polyline"), "{body}");
}

#[tokio::test]
async fn lists_the_copies_that_lost_the_most() {
    let t = TestApp::new().await;
    let cards = priced_fixture(&t).await;
    // the second Dark Magician cost 8.00: worth 5.40, so −2.60
    sqlx::query("UPDATE unite SET price = '8.00' WHERE card_id = ?")
        .bind(cards[1])
        .execute(&t.state.pool)
        .await
        .unwrap();

    let (status, body) = t.get("/collection/value").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let losses = &body[body.find("Biggest losses").unwrap()..];
    let losses = &losses[..losses.find("</table>").unwrap()];
    let row = |card_id| format!(r#"href="/cards/{card_id}""#);
    assert!(losses.contains(&row(cards[1])), "{losses}");
    assert!(losses.contains("<td>8.00</td>"), "{losses}");
    assert!(losses.contains("<td>-2.60</td>"), "{losses}");
    // the other copy gained: not listed
    assert!(!losses.contains(&row(cards[0])), "{losses}");
}

#[tokio::test]
async fn each_lot_against_its_full_price() {
    let t = TestApp::new().await;
    let cards = priced_fixture(&t).await;

    let (status, body) = t.get("/collection/value").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let lots = &body[body.find("Gain / loss by lot").unwrap()..];
    let lots = &lots[..lots.find("</table>").unwrap()];
    // 2 copies at 5.40 against the 10.50 lot
    assert!(lots.contains(r#"<a href="/lots/1">#1</a>"#), "{lots}");
    assert!(lots.contains("<td>10.50</td>"), "{lots}");
    assert!(lots.contains("<td>10.80</td>"), "{lots}");
    assert!(lots.contains("0.30\n"), "{lots}");
    assert!(lots.contains(">+2.9%</td>"), "{lots}");
    // a gain: in the gains, no loss at all
    let gains = &body[body.find("Biggest gains").unwrap()..body.find("Worst losses").unwrap()];
    assert!(gains.contains(r#"<a href="/lots/1">#1</a>"#), "{gains}");
    assert!(
        body.contains("No lot is worth less than it cost."),
        "{body}"
    );

    // one copy sold for 7.00: its sale counts instead of its value
    sqlx::query(
        "INSERT INTO lot (id, lot_type, price, buy_date) VALUES (2, 'sale', '7.00', '2025-07-01');
         INSERT INTO unite (price, lot_id, card_id) VALUES ('7.00', 2, ?);",
    )
    .bind(cards[1])
    .execute(&t.state.pool)
    .await
    .unwrap();
    let (_, body) = t.get("/collection/value").await;
    let lots = &body[body.find("Gain / loss by lot").unwrap()..];
    let lots = &lots[..lots.find("</table>").unwrap()];
    assert!(lots.contains("1 + 1 sold"), "{lots}");
    assert!(lots.contains("<td>5.40</td>"), "{lots}");
    assert!(lots.contains(">7.00</td>"), "{lots}");
    assert!(lots.contains("1.90\n"), "{lots}");
    // the sale lot isn't listed
    assert!(!lots.contains("/lots/2"), "{lots}");

    // nor a lot without cards
    sqlx::query("INSERT INTO lot (id, lot_type, price, buy_date, no_card_price) VALUES (3, 'purchase', '20.00', '2025-08-01', '19.00')")
        .execute(&t.state.pool)
        .await
        .unwrap();
    let (_, body) = t.get("/collection/value").await;
    assert!(!body.contains(r#"href="/lots/3""#), "{body}");
}

#[tokio::test]
async fn the_lists_show_the_estimates() {
    let t = TestApp::new().await;
    priced_fixture(&t).await;

    // French NM Dark Magicians at 6.00 × 0.9: on their pictures, and in the table
    let (status, cards) = t.get("/cards").await;
    assert_eq!(status, StatusCode::OK, "{cards}");
    assert!(
        cards.contains(r#"<span class="card-tile-value">5.40 €</span>"#),
        "{cards}"
    );
    let (status, table) = t.get("/cards?view=table").await;
    assert_eq!(status, StatusCode::OK, "{table}");
    assert!(table.contains("<td>5.40</td>"), "{table}");

    let (_, lots) = t.get("/lots").await;
    assert!(lots.contains("Estimate: 10.80 €"), "{lots}");

    let (_, lot) = t.get("/lots/1").await;
    assert!(lot.contains("Estimated value:</strong> 10.80 €"), "{lot}");
    assert!(lot.contains("<td>5.40</td>"), "{lot}");

    let (_, seller) = t.get("/sellers/1").await;
    assert!(seller.contains("Estimate: 10.80 €"), "{seller}");
}
