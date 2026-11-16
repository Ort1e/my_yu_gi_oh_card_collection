//! Copies "to sell": the flag, the list by set, a sale lot taking the flagged
//! copies (`store::cards`, `routes::cards::for_sale`), the builds leaving them
//! for last.

use serde_json::json;

use super::*;
use crate::testing::DM;

async fn for_sale(t: &TestApp, card: i64) -> i64 {
    t.scalar(&format!("SELECT for_sale FROM card WHERE id = {card}"))
        .await
}

#[tokio::test]
async fn flag_and_unflag_from_the_card_page() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;

    let (status, location) = t
        .post(
            &format!("/cards/{}/for-sale", cards[0]),
            &[("for_sale", "1")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, format!("/cards/{}", cards[0]));
    assert_eq!(for_sale(&t, cards[0]).await, 1);
    assert_eq!(for_sale(&t, cards[1]).await, 0);

    let (_, page) = t.get(&format!("/cards/{}", cards[0])).await;
    assert!(page.contains("Not to sell any more"), "{page}");
    let (_, list) = t.get("/cards?for_sale=1").await;
    assert!(list.contains(&format!("/cards/{}\"", cards[0])), "{list}");
    assert!(!list.contains(&format!("/cards/{}\"", cards[1])), "{list}");
    assert!(list.contains("To sell"), "{list}");

    t.post(
        &format!("/cards/{}/for-sale", cards[0]),
        &[("for_sale", "0")],
    )
    .await;
    assert_eq!(for_sale(&t, cards[0]).await, 0);
}

#[tokio::test]
async fn the_lot_page_marks_its_copies() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;
    let (_, page) = t.get("/lots/1").await;
    assert!(page.contains(r#"name="group""#), "{page}");

    let group = format!("{},{}", cards[0], cards[1]);
    let (status, _) = t
        .post("/lots/1/for-sale", &[("action", "mark"), ("group", &group)])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(for_sale(&t, cards[0]).await, 1);
    assert_eq!(for_sale(&t, cards[1]).await, 1);

    t.post(
        "/lots/1/for-sale",
        &[("action", "unmark"), ("group", &group)],
    )
    .await;
    assert_eq!(for_sale(&t, cards[0]).await, 0);

    // a card of another lot is left alone
    t.exec("INSERT INTO card (id, name) VALUES (99, 'Other')")
        .await;
    t.post("/lots/1/for-sale", &[("action", "mark"), ("group", "99")])
        .await;
    assert_eq!(for_sale(&t, 99).await, 0);
}

#[tokio::test]
async fn the_list_is_by_set_with_an_other_section() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;
    t.exec(
        "INSERT INTO ygo_set (name, code, tcg_date)
             VALUES ('Legend of Blue Eyes White Dragon', 'LOB', '2002-03-08');
         INSERT INTO card (id, name, code, last_known_status) VALUES (50, 'Mystery', 'ZZZ-EN001', 'EX');
         UPDATE card SET for_sale = 1 WHERE id IN (1, 2, 50)",
    )
    .await;
    assert_eq!(cards.len(), 2);

    let (status, page) = t.get("/cards/to-sell").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Legend of Blue Eyes White Dragon"), "{page}");
    assert!(page.contains("Other"), "{page}");
    // the two Dark Magicians are one row of the list, with their count
    assert!(page.contains("<td>2</td>"), "{page}");
    assert!(page.contains("005"), "{page}");
}

#[tokio::test]
async fn a_sale_lot_takes_the_flagged_copies() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    // a built version holding both copies would need `take_order`: the slots
    // are inserted by hand
    t.set_main(v1, &[(DM, "Magicien Sombre", 2)]).await;
    t.exec(&format!(
        "INSERT INTO deck_version_copy (deck_version_id, zone, position, slot, card_id)
         SELECT {v1}, zone, position, 0, {} FROM deck_version_card WHERE deck_version_id = {v1}
         UNION ALL SELECT {v1}, zone, position, 1, {} FROM deck_version_card WHERE deck_version_id = {v1};
         UPDATE card SET for_sale = 1 WHERE id = {}",
        cards[0], cards[1], cards[1]
    ))
    .await;

    let (status, _) = t
        .post(
            "/lots/add",
            &[
                ("lot_type", "sale"),
                ("price", "3,00"),
                ("buy_date", "2025-07-01"),
                ("seller_name", "Buyer"),
                ("cards-0-name", "Magicien Sombre"),
                ("cards-0-code", "LOB-FR005"),
                ("cards-0-status", "NM"),
                ("cards-0-price", "3.00"),
                ("cards-0-quantity", "1"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    // the same card row is in the sale lot, no new card
    assert_eq!(t.scalar("SELECT count(*) FROM card").await, 2);
    let sale = t.scalar("SELECT max(id) FROM lot").await;
    assert_eq!(
        t.scalar(&format!("SELECT card_id FROM unite WHERE lot_id = {sale}"))
            .await,
        cards[1]
    );
    assert_eq!(for_sale(&t, cards[1]).await, 0);
    // its purchase stays, and its built slot is empty
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM unite WHERE card_id = {} AND lot_id = 1",
            cards[1]
        ))
        .await,
        1
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_version_copy WHERE card_id = {}",
            cards[1]
        ))
        .await,
        0
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_version_copy WHERE card_id = {}",
            cards[0]
        ))
        .await,
        1
    );

    // a line with nothing flagged left makes a new card, as before
    t.post(
        "/lots/add",
        &[
            ("lot_type", "sale"),
            ("price", "1.00"),
            ("buy_date", "2025-07-02"),
            ("seller_name", "Buyer"),
            ("cards-0-name", "Magicien Sombre"),
            ("cards-0-code", "LOB-FR005"),
            ("cards-0-status", "NM"),
        ],
    )
    .await;
    assert_eq!(t.scalar("SELECT count(*) FROM card").await, 3);
}

#[tokio::test]
async fn building_takes_the_flagged_copy_last() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    // the older copy is flagged: the newer one goes first
    t.exec(&format!(
        "UPDATE card SET for_sale = 1 WHERE id = {}",
        cards[0]
    ))
    .await;
    t.set_main(v1, &[(DM, "Magicien Sombre", 1)]).await;
    let built = t.patch(v1, json!({ "is_built": true })).await;
    assert_eq!(built["main"][0]["copy"]["id"], cards[1]);
    assert_eq!(built["main"][0]["for_sale"], false);

    // taken when it is the only one left: the tile says so
    t.set_main(v1, &[(DM, "Magicien Sombre", 2)]).await;
    let (_, deck) = t
        .json("GET", &format!("/api/deck-versions/{v1}"), json!(null))
        .await;
    let flagged: Vec<bool> = deck["main"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["for_sale"].as_bool().unwrap())
        .collect();
    assert_eq!(flagged.iter().filter(|f| **f).count(), 1, "{deck}");
}
