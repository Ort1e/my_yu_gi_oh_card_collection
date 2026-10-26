//! Names typed or fetched (cards, decks) never inject HTML in the pages.

use super::*;

#[tokio::test]
async fn names_cannot_inject_html() {
    let t = TestApp::new().await;
    let attack = "</script><script>alert(1)</script><img src=x onerror=alert(2)>";
    sqlx::query(
        "INSERT INTO card (id, name, en_name, code, card_data_id, is_proxy) VALUES (1, ?1, 'x', 'LOB-FR005', 1, 0);
         INSERT INTO deck (id, name) VALUES (1, ?1);
         INSERT INTO deck_version (id, deck_id, version_name) VALUES (1, 1, ?1);
         INSERT INTO deck_version_main_deck (deck_version_id, card_id) VALUES (1, 1);",
    )
    .bind(attack)
    .execute(&t.state.pool)
    .await
    .unwrap();

    for page in [
        "/",
        "/cards",
        "/cards?view=table",
        "/cards/1",
        "/decks",
        "/decks/1",
        "/collection/value",
    ] {
        let (status, body) = t.get(page).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(!body.contains("<script>alert"), "{page}");
        assert!(!body.contains("<img src=x"), "{page}");
    }
    let (_, cards) = t.get("/cards").await;
    assert!(
        cards.contains("&lt;script&gt;alert(1)"),
        "the name is shown, escaped"
    );
}
