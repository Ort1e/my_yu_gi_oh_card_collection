//! Built decks: building a version locks its real cards (`yugioh_domain::built_deck`,
//! `decks/api/version.rs`), and the collection reads skip them.

use serde_json::{json, Value};
use yugioh_domain::ydke::YdkeDeck;

use super::*;
use crate::store::cards::replace_proxies;

/// A new version of the fixture's deck with these main deck cards.
async fn version(t: &TestApp, name: &str, cards: &[i64]) -> i64 {
    let id = sqlx::query("INSERT INTO deck_version (deck_id, version_name) VALUES (1, ?)")
        .bind(name)
        .execute(&t.state.pool)
        .await
        .unwrap()
        .last_insert_rowid();
    put(t, id, cards).await;
    id
}

/// Put cards in a version's main deck, as a stale page could have saved them.
async fn put(t: &TestApp, version: i64, cards: &[i64]) {
    for card in cards {
        sqlx::query("INSERT INTO deck_version_main_deck (deck_version_id, card_id) VALUES (?, ?)")
            .bind(version)
            .bind(card)
            .execute(&t.state.pool)
            .await
            .unwrap();
    }
}

async fn main_deck(t: &TestApp, version: i64) -> Vec<(i64, bool, Option<i64>)> {
    sqlx::query_as(
        "SELECT card.id, card.is_proxy, card.card_data_id FROM deck_version_main_deck z
         JOIN card ON card.id = z.card_id WHERE z.deck_version_id = ? ORDER BY z.rowid",
    )
    .bind(version)
    .fetch_all(&t.state.pool)
    .await
    .unwrap()
}

async fn set_built(t: &TestApp, version: i64, built: bool) -> Value {
    let (status, body) = t
        .json(
            "PATCH",
            &format!("/api/deck-versions/{version}"),
            json!({ "is_built": built }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn reserve_ids(t: &TestApp, version: i64) -> Vec<i64> {
    let (status, body) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{version}/reserve"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn building_turns_the_shared_cards_into_proxies_elsewhere() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    let (dm1, dm2, kuriboh) = (cards[0], cards[1], cards[2]);
    put(&t, v1, &[dm1, kuriboh]).await;
    let v2 = version(&t, "v2", &[dm1, dm2]).await;
    // a category of v2 on the shared card
    sqlx::query(
        "INSERT INTO card_category (id, name, deck_version_id) VALUES (50, 'Starter', ?);
         INSERT INTO card_category_assignment (category_id, card_id) VALUES (50, ?);",
    )
    .bind(v2)
    .bind(dm1)
    .execute(&t.state.pool)
    .await
    .unwrap();

    let built = set_built(&t, v1, true).await;
    assert_eq!(built["is_built"], true);
    assert_eq!(built["build_report"]["proxied_here"], json!([]));
    assert_eq!(
        built["build_report"]["proxied_elsewhere"],
        json!(["Magicien Sombre → Test - v2"])
    );

    // v1 keeps its card; v2 has a proxy of it (same card data), dm2 stays
    assert_eq!(main_deck(&t, v1).await[0], (dm1, false, Some(1)));
    let v2_cards = main_deck(&t, v2).await;
    let (proxy, is_proxy, data) = v2_cards[0];
    assert!(is_proxy && data == Some(1) && proxy != dm1);
    assert_eq!(v2_cards[1], (dm2, false, Some(1)));
    assert_eq!(
        t.text(&format!("SELECT name FROM card WHERE id = {proxy}"))
            .await
            .as_deref(),
        Some("Magicien Sombre")
    );
    // the category moved to the proxy
    assert_eq!(
        t.scalar("SELECT card_id FROM card_category_assignment WHERE category_id = 50")
            .await,
        proxy
    );

    // the answer without building has no report
    let (_, again) = t
        .json("GET", &format!("/api/deck-versions/{v1}"), json!(null))
        .await;
    assert!(again.get("build_report").is_none(), "{again}");
}

#[tokio::test]
async fn the_first_built_deck_keeps_its_cards() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0]]).await;
    set_built(&t, v1, true).await;

    // a version saved by a stale page with the locked card
    let v2 = version(&t, "v2", &[cards[0], cards[1]]).await;
    let built = set_built(&t, v2, true).await;
    assert_eq!(
        built["build_report"]["proxied_here"],
        json!(["Magicien Sombre (in Test - v1)"])
    );

    assert_eq!(main_deck(&t, v1).await, vec![(cards[0], false, Some(1))]);
    let v2_cards = main_deck(&t, v2).await;
    assert!(v2_cards[0].1, "a proxy in the second built version");
    assert_eq!(v2_cards[1], (cards[1], false, Some(1)));
}

#[tokio::test]
async fn two_versions_of_a_deck_can_both_be_built() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0]]).await;
    let v2 = version(&t, "v2", &[cards[1]]).await;

    set_built(&t, v1, true).await;
    set_built(&t, v2, true).await;
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version WHERE is_built = 1")
            .await,
        2
    );
    // and unbuilt: only the flag changes
    let unbuilt = set_built(&t, v2, false).await;
    assert_eq!(unbuilt["is_built"], false);
    assert_eq!(main_deck(&t, v2).await, vec![(cards[1], false, Some(1))]);
}

#[tokio::test]
async fn locked_cards_leave_the_reserve_until_unbuilt() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0]]).await;
    let v2 = version(&t, "v2", &[]).await;
    assert_eq!(reserve_ids(&t, v2).await, vec![cards[0], cards[1]]);

    set_built(&t, v1, true).await;
    assert_eq!(reserve_ids(&t, v2).await, vec![cards[1]]);
    // the built version's own reserve is unchanged
    assert_eq!(reserve_ids(&t, v1).await, vec![cards[1]]);

    set_built(&t, v1, false).await;
    assert_eq!(reserve_ids(&t, v2).await, vec![cards[0], cards[1]]);
}

#[tokio::test]
async fn saving_a_card_locked_elsewhere_is_refused() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0]]).await;
    set_built(&t, v1, true).await;
    let v2 = version(&t, "v2", &[]).await;

    let (status, body) = t
        .json(
            "PATCH",
            &format!("/api/deck-versions/{v2}"),
            json!({ "main": [cards[0], cards[1]] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "Magicien Sombre is already in a built deck (Test - v1), so it is no longer in \
         your collection. Reload the page: it will be gone from the reserve; add proxies \
         instead if this deck needs it."
    );
    assert!(main_deck(&t, v2).await.is_empty(), "nothing saved");

    // the built version itself saves its own cards
    let (status, body) = t
        .json(
            "PATCH",
            &format!("/api/deck-versions/{v1}"),
            json!({ "main": [cards[0], cards[1]] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn deck_box_and_sleeves() {
    let t = TestApp::new().await;
    let (v1, _) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v1}");

    let (_, body) = t
        .json(
            "PATCH",
            &uri,
            json!({ "deck_box": " red Boulder 100+ ", "sleeves": "Dragon Shield matte black" }),
        )
        .await;
    assert_eq!(body["deck_box"], "red Boulder 100+");
    assert_eq!(body["sleeves"], "Dragon Shield matte black");

    // absent: kept; blank or null: cleared
    let (_, body) = t.json("PATCH", &uri, json!({ "deck_box": "" })).await;
    assert_eq!(body["deck_box"], Value::Null);
    assert_eq!(body["sleeves"], "Dragon Shield matte black");
    let (_, body) = t.json("PATCH", &uri, json!({ "sleeves": null })).await;
    assert_eq!(body["sleeves"], Value::Null);
}

#[tokio::test]
async fn ydke_import_skips_locked_copies() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0]]).await;
    set_built(&t, v1, true).await;

    let ydke = YdkeDeck {
        main: vec![46986414, 46986414],
        extra: vec![],
        side: vec![],
    }
    .to_url();
    let (status, body) = t
        .json(
            "POST",
            "/api/decks/1/import-ydke",
            json!({ "ydke": ydke, "name": "imported" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let imported = main_deck(&t, body["id"].as_i64().unwrap()).await;
    // the free copy, then a proxy (the other copy is locked)
    assert_eq!(imported[0], (cards[1], false, Some(1)));
    assert!(imported[1].1);
}

#[tokio::test]
async fn clean_cards_never_takes_a_locked_card() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0], cards[1]]).await;
    set_built(&t, v1, true).await;
    let proxy = crate::store::cards::new_proxy(
        &mut t.state.pool.acquire().await.unwrap(),
        "Dark Magician",
        Some(1),
    )
    .await
    .unwrap();
    let v2 = version(&t, "v2", &[proxy]).await;

    let mut conn = t.state.pool.acquire().await.unwrap();
    assert_eq!(replace_proxies(&mut conn).await.unwrap(), 0);
    drop(conn);

    // unbuilt: the owned card goes back to v2
    set_built(&t, v1, false).await;
    let mut conn = t.state.pool.acquire().await.unwrap();
    assert_eq!(replace_proxies(&mut conn).await.unwrap(), 1);
    drop(conn);
    assert!(!main_deck(&t, v2).await[0].1);
}

#[tokio::test]
async fn a_copy_of_a_built_version_gets_proxies() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0], cards[1]]).await;
    set_built(&t, v1, true).await;

    let (status, body) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{v1}/clone"),
            json!({ "name": "copy" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let copy = body["id"].as_i64().unwrap();

    let copied = main_deck(&t, copy).await;
    assert_eq!(copied.len(), 2);
    assert!(copied
        .iter()
        .all(|(id, is_proxy, data)| *is_proxy && *data == Some(1) && !cards.contains(id)));
    let (_, body) = t
        .json("GET", &format!("/api/deck-versions/{copy}"), json!(null))
        .await;
    assert_eq!(body["is_built"], false);
}

#[tokio::test]
async fn the_built_page_lists_what_is_missing() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0], cards[2]]).await;
    sqlx::query("UPDATE deck_version SET deck_box = 'red box' WHERE id = ?")
        .bind(v1)
        .execute(&t.state.pool)
        .await
        .unwrap();
    set_built(&t, v1, true).await;

    let (status, page) = t.get("/decks/built").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Test - v1"), "{page}");
    assert!(page.contains("Deck box: red box"), "{page}");
    assert!(page.contains("1 × Kuriboh"), "{page}");
    assert!(
        page.contains("<td>Kuriboh</td><td>1</td><td class=\"text\">Test - v1</td>"),
        "{page}"
    );

    // the badge on the deck page
    let (_, deck) = t.get("/decks/1").await;
    assert!(deck.contains("built-badge"), "{deck}");
}

#[tokio::test]
async fn the_built_page_gives_a_cardmarket_wants_list_by_english_name() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    // the card data's English name has its capitals (`en_name` is lowercase)
    t.exec(
        "UPDATE card_data SET json_data = '{\"name\":\"Kuriboh\"}' WHERE id = 2;
         UPDATE card_data SET json_data = '{\"name\":\"Dark Magician\"}' WHERE id = 1;
         UPDATE card SET is_proxy = 1 WHERE id = 1",
    )
    .await;
    // a French proxy, twice, plus the Kuriboh proxy
    put(&t, v1, &[cards[0], cards[2]]).await;
    set_built(&t, v1, true).await;

    let (status, page) = t.get("/decks/built").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    // only the whole list (sorted like the table, by the name on the card):
    // each version's own list is in the deck builder's To buy tab
    assert!(
        !page.contains(">1 Dark Magician\n1 Kuriboh</textarea>"),
        "{page}"
    );
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
async fn any_version_page_gives_its_wants_list() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    t.exec("UPDATE card_data SET json_data = '{\"name\":\"Kuriboh\"}' WHERE id = 2")
        .await;

    // not built, one proxy and one real card: the builder's Missing tab
    // gets the list from the version's check
    put(&t, v1, &[cards[0], cards[2]]).await;
    let (status, check) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{v1}/check"),
            json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{check}");
    assert_eq!(check["wants"], "1 Kuriboh");

    // the read-only page has no wants list any more
    let (_, page) = t.get(&format!("/deck-versions/{v1}")).await;
    assert!(!page.contains("wants-version"), "{page}");

    // no proxy: no wants list
    t.exec("DELETE FROM deck_version_main_deck WHERE card_id = 3")
        .await;
    let (_, check) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{v1}/check"),
            json!(null),
        )
        .await;
    assert_eq!(check["wants"], "");
}

async fn dm_proxy(t: &TestApp) -> i64 {
    crate::store::cards::new_proxy(
        &mut t.state.pool.acquire().await.unwrap(),
        "Dark Magician",
        Some(1),
    )
    .await
    .unwrap()
}

async fn replace(t: &TestApp, version: i64, card: i64) -> (StatusCode, Value) {
    t.json(
        "POST",
        &format!("/api/deck-versions/{version}/proxies/{card}/replace"),
        json!(null),
    )
    .await
}

#[tokio::test]
async fn built_versions_keep_their_proxies_until_swapped_by_hand() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    let (built_proxy, other_proxy) = (dm_proxy(&t).await, dm_proxy(&t).await);
    put(&t, v1, &[built_proxy]).await;
    set_built(&t, v1, true).await;
    let v2 = version(&t, "v2", &[other_proxy]).await;

    // the automatic swap (new lot, clean-cards) skips the built version
    let mut conn = t.state.pool.acquire().await.unwrap();
    assert_eq!(replace_proxies(&mut conn).await.unwrap(), 1);
    drop(conn);
    assert_eq!(main_deck(&t, v1).await[0], (built_proxy, true, Some(1)));
    assert_eq!(main_deck(&t, v2).await[0], (cards[0], false, Some(1)));

    // unbuilt, it is swapped like any other version (versions that are not
    // built may share a copy)
    set_built(&t, v1, false).await;
    let mut conn = t.state.pool.acquire().await.unwrap();
    assert_eq!(replace_proxies(&mut conn).await.unwrap(), 1);
    drop(conn);
    assert_eq!(main_deck(&t, v1).await[0], (cards[0], false, Some(1)));
}

#[tokio::test]
async fn replacing_a_proxy_by_hand_in_a_built_version() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    let (dm1, dm2) = (cards[0], cards[1]);
    let (first, second) = (dm_proxy(&t).await, dm_proxy(&t).await);
    put(&t, v1, &[first, second]).await;
    set_built(&t, v1, true).await;
    let v2 = version(&t, "v2", &[dm1]).await;
    // a category of v1 on the first proxy
    sqlx::query(
        "INSERT INTO card_category (id, name, deck_version_id) VALUES (60, 'Boss', ?);
         INSERT INTO card_category_assignment (category_id, card_id) VALUES (60, ?);",
    )
    .bind(v1)
    .bind(first)
    .execute(&t.state.pool)
    .await
    .unwrap();

    // the loose copy first, with the proxy's category; the proxy is deleted
    let (status, body) = replace(&t, v1, first).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["card"]["id"], dm2);
    assert_eq!(body["proxied_in"], json!([]));
    assert_eq!(
        t.scalar("SELECT card_id FROM card_category_assignment WHERE category_id = 60")
            .await,
        dm2
    );
    assert_eq!(
        t.scalar(&format!("SELECT count(*) FROM card WHERE id = {first}"))
            .await,
        0
    );

    // then the copy of v2 (not built), which gets a proxy instead
    let (status, body) = replace(&t, v1, second).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["card"]["id"], dm1);
    assert_eq!(body["proxied_in"], json!(["Test - v2"]));
    assert_eq!(
        main_deck(&t, v1).await,
        vec![(dm2, false, Some(1)), (dm1, false, Some(1))]
    );
    let (proxy, is_proxy, data) = main_deck(&t, v2).await[0];
    assert!(is_proxy && data == Some(1) && proxy != dm1);
}

#[tokio::test]
async fn replacing_a_proxy_by_hand_needs_a_free_copy() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    put(&t, v1, &[cards[0], cards[1]]).await;
    set_built(&t, v1, true).await;
    let proxy = dm_proxy(&t).await;
    let v2 = version(&t, "v2", &[proxy, cards[2]]).await;

    // both copies are locked by v1
    let (status, body) = replace(&t, v2, proxy).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "No free copy of Dark Magician in your collection."
    );

    // not a proxy, or not in this version
    let (status, _) = replace(&t, v1, cards[0]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = replace(&t, v1, proxy).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = replace(&t, 999, proxy).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn saving_a_built_version_takes_its_new_cards_from_the_others() {
    let t = TestApp::new().await;
    let (v1, cards) = t.deck_fixture().await;
    let (dm1, dm2) = (cards[0], cards[1]);
    put(&t, v1, &[dm1]).await;
    set_built(&t, v1, true).await;
    let v2 = version(&t, "v2", &[dm2]).await;

    // dm2, used by v2, added to the built v1 from the reserve
    let (status, body) = t
        .json(
            "PATCH",
            &format!("/api/deck-versions/{v1}"),
            json!({ "main": [dm1, dm2] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["build_report"]["proxied_elsewhere"],
        json!(["Magicien Sombre → Test - v2"])
    );
    assert_eq!(body["is_built"], true);
    let (proxy, is_proxy, data) = main_deck(&t, v2).await[0];
    assert!(is_proxy && data == Some(1) && proxy != dm2);

    // saved again with nothing new: nothing taken, no report
    let (status, body) = t
        .json(
            "PATCH",
            &format!("/api/deck-versions/{v1}"),
            json!({ "main": [dm1, dm2] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.get("build_report").is_none(), "{body}");

    // a version that is not built still shares its cards
    let v3 = version(&t, "v3", &[]).await;
    let (status, body) = t
        .json(
            "PATCH",
            &format!("/api/deck-versions/{v3}"),
            json!({ "main": [proxy] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.get("build_report").is_none());
}

/// Dark Magician plays the part of a card errata'd in 2016 ("New Pack"),
/// printed before in "Metal Raiders"; the fixture's version plays the format
/// "Edison" (cutoff 2010-03-01) with one proxy and one real copy coded
/// `copy_code`. Nothing is asked to Yugipedia: its errata are cached.
async fn errata_fixture(t: &TestApp, copy_code: &str) -> i64 {
    let (v1, cards) = t.deck_fixture().await;
    t.exec(
        r#"UPDATE card_data SET json_data = '{"name":"Dark Magician","card_sets":[
               {"set_name":"Metal Raiders","set_code":"MRD-069","set_rarity":"Common"},
               {"set_name":"New Pack","set_code":"NEW-EN001","set_rarity":"Rare"}]}' WHERE id = 1;
           INSERT INTO ygo_set (name, code, tcg_date) VALUES
               ('Metal Raiders', 'MRD', '2002-06-26'), ('New Pack', 'NEW', '2016-01-29');
           INSERT INTO deck_format (id, name, version, cutoff) VALUES (1, 'Edison', 1, '2010-03-01');
           INSERT INTO yugipedia_errata (card_data_id, versions, checked_at) VALUES (1,
               '[{"text":"Old text.","code":"MRD-069","set":"Metal Raiders","reprint_in":null},
                 {"text":"New text.","code":"NEW-EN001","set":"New Pack","reprint_in":null}]',
               date('now'));
           UPDATE card SET is_proxy = 1 WHERE id = 1"#,
    )
    .await;
    sqlx::query("UPDATE card SET code = ? WHERE id = 2")
        .bind(copy_code)
        .execute(&t.state.pool)
        .await
        .unwrap();
    put(t, v1, &[cards[0], cards[1]]).await;
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

    let (status, page) = t.get("/decks/built").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    // the proxy and the owned copy with the new text, both to buy
    assert!(page.contains("errata'd in New Pack"), "{page}");
    assert!(page.contains("buy Metal Raiders (MRD-069)"), "{page}");
    assert!(
        !page.contains("buy Metal Raiders (MRD-069), New Pack"),
        "{page}"
    );
    assert!(page.contains("Old text."), "{page}");
    assert!(page.contains("New text."), "{page}");
    assert!(page.contains("Wrong text: 1"), "{page}");
    assert!(
        page.contains("owned with the wrong text: NEW-FR001"),
        "{page}"
    );
    assert!(page.contains(">2 Dark Magician</textarea>"), "{page}");
    assert!(
        page.contains("1 replace owned copies with the wrong text"),
        "{page}"
    );
    assert!(!page.contains("being looked up"), "{page}");
}

#[tokio::test]
async fn an_owned_copy_printed_with_the_right_text_is_not_missing() {
    let t = TestApp::new().await;
    errata_fixture(&t, "MRD-FR069").await;

    let (_, page) = t.get("/decks/built").await;
    assert!(page.contains("errata'd in New Pack"), "{page}");
    assert!(!page.contains("Wrong text:"), "{page}");
    assert!(!page.contains("owned with the wrong text"), "{page}");
    assert!(page.contains(">1 Dark Magician</textarea>"), "{page}");

    // a copy whose set has no date is not judged either
    t.exec("UPDATE card SET code = 'ZZZ-FR001' WHERE id = 2")
        .await;
    let (_, page) = t.get("/decks/built").await;
    assert!(!page.contains("Wrong text:"), "{page}");
}

#[tokio::test]
async fn no_errata_warning_for_a_later_cutoff_or_no_cutoff() {
    let t = TestApp::new().await;
    let v1 = errata_fixture(&t, "NEW-FR001").await;

    // the format's cutoff is after the errata
    t.exec("UPDATE deck_format SET cutoff = '2020-01-01'").await;
    let (_, page) = t.get("/decks/built").await;
    assert!(!page.contains('⚠'), "{page}");
    assert!(!page.contains("Wrong text:"), "{page}");
    assert!(page.contains(">1 Dark Magician</textarea>"), "{page}");

    // neither a format nor a ban list: no cutoff
    t.exec(&format!(
        "UPDATE deck_version SET format_id = NULL WHERE id = {v1}"
    ))
    .await;
    let (_, page) = t.get("/decks/built").await;
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
    let (_, page) = t.get("/decks/built").await;
    assert!(
        page.contains("ban list 01/03/2010: errata'd in New Pack"),
        "{page}"
    );
    assert!(page.contains("Wrong text: 1"), "{page}");

    // a list from after the errata: nothing to warn
    t.exec("UPDATE advanced_ban_list SET date = '2017-01-01'")
        .await;
    let (_, page) = t.get("/decks/built").await;
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

    let (status, page) = t.get("/decks/built").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains("Errata are being looked up on Yugipedia for 1 card(s)"),
        "{page}"
    );
    assert!(!page.contains('⚠'), "{page}");
    // meanwhile the proxy is missing as before
    assert!(page.contains(">1 Dark Magician</textarea>"), "{page}");
}
