//! Card rarity: lot card rows, card edition, the cards filter, the lot page
//! and the deck builder API.

use super::*;
use serde_json::json;

/// `LOT` with a rarity on its first card row.
fn lot_with_rarity(rarity: &'static str) -> Vec<(&'static str, &'static str)> {
    let mut form = LOT.to_vec();
    form.push(("cards-0-rarity", rarity));
    form
}

#[tokio::test]
async fn lot_card_rows_store_the_rarity_label() {
    let t = TestApp::new().await;
    // any spelling is accepted, the label is stored
    let (status, _) = t
        .post("/lots/add", &lot_with_rarity("Collectors Rare"))
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.scalar("SELECT count(*) FROM card WHERE rarity = 'Collector''s Rare'")
            .await,
        2,
        "both copies of the row"
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM card WHERE rarity IS NULL")
            .await,
        1
    );

    let lot_id = t.scalar("SELECT id FROM lot").await;
    let (_, page) = t.get(&format!("/lots/{lot_id}")).await;
    assert!(
        page.contains("/rarities?rarity=Collector%27s%20Rare"),
        "badge on the lot page"
    );
}

#[tokio::test]
async fn unknown_rarity_is_an_error() {
    let t = TestApp::new().await;
    let (status, page) = t.post("/lots/add", &lot_with_rarity("Shiny")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Card 1: unknown rarity."), "{page}");
    assert_eq!(t.scalar("SELECT count(*) FROM lot").await, 0);
}

#[tokio::test]
async fn cards_filter_and_edit() {
    let t = TestApp::new().await;
    t.post("/lots/add", &lot_with_rarity("Ultra Rare")).await;

    let (status, page) = t.get("/cards?rarity=Ultra%20Rare").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Magicien Sombre") && !page.contains("Kuriboh"));

    let kuriboh = t.scalar("SELECT id FROM card WHERE name = 'Kuriboh'").await;
    let (status, _) = t
        .post(
            &format!("/cards/{kuriboh}/edit"),
            &[("name", "Kuriboh"), ("rarity", "Secret Rare")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.text(&format!("SELECT rarity FROM card WHERE id = {kuriboh}"))
            .await
            .as_deref(),
        Some("Secret Rare")
    );

    // and back to unknown
    t.post(&format!("/cards/{kuriboh}/edit"), &[("name", "Kuriboh")])
        .await;
    assert_eq!(
        t.text(&format!("SELECT rarity FROM card WHERE id = {kuriboh}"))
            .await,
        None
    );
}

#[tokio::test]
async fn deck_builder_cards_have_their_rarity() {
    let t = TestApp::new().await;
    let (version, cards) = t.deck_fixture().await;
    sqlx::query("UPDATE card SET rarity = 'Ultra Rare' WHERE id = ?")
        .bind(cards[0])
        .execute(&t.state.pool)
        .await
        .unwrap();

    let (status, found) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{version}/reserve"),
            json!({ "name": "magicien" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let rarity = |id: i64| {
        found
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .map(|c| c["rarity"].clone())
    };
    assert_eq!(rarity(cards[0]), Some(json!("Ultra Rare")));
    assert_eq!(rarity(cards[1]), Some(json!(null)));
}
