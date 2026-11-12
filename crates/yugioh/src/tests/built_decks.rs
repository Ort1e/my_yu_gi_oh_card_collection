//! Built decks: only a built version holds copies (`store::lists`,
//! `decks/api/version.rs`); building takes free copies, best condition first,
//! and never touches the other versions.

use serde_json::{json, Value};

use super::*;
use crate::testing::{tiles, DM, KURIBOH};

/// A new version of the fixture's deck listing these main deck cards.
async fn version(t: &TestApp, name: &str, cards: &[(Option<i64>, &str, usize)]) -> i64 {
    let id = sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (1, ?)")
        .bind(name)
        .execute(&t.state.pool)
        .await
        .unwrap()
        .last_insert_rowid();
    t.set_main(id, cards).await;
    id
}

async fn set_built(t: &TestApp, version: i64, built: bool) -> Value {
    t.patch(version, json!({ "is_built": built })).await
}

/// The copies a version holds, in slot order (`None`: missing).
async fn slots(t: &TestApp, version: i64) -> Vec<Option<i64>> {
    sqlx::query_scalar(
        "SELECT card_id FROM deck_version_copy WHERE deck_version_id = ?
         ORDER BY zone, position, slot",
    )
    .bind(version)
    .fetch_all(&t.state.pool)
    .await
    .unwrap()
}

/// `missing` of each Main Deck tile.
fn missing(deck: &Value) -> Vec<bool> {
    deck["main"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["missing"].as_bool().unwrap())
        .collect()
}

const DM2: (Option<i64>, &str, usize) = (DM, "Magicien Sombre", 2);

#[tokio::test]
async fn building_takes_free_copies_and_leaves_the_others_alone() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    // the second copy in better condition: taken first
    t.exec(&format!(
        "UPDATE card SET last_known_status = 'EX' WHERE id = {}",
        cards[0]
    ))
    .await;
    t.set_main(v1, &[(DM, "Magicien Sombre", 1), (KURIBOH, "Kuriboh", 1)])
        .await;
    let v2 = version(&t, "v2", &[DM2]).await;

    let built = set_built(&t, v1, true).await;
    assert_eq!(built["is_built"], true);
    assert_eq!(built["build_report"], json!({ "copies": 1, "missing": 1 }));
    assert_eq!(slots(&t, v1).await, vec![Some(cards[1]), None]);
    assert_eq!(built["main"][0]["copy"]["id"], cards[1]);
    assert_eq!(built["main"][0]["here"], 1);
    assert_eq!(missing(&built), vec![false, true]);

    // the other version holds nothing and lost nothing: one copy is free
    // for it now, so one of its two is missing
    assert!(slots(&t, v2).await.is_empty());
    let (_, other) = t
        .json("GET", &format!("/api/deck-versions/{v2}"), json!(null))
        .await;
    assert_eq!(missing(&other), vec![false, true]);
    assert_eq!(other["main"][0]["free"], 1);

    // unbuilt: the copy is free again
    let unbuilt = set_built(&t, v1, false).await;
    assert_eq!(unbuilt["is_built"], false);
    assert!(slots(&t, v1).await.is_empty());
    let (_, other) = t
        .json("GET", &format!("/api/deck-versions/{v2}"), json!(null))
        .await;
    assert_eq!(missing(&other), vec![false, false]);
}

#[tokio::test]
async fn the_first_built_deck_keeps_its_copies() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    t.set_main(v1, &[DM2]).await;
    let v2 = version(&t, "v2", &[(DM, "Magicien Sombre", 1)]).await;

    set_built(&t, v1, true).await;
    let built = set_built(&t, v2, true).await;
    assert_eq!(built["build_report"], json!({ "copies": 0, "missing": 1 }));
    assert_eq!(slots(&t, v1).await, vec![Some(cards[0]), Some(cards[1])]);
    assert_eq!(slots(&t, v2).await, vec![None]);

    // the card page: held by a built version
    let (_, page) = t.get(&format!("/cards/{}", cards[0])).await;
    assert!(page.contains("built-badge"), "{page}");
}

#[tokio::test]
async fn a_built_version_is_public_by_default_and_no_longer_planned() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    t.set_main(v1, &[(DM, "Magicien Sombre", 1)]).await;
    assert_eq!(
        t.patch(v1, json!({ "is_planned": true })).await["is_planned"],
        true
    );

    let built = set_built(&t, v1, true).await;
    assert_eq!(built["is_public"], true);
    assert_eq!(built["is_planned"], false);
    // a built version can't be planned
    assert_eq!(
        t.patch(v1, json!({ "is_planned": true })).await["is_planned"],
        false
    );

    // hidden by hand, it stays hidden: saving its zones doesn't show it again
    let body = t
        .patch(v1, json!({ "is_public": false, "main": tiles(&[DM2]) }))
        .await;
    assert_eq!(body["is_public"], false);

    // unbuilding leaves the switch alone; built again, it is shown again
    assert_eq!(set_built(&t, v1, false).await["is_public"], false);
    assert_eq!(set_built(&t, v1, true).await["is_public"], true);

    // built while hidden in the same request: the request wins
    let v2 = version(&t, "v2", &[(KURIBOH, "Kuriboh", 1)]).await;
    let body = t
        .patch(v2, json!({ "is_built": true, "is_public": false }))
        .await;
    assert_eq!(body["is_public"], false);
}

#[tokio::test]
async fn saving_a_built_version_keeps_its_copies_and_fills_new_lines() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    t.set_main(v1, &[(DM, "Magicien Sombre", 1), (KURIBOH, "Kuriboh", 1)])
        .await;
    set_built(&t, v1, true).await;
    assert_eq!(slots(&t, v1).await, vec![Some(cards[0]), None]);

    // a second Dark Magician: the free copy is taken; Kuriboh still missing
    t.set_main(v1, &[(KURIBOH, "Kuriboh", 1), DM2]).await;
    assert_eq!(
        slots(&t, v1).await,
        vec![None, Some(cards[0]), Some(cards[1])]
    );

    // a line removed frees its copies
    t.set_main(v1, &[(KURIBOH, "Kuriboh", 1)]).await;
    assert_eq!(slots(&t, v1).await, vec![None]);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version_copy WHERE card_id IS NOT NULL")
            .await,
        0
    );
}

#[tokio::test]
async fn a_copy_bought_later_waits_for_take_free_copies() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    t.set_main(v1, &[(KURIBOH, "Kuriboh", 2)]).await;
    set_built(&t, v1, true).await;
    assert_eq!(slots(&t, v1).await, vec![None, None]);

    // bought: free, so not to buy, but not in the deck until taken
    let bought = sqlx::query(
        "INSERT INTO card (name, en_name, card_data_id) VALUES ('Kuriboh', 'kuriboh', 2)",
    )
    .execute(&t.state.pool)
    .await
    .unwrap()
    .last_insert_rowid();
    let (_, deck) = t
        .json("GET", &format!("/api/deck-versions/{v1}"), json!(null))
        .await;
    assert_eq!(missing(&deck), vec![false, true]);
    assert_eq!(deck["main"][0]["copy"], json!(null));
    assert_eq!(slots(&t, v1).await, vec![None, None]);

    let (status, taken) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{v1}/take-free"),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{taken}");
    assert_eq!(taken, json!({ "taken": 1, "missing": 1 }));
    assert_eq!(slots(&t, v1).await, vec![Some(bought), None]);

    // a plain version holds no copies
    let v2 = version(&t, "v2", &[]).await;
    let (status, _) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{v2}/take-free"),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_sold_copy_leaves_the_built_deck_on_its_next_save() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    t.set_main(v1, &[DM2]).await;
    set_built(&t, v1, true).await;
    t.exec(&format!(
        "INSERT INTO lot (id, lot_type, price, buy_date) VALUES (9, 'sale', '1.00', '2026-01-01');
         INSERT INTO unite (lot_id, card_id) VALUES (9, {})",
        cards[0]
    ))
    .await;
    let (_, deck) = t
        .json("GET", &format!("/api/deck-versions/{v1}"), json!(null))
        .await;
    assert_eq!(deck["main"][0]["owned"], 1, "sold: not owned");

    t.set_main(v1, &[DM2]).await;
    assert_eq!(slots(&t, v1).await, vec![Some(cards[1]), None]);
}

#[tokio::test]
async fn a_copy_of_a_built_version_is_a_plain_list() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    t.set_main(v1, &[DM2]).await;
    set_built(&t, v1, true).await;

    let (status, copy) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{v1}/clone"),
            json!({ "name": "v2" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{copy}");
    let copy = copy["id"].as_i64().unwrap();
    assert!(slots(&t, copy).await.is_empty());
    let (_, body) = t
        .json("GET", &format!("/api/deck-versions/{copy}"), json!(null))
        .await;
    assert_eq!(body["is_built"], false);
    assert_eq!(
        missing(&body),
        vec![true, true],
        "the copies are held by v1"
    );
}

#[tokio::test]
async fn deck_box_and_sleeves() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;

    let body = t
        .patch(
            v1,
            json!({ "deck_box": " red Boulder 100+ ", "sleeves": "Dragon Shield matte black" }),
        )
        .await;
    assert_eq!(body["deck_box"], "red Boulder 100+");
    assert_eq!(body["sleeves"], "Dragon Shield matte black");

    // absent: kept; blank or null: cleared
    let body = t.patch(v1, json!({ "deck_box": "" })).await;
    assert_eq!(body["deck_box"], Value::Null);
    assert_eq!(body["sleeves"], "Dragon Shield matte black");
    let body = t.patch(v1, json!({ "sleeves": null })).await;
    assert_eq!(body["sleeves"], Value::Null);
}

#[tokio::test]
async fn the_built_page_lists_the_built_decks() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    t.set_main(v1, &[(DM, "Magicien Sombre", 1), (KURIBOH, "Kuriboh", 1)])
        .await;
    t.exec(&format!(
        "UPDATE deck_version SET deck_box = 'red box' WHERE id = {v1}"
    ))
    .await;
    set_built(&t, v1, true).await;

    let (status, page) = t.get("/decks/built").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Test - v1"), "{page}");
    assert!(page.contains("Deck box: red box"), "{page}");
    assert!(page.contains("Cards: 2 (1 of your copies)"), "{page}");
    assert!(page.contains("To buy: 1"), "{page}");
    assert!(page.contains("/decks/shopping"), "{page}");

    // the shopping list: the missing card
    let (status, page) = t.get("/decks/shopping").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Missing: 1 copy"), "{page}");
    assert!(page.contains("data-name=\"kuriboh\""), "{page}");
    assert!(page.contains("Estimated price"), "{page}");
    assert!(page.contains("No budget set"), "{page}");

    // the badge and the missing count on the deck page
    let (_, deck) = t.get("/decks/1").await;
    assert!(deck.contains("built-badge"), "{deck}");
    assert!(
        deck.contains(&format!("/deck-builder/{v1}#to-buy\">1</a>")),
        "{deck}"
    );
}

#[tokio::test]
async fn planned_decks_share_the_copies_on_the_shopping_list() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    t.exec("UPDATE card_data SET json_data = '{\"name\":\"Dark Magician\"}' WHERE id = 1")
        .await;
    // 2 owned: the built deck takes them, the planned one lacks all 3
    t.set_main(v1, &[DM2]).await;
    set_built(&t, v1, true).await;
    let v2 = version(&t, "v2", &[(DM, "Magicien Sombre", 3)]).await;
    let v3 = version(&t, "v3", &[(DM, "Magicien Sombre", 3)]).await;

    let (_, page) = t.get("/decks/shopping").await;
    assert!(!page.contains("Test - v2"), "not planned yet: {page}");
    t.patch(v2, json!({ "is_planned": true })).await;

    let (status, page) = t.get("/decks/shopping").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Test - v2"), "{page}");
    assert!(page.contains("Planned"), "{page}");
    assert!(
        !page.contains(&format!("/deck-builder/{v3}")),
        "not planned: {page}"
    );
    assert!(page.contains(">3 Dark Magician</textarea>"), "{page}");

    // a second planned deck: 3 more, not counting the same copies twice
    t.patch(v3, json!({ "is_planned": true })).await;
    let (_, page) = t.get("/decks/shopping").await;
    assert!(page.contains(">6 Dark Magician</textarea>"), "{page}");
}

#[tokio::test]
async fn the_built_page_gives_a_cardmarket_wants_list_by_english_name() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    // the card data's English name has its capitals (`en_name` is lowercase)
    t.exec(
        "UPDATE card_data SET json_data = '{\"name\":\"Kuriboh\"}' WHERE id = 2;
         UPDATE card_data SET json_data = '{\"name\":\"Dark Magician\"}' WHERE id = 1;",
    )
    .await;
    // 3 Dark Magicians (2 owned), a Kuriboh
    t.set_main(v1, &[(DM, "Magicien Sombre", 3), (KURIBOH, "Kuriboh", 1)])
        .await;
    set_built(&t, v1, true).await;

    let (status, page) = t.get("/decks/shopping").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains(&format!("/deck-builder/{v1}#to-buy")),
        "{page}"
    );
    assert!(page.contains("Min. condition: Good"), "{page}");
    assert!(
        page.contains(">1 Kuriboh\n1 Dark Magician</textarea>"),
        "{page}"
    );
    assert!(!page.contains("Magicien Sombre</textarea>"), "{page}");
    assert!(page.contains("data-copy=\"wants-all\""), "{page}");
}

#[tokio::test]
async fn the_built_page_estimates_the_wants_list() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    // Kuriboh at 0.75, a third Dark Magician without a price
    t.exec("INSERT INTO card_price (card_data_id, date, cardmarket_price) VALUES (2, '2026-11-01', '0.75')")
        .await;
    t.set_main(v1, &[(DM, "Magicien Sombre", 3), (KURIBOH, "Kuriboh", 1)])
        .await;
    set_built(&t, v1, true).await;

    let (status, page) = t.get("/decks/shopping").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("data-price=\"0.75\""), "{page}");
    assert!(page.contains("<strong id=\"wants-total\">"), "{page}");
    assert!(page.contains("without a price"), "{page}");
}

#[tokio::test]
async fn any_version_gives_its_wants_list() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    t.exec("UPDATE card_data SET json_data = '{\"name\":\"Kuriboh\"}' WHERE id = 2")
        .await;

    // not built: the builder's To buy tab gets the list from the check
    t.set_main(v1, &[(DM, "Magicien Sombre", 1), (KURIBOH, "Kuriboh", 1)])
        .await;
    let (status, check) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{v1}/check"),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{check}");
    assert_eq!(check["wants"], "1 Kuriboh");

    // the read-only page has no wants list
    let (_, page) = t.get(&format!("/deck-versions/{v1}")).await;
    assert!(!page.contains("wants-version"), "{page}");

    // nothing missing: no wants list
    t.set_main(v1, &[DM2]).await;
    let (_, check) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{v1}/check"),
            json!(null),
        )
        .await;
    assert_eq!(check["wants"], "");
}

/// Dark Magician plays the part of a card errata'd in 2016 ("New Pack"),
/// printed before in "Metal Raiders"; the fixture's version plays the format
/// "Edison" (cutoff 2010-03-01) with 2 Dark Magicians: one owned copy coded
/// `copy_code`, one missing. Nothing is asked to Yugipedia: its errata are
/// cached.
async fn errata_fixture(t: &TestApp, copy_code: &str) -> i64 {
    let (v1, cards) = t.deck_fixture().await;
    t.exec(&format!(
        r#"UPDATE card_data SET json_data = '{{"name":"Dark Magician","card_sets":[
               {{"set_name":"Metal Raiders","set_code":"MRD-069","set_rarity":"Common"}},
               {{"set_name":"New Pack","set_code":"NEW-EN001","set_rarity":"Rare"}}]}}' WHERE id = 1;
           INSERT INTO ygo_set (name, code, tcg_date) VALUES
               ('Metal Raiders', 'MRD', '2002-06-26'), ('New Pack', 'NEW', '2016-01-29');
           INSERT INTO deck_format (id, name, version, cutoff) VALUES (1, 'Edison', 1, '2010-03-01');
           INSERT INTO yugipedia_errata (card_data_id, versions, checked_at) VALUES (1,
               '[{{"text":"Old text.","code":"MRD-069","set":"Metal Raiders","reprint_in":null}},
                 {{"text":"New text.","code":"NEW-EN001","set":"New Pack","reprint_in":null}}]',
               date('now'));
           DELETE FROM unite WHERE card_id = {first};
           DELETE FROM card WHERE id = {first};"#,
        first = cards[0]
    ))
    .await;
    sqlx::query("UPDATE card SET code = ? WHERE id = ?")
        .bind(copy_code)
        .bind(cards[1])
        .execute(&t.state.pool)
        .await
        .unwrap();
    t.set_main(v1, &[DM2]).await;
    t.exec(&format!(
        "UPDATE deck_version SET format_id = 1 WHERE id = {v1}"
    ))
    .await;
    set_built(t, v1, true).await;
    v1
}

#[tokio::test]
async fn the_built_page_warns_about_errata_after_the_formats_cutoff() {
    let t = TestApp::new().await;
    errata_fixture(&t, "NEW-FR001").await;

    let (status, page) = t.get("/decks/shopping").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    // the missing one and the owned copy with the new text, both to buy
    assert!(page.contains("errata'd in New Pack"), "{page}");
    assert!(page.contains("buy Metal Raiders (MRD-069)"), "{page}");
    assert!(
        !page.contains("buy Metal Raiders (MRD-069), New Pack"),
        "{page}"
    );
    assert!(page.contains("Old text."), "{page}");
    assert!(page.contains("New text."), "{page}");
    assert!(page.contains("Wrong text: 1"), "{page}");
    assert!(page.contains("with an errata"), "{page}");
    assert!(page.contains(">2 Dark Magician</textarea>"), "{page}");
    assert!(
        page.contains("owned with the wrong text: NEW-FR001"),
        "{page}"
    );
    assert!(!page.contains("being looked up"), "{page}");
}

#[tokio::test]
async fn an_owned_copy_printed_with_the_right_text_is_not_missing() {
    let t = TestApp::new().await;
    errata_fixture(&t, "MRD-FR069").await;

    let (_, page) = t.get("/decks/shopping").await;
    assert!(page.contains("errata'd in New Pack"), "{page}");
    assert!(!page.contains("Wrong text:"), "{page}");
    assert!(!page.contains("owned with the wrong text"), "{page}");
    assert!(page.contains(">1 Dark Magician</textarea>"), "{page}");

    // a copy whose set has no date is not judged either
    t.exec("UPDATE card SET code = 'ZZZ-FR001'").await;
    let (_, page) = t.get("/decks/shopping").await;
    assert!(!page.contains("Wrong text:"), "{page}");
}

#[tokio::test]
async fn no_errata_warning_for_a_later_cutoff_or_no_cutoff() {
    let t = TestApp::new().await;
    let v1 = errata_fixture(&t, "NEW-FR001").await;

    // the format's cutoff is after the errata
    t.exec("UPDATE deck_format SET cutoff = '2020-01-01'").await;
    let (_, page) = t.get("/decks/shopping").await;
    assert!(!page.contains('⚠'), "{page}");
    assert!(!page.contains("Wrong text:"), "{page}");
    assert!(page.contains(">1 Dark Magician</textarea>"), "{page}");

    // neither a format nor a ban list: no cutoff
    t.exec(&format!(
        "UPDATE deck_version SET format_id = NULL WHERE id = {v1}"
    ))
    .await;
    let (_, page) = t.get("/decks/shopping").await;
    assert!(!page.contains('⚠'), "{page}");
    assert!(!page.contains("Wrong text:"), "{page}");
}

#[tokio::test]
async fn a_ban_list_version_is_judged_at_the_ban_lists_date() {
    let t = TestApp::new().await;
    let v1 = errata_fixture(&t, "NEW-FR001").await;
    t.exec("INSERT INTO advanced_ban_list (id, date) VALUES (1, '2010-03-01')")
        .await;
    t.exec(&format!(
        "UPDATE deck_version SET format_id = NULL, ban_list_id = 1 WHERE id = {v1}"
    ))
    .await;
    let (_, page) = t.get("/decks/shopping").await;
    assert!(
        page.contains("ban list 01/03/2010: errata'd in New Pack"),
        "{page}"
    );
    assert!(page.contains("Wrong text: 1"), "{page}");

    // a list from after the errata: nothing to warn
    t.exec("UPDATE advanced_ban_list SET date = '2017-01-01'")
        .await;
    let (_, page) = t.get("/decks/shopping").await;
    assert!(!page.contains('⚠'), "{page}");
}

#[tokio::test]
async fn the_deck_builders_check_lists_the_errata() {
    let t = TestApp::new().await;
    let v1 = errata_fixture(&t, "NEW-FR001").await;

    let (status, check) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{v1}/check"),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{check}");
    assert_eq!(check["errata_pending"], 0);
    let errata = check["errata"].as_array().unwrap();
    assert_eq!(errata.len(), 1, "{check}");
    assert_eq!(errata[0]["count"], 2);
    assert_eq!(errata[0]["wrong_text"], json!(["NEW-FR001"]));
    assert_eq!(errata[0]["warning"]["changed_in"], "New Pack");
    assert_eq!(errata[0]["warning"]["new_text"], "New text.");

    // no format nor ban list: none
    t.exec(&format!(
        "UPDATE deck_version SET format_id = NULL WHERE id = {v1}"
    ))
    .await;
    let (_, check) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{v1}/check"),
            json!(null),
        )
        .await;
    assert_eq!(check["errata"], json!([]));
}

#[tokio::test]
async fn the_built_page_says_when_errata_are_still_being_looked_up() {
    let t = TestApp::new().await;
    errata_fixture(&t, "NEW-FR001").await;
    t.exec("DELETE FROM yugipedia_errata").await;

    let (status, page) = t.get("/decks/shopping").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains("Errata are being looked up on Yugipedia for 1 card(s)"),
        "{page}"
    );
    assert!(!page.contains('⚠'), "{page}");
    // meanwhile the missing one is missing as before
    assert!(page.contains(">1 Dark Magician</textarea>"), "{page}");
}
