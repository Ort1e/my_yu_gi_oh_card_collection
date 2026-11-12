//! The deck builder JSON API: versions as lists (`store::lists`).

use super::*;
use crate::testing::{tiles, DM, KURIBOH};
use serde_json::json;

/// `(missing, card data)` of each tile of a zone.
fn zone(deck: &serde_json::Value, zone: &str) -> Vec<(bool, i64)> {
    deck[zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["missing"].as_bool().unwrap(),
                c["card_data_id"].as_i64().unwrap_or(0),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_version_is_a_list_and_the_collection_says_what_is_owned() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}");

    let (status, deck) = t.json("GET", &uri, json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deck["main"], json!([]));
    assert_eq!(deck["versions"].as_array().unwrap().len(), 1);
    assert_eq!(deck["ydke"]["all"], "ydke://!!!");
    assert_eq!(deck["budget_left"], json!(null), "no budget set yet");
    assert_eq!(deck["is_planned"], false);

    // a budget for this month: what is left of it shows in the header
    let month = crate::store::db::today().format("%Y-%m-01").to_string();
    t.exec(&format!(
        "INSERT INTO monthly_budget (month, amount) VALUES ('{month}', '30.00')"
    ))
    .await;
    let (_, deck) = t.json("GET", &uri, json!(null)).await;
    assert!(deck["budget_left"].is_string(), "{deck}");

    // the search: a card owned twice, a card known but not owned
    let (_, found) = t
        .json(
            "POST",
            &format!("{uri}/reserve"),
            json!({ "name": "magician" }),
        )
        .await;
    assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    assert_eq!(found[0]["card_data_id"], 1);
    assert_eq!(found[0]["name"], "Magicien Sombre");
    assert_eq!(
        (found[0]["owned"].clone(), found[0]["free"].clone()),
        (json!(2), json!(2))
    );
    assert_eq!(found[0]["missing"], false);
    let (_, found) = t
        .json(
            "POST",
            &format!("{uri}/reserve"),
            json!({ "name": "kuriboh" }),
        )
        .await;
    assert_eq!(found[0]["card_data_id"], 2, "{found}");
    assert_eq!(found[0]["missing"], true);
    let (_, found) = t
        .json(
            "POST",
            &format!("{uri}/reserve"),
            json!({ "code": "LOB-FR" }),
        )
        .await;
    assert_eq!(
        found.as_array().unwrap().len(),
        1,
        "by code: the owned ones"
    );

    // 3 Dark Magicians (2 owned) and a Kuriboh
    let deck = t
        .patch(
            v,
            json!({
                "main": tiles(&[(DM, "Magicien Sombre", 3)]),
                "side": tiles(&[(KURIBOH, "Kuriboh", 1)]),
                "ban_list_id": null,
            }),
        )
        .await;
    assert_eq!(zone(&deck, "main"), vec![(false, 1), (false, 1), (true, 1)]);
    assert_eq!(zone(&deck, "side"), vec![(true, 2)]);
    assert_eq!(deck["main"][0]["price"], "2.25");
    assert_eq!(deck["main"][2]["price"], json!(null));
    assert_eq!(
        deck["main"][0]["copy"],
        json!(null),
        "a plain list holds no copy"
    );
    assert!(deck["main"][0]["sample_copy"].is_i64());
    let owned =
        yugioh_domain::ydke::YdkeDeck::parse(deck["ydke"]["owned"].as_str().unwrap()).unwrap();
    assert_eq!(owned.main, vec![46986414, 46986414]);
    assert!(owned.side.is_empty());
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_version_card WHERE deck_version_id = {v}"
        ))
        .await,
        2,
        "one line per card"
    );
    assert_eq!(t.scalar("SELECT count(*) FROM deck_version_copy").await, 0);

    let (status, error) = t
        .json(
            "PATCH",
            &uri,
            json!({ "main": [{ "card_data_id": 999999, "name": "x" }] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("don't exist"));

    let deck = t.patch(v, json!({ "version_name": "v2" })).await;
    assert_eq!(deck["version_name"], "v2");
    assert_eq!(deck["main"].as_array().unwrap().len(), 3, "zones untouched");

    // only the Main Deck sent: the others keep their lines
    let deck = t.set_main(v, &[(DM, "Magicien Sombre", 1)]).await;
    assert_eq!(zone(&deck, "side"), vec![(true, 2)]);
}

#[tokio::test]
async fn a_copy_bought_is_owned_at_once_in_every_list() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    let deck = t.set_main(v, &[(KURIBOH, "Kuriboh", 1)]).await;
    assert_eq!(zone(&deck, "main"), vec![(true, 2)]);

    t.exec("INSERT INTO card (name, en_name, card_data_id) VALUES ('Kuriboh', 'kuriboh', 2)")
        .await;
    let (_, deck) = t
        .json("GET", &format!("/api/deck-versions/{v}"), json!(null))
        .await;
    assert_eq!(zone(&deck, "main"), vec![(false, 2)]);

    // two plain lists may count on the same copy
    let (_, other) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{v}/clone"),
            json!({ "name": "v2" }),
        )
        .await;
    let (_, deck) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{}", other["id"]),
            json!(null),
        )
        .await;
    assert_eq!(zone(&deck, "main"), vec![(false, 2)]);
}

#[tokio::test]
async fn categories_and_monte_carlo() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}");
    t.set_main(v, &[(DM, "Magicien Sombre", 2), (KURIBOH, "Kuriboh", 1)])
        .await;

    let (status, cat) = t
        .json(
            "POST",
            &format!("{uri}/categories"),
            json!({ "name": "starter" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let cat_id = cat["id"].as_i64().unwrap();
    // a category is given to a card (its card data): all its copies
    let (status, _) = t
        .json(
            "PUT",
            &format!("{uri}/cards/1/categories/{cat_id}"),
            json!({ "assigned": true }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = t
        .json("GET", &format!("{uri}/cards/1/categories"), json!(null))
        .await;
    assert_eq!(
        list,
        json!([{ "id": cat_id, "name": "starter", "assigned": true }])
    );
    let (_, list) = t
        .json("GET", &format!("{uri}/cards/2/categories"), json!(null))
        .await;
    assert_eq!(list[0]["assigned"], false);

    let (status, result) = t
        .json(
            "POST",
            &format!("{uri}/monte-carlo"),
            json!({ "hand_size": 5, "simulations": 100 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["categories"][0]["cards_in_deck"], 2, "two copies");
    assert_eq!(
        result["categories"][0]["occurrences"], 200,
        "the whole 3-card deck is drawn"
    );
    let (status, _) = t
        .json(
            "POST",
            &format!("{uri}/monte-carlo"),
            json!({ "hand_size": 0, "simulations": 100 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // on the zones as they are on the screen, saved or not: one copy, and
    // a card known by its name only
    let (status, result) = t
        .json(
            "POST",
            &format!("{uri}/monte-carlo"),
            json!({ "hand_size": 5, "simulations": 100, "main": [1, null] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["categories"][0]["cards_in_deck"], 1);
    let (status, _) = t
        .json(
            "POST",
            &format!("{uri}/monte-carlo"),
            json!({ "hand_size": 5, "simulations": 100, "main": [999_999] }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // a card taken out of the deck loses its assignments
    t.set_main(v, &[(KURIBOH, "Kuriboh", 1)]).await;
    assert_eq!(
        t.scalar("SELECT count(*) FROM card_category_assignment")
            .await,
        0
    );
    t.set_main(v, &[(DM, "Magicien Sombre", 1)]).await;
    t.json(
        "PUT",
        &format!("{uri}/cards/1/categories/{cat_id}"),
        json!({ "assigned": true }),
    )
    .await;
    let (status, _) = t
        .json("DELETE", &format!("{uri}/categories/{cat_id}"), json!(null))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        t.scalar("SELECT count(*) FROM card_category_assignment")
            .await,
        0
    );
}

#[tokio::test]
async fn conditional_categories() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}/categories");
    let mut ids = Vec::new();
    for name in ["starter", "extender", "brick"] {
        let (_, cat) = t.json("POST", &uri, json!({ "name": name })).await;
        ids.push(cat["id"].as_i64().unwrap());
    }
    let [starter, extender, brick] = ids[..] else {
        unreachable!()
    };

    let rule = |if_any_of: Vec<i64>, then: i64| {
        json!({
            "name": "starter with an extender",
            "condition": {
                "description": "starts only with an extender",
                "if_any_of": if_any_of,
                "then": then,
                "otherwise": brick,
            },
        })
    };
    let (status, cat) = t.json("POST", &uri, rule(vec![extender], starter)).await;
    assert_eq!(status, StatusCode::CREATED, "{cat}");
    let (_, list) = t.json("GET", &uri, json!(null)).await;
    assert_eq!(list[3]["condition"]["if_any_of"], json!([extender]));
    assert_eq!(list[3]["condition"]["then"], starter);
    assert_eq!(list[3]["condition"]["otherwise"], brick);
    assert_eq!(list[0]["condition"], json!(null));

    // invalid rules
    let (status, _) = t.json("POST", &uri, rule(vec![], starter)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = t.json("POST", &uri, rule(vec![extender], 12345)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // deleting the `then` category deletes the conditional one
    t.json("DELETE", &format!("{uri}/{starter}"), json!(null))
        .await;
    let (_, list) = t.json("GET", &uri, json!(null)).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn missing_cards_clone_and_delete() {
    let t = TestApp::new().await;
    let (v, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}");
    t.set_main(v, &[(DM, "Magicien Sombre", 1)]).await;

    // "+ Missing card": a tile of a card known by its name, nothing saved
    let (status, tile) = t
        .json(
            "POST",
            &format!("{uri}/lookup"),
            json!({ "name": "Kuriboh" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{tile}");
    assert_eq!(tile["card_data_id"], 2);
    assert_eq!(tile["missing"], true);
    let (status, _) = t
        .json("POST", &format!("{uri}/lookup"), json!({ "name": " " }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_version_card WHERE deck_version_id = {v}"
        ))
        .await,
        1
    );

    // a name-only card (not in ygoprodeck) is never owned
    let deck = t
        .patch(
            v,
            json!({ "extra": [{ "card_data_id": null, "name": "Some Fan Card" }] }),
        )
        .await;
    assert_eq!(zone(&deck, "extra"), vec![(true, 0)]);
    assert_eq!(deck["extra"][0]["name"], "Some Fan Card");

    let (status, clone) = t
        .json("POST", &format!("{uri}/clone"), json!({ "name": "v2" }))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, cloned) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{}", clone["id"]),
            json!(null),
        )
        .await;
    assert_eq!(zone(&cloned, "main"), vec![(false, 1)]);
    assert_eq!(cloned["extra"][0]["name"], "Some Fan Card");
    assert_eq!(cloned["versions"].as_array().unwrap().len(), 2);

    let (status, deleted) = t.json("DELETE", &uri, json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(deleted["deck_id"].is_i64());
    assert_eq!(
        t.json("GET", &uri, json!(null)).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM card WHERE id = {}",
            cards[0]
        ))
        .await,
        1,
        "the cards stay in the collection"
    );
}

#[tokio::test]
async fn ydke_import() {
    let t = TestApp::new().await;
    t.deck_fixture().await;

    let ydke = yugioh_domain::ydke::YdkeDeck {
        main: vec![46986414, 46986414, 46986414],
        extra: vec![],
        side: vec![40640057],
    }
    .to_url();
    let deck_id = t.scalar("SELECT id FROM deck").await;
    let (status, created) = t
        .json(
            "POST",
            &format!("/api/decks/{deck_id}/import-ydke"),
            json!({ "ydke": ydke, "name": "imported" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (_, deck) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{}", created["id"]),
            json!(null),
        )
        .await;
    assert_eq!(
        zone(&deck, "main"),
        vec![(false, 1), (false, 1), (true, 1)],
        "the 2 owned copies count"
    );
    assert_eq!(zone(&deck, "side"), vec![(true, 2)]);
    assert_eq!(
        t.scalar(&format!(
            "SELECT count FROM deck_version_card WHERE deck_version_id = {} AND zone = 'main'",
            created["id"]
        ))
        .await,
        3,
        "one line of 3"
    );

    let (status, error) = t
        .json(
            "POST",
            &format!("/api/decks/{deck_id}/import-ydke"),
            json!({ "ydke": "nope" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "Invalid YDKE URL");
}
