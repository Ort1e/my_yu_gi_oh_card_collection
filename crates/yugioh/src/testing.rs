//! The Yu-Gi-Oh! test fixtures (feature `test-support`): shared by this
//! crate's tests and the backend's cross-collection tests.

use common::testing::TestApp;

/// A seller source and two card data rows (Dark Magician, Kuriboh) every
/// Yu-Gi-Oh! test may use, so nothing calls ygoprodeck.
pub async fn insert_card_data(t: &TestApp) {
    t.exec(
        "INSERT INTO seller_source (id, name) VALUES (1, 'Cardmarket');
         INSERT INTO card_data (id, en_name, ygopro_id, card_type, json_data)
         VALUES (1, 'dark magician', 46986414, 'Normal Monster', '{}'),
                (2, 'kuriboh', 40640057, 'Effect Monster', '{}');",
    )
    .await;
}

pub const LOT: &[(&str, &str)] = &[
    ("lot_type", "purchase"),
    ("price", "10,50"),
    ("buy_date", "2025-06-12"),
    ("received_date", ""),
    ("no_card_price", "0.00"),
    ("seller_select", ""),
    ("seller_name", "New Shop"),
    ("seller_source", "1"),
    ("cards-0-name", "Magicien Sombre"),
    ("cards-0-en_name", "Dark Magician (V.1 - Ultra Rare)"),
    ("cards-0-code", "LOB-FR005"),
    ("cards-0-status", "NM"),
    ("cards-0-price", "2.25"),
    ("cards-0-quantity", "2"),
    ("cards-3-name", "Kuriboh"),
    ("cards-3-is_proxy", "on"),
];

/// A deck with one version, and a lot with 2 owned Dark Magicians and a
/// Kuriboh proxy. Returns (version id, [dm1, dm2, kuriboh]).
pub async fn deck_fixture(t: &TestApp) -> (i64, Vec<i64>) {
    t.post("/decks/add", &[("name", "Test"), ("version_name", "v1")])
        .await;
    // the lot of `LOT`, inserted directly: saving it through the form
    // would start the background task (link, translate, replace proxies),
    // which would race with the test
    sqlx::query(
        "INSERT INTO seller (name, is_person, source_id) VALUES ('New Shop', 0, 1);
         INSERT INTO lot (lot_type, price, buy_date, seller_id, no_card_price)
             VALUES ('purchase', '10.50', '2025-06-12', 1, '0.00');
         INSERT INTO card (name, en_name, code, last_known_status, card_data_id, is_proxy) VALUES
             ('Magicien Sombre', 'dark magician', 'LOB-FR005', 'NM', 1, 0),
             ('Magicien Sombre', 'dark magician', 'LOB-FR005', 'NM', 1, 0),
             ('Kuriboh', 'kuriboh', NULL, NULL, 2, 1);
         INSERT INTO unite (price, lot_id, card_id)
             SELECT CASE WHEN is_proxy THEN NULL ELSE '2.25' END, 1, id FROM card ORDER BY id;",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();
    let version = t.scalar("SELECT id FROM deck_version").await;
    let cards = sqlx::query_scalar("SELECT id FROM card ORDER BY id")
        .fetch_all(&t.state.pool)
        .await
        .unwrap();
    (version, cards)
}
