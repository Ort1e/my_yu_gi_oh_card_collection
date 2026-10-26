//! The deck builder JSON API.

use super::*;
use serde_json::json;

#[tokio::test]
async fn deck_version_zones_and_reserve() {
    let t = TestApp::new().await;
    let (v, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}");

    let (status, deck) = t.json("GET", &uri, json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deck["main"], json!([]));
    assert_eq!(deck["versions"].as_array().unwrap().len(), 1);
    assert_eq!(deck["ydke"]["with_proxies"], "ydke://!!!");

    // reserve: owned, non-proxy cards only
    let (_, reserve) = t
        .json(
            "POST",
            &format!("{uri}/reserve"),
            json!({ "name": "magician" }),
        )
        .await;
    assert_eq!(reserve.as_array().unwrap().len(), 2);
    let (_, reserve) = t
        .json(
            "POST",
            &format!("{uri}/reserve"),
            json!({ "name": "kuriboh" }),
        )
        .await;
    assert_eq!(reserve, json!([]), "proxies are not in the reserve");

    let (status, deck) = t
        .json(
            "PATCH",
            &uri,
            json!({ "main": [cards[0]], "side": [cards[2]], "ban_list_id": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{deck}");
    assert_eq!(deck["main"][0]["id"], cards[0]);
    assert_eq!(deck["main"][0]["price"], "2.25");
    assert_eq!(deck["side"][0]["is_proxy"], true);
    let ydke =
        yugioh_domain::ydke::YdkeDeck::parse(deck["ydke"]["without_proxies"].as_str().unwrap()).unwrap();
    assert_eq!(ydke.main, vec![46986414]);
    assert!(ydke.side.is_empty());

    let (_, reserve) = t
        .json(
            "POST",
            &format!("{uri}/reserve"),
            json!({ "name": "magician" }),
        )
        .await;
    assert_eq!(
        reserve.as_array().unwrap().len(),
        1,
        "cards in the deck leave the reserve"
    );

    let (status, error) = t.json("PATCH", &uri, json!({ "main": [999999] })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"].as_str().unwrap().contains("don't exist"));

    let (_, deck) = t.json("PATCH", &uri, json!({ "version_name": "v2" })).await;
    assert_eq!(deck["version_name"], "v2");
    assert_eq!(deck["main"][0]["id"], cards[0], "zones untouched");
}

#[tokio::test]
async fn categories_and_monte_carlo() {
    let t = TestApp::new().await;
    let (v, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}");
    t.json("PATCH", &uri, json!({ "main": [cards[0], cards[1]] }))
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
    for card in &cards[..2] {
        let (status, _) = t
            .json(
                "PUT",
                &format!("{uri}/cards/{card}/categories/{cat_id}"),
                json!({ "assigned": true }),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (_, list) = t
        .json(
            "GET",
            &format!("{uri}/cards/{}/categories", cards[0]),
            json!(null),
        )
        .await;
    assert_eq!(
        list,
        json!([{ "id": cat_id, "name": "starter", "assigned": true }])
    );

    let (status, result) = t
        .json(
            "POST",
            &format!("{uri}/monte-carlo"),
            json!({ "hand_size": 5, "simulations": 100 }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["categories"][0]["cards_in_deck"], 2);
    assert_eq!(
        result["categories"][0]["occurrences"], 200,
        "the whole 2-card deck is drawn"
    );
    let (status, _) = t
        .json(
            "POST",
            &format!("{uri}/monte-carlo"),
            json!({ "hand_size": 0, "simulations": 100 }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // removing a card from the deck drops its assignments
    t.json("PATCH", &uri, json!({ "main": [cards[0]] })).await;
    assert_eq!(
        t.scalar("SELECT count(*) FROM card_category_assignment")
            .await,
        1
    );

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
async fn proxies_clone_and_delete() {
    let t = TestApp::new().await;
    let (v, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}");
    // the Kuriboh proxy belongs to a lot
    t.json("PATCH", &uri, json!({ "main": [cards[0], cards[2]] }))
        .await;

    let (status, proxy) = t
        .json(
            "POST",
            &format!("{uri}/proxies"),
            json!({ "name": "Dark Magician", "zone": "extra" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{proxy}");
    assert_eq!(proxy["is_proxy"], true);
    assert_eq!(proxy["card_data_id"], 1);
    let proxy_id = proxy["id"].as_i64().unwrap();

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
    assert_eq!(cloned["main"][0]["id"], cards[0], "real cards are shared");
    assert_ne!(cloned["extra"][0]["id"], proxy_id, "proxies are duplicated");
    assert_eq!(cloned["versions"].as_array().unwrap().len(), 2);

    let (status, _) = t
        .json("DELETE", &format!("{uri}/proxies/{proxy_id}"), json!(null))
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        t.scalar(&format!("SELECT count(*) FROM card WHERE id = {proxy_id}"))
            .await,
        0
    );
    let (status, _) = t
        .json(
            "DELETE",
            &format!("{uri}/proxies/{}", cards[0]),
            json!(null),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "owned cards can't be deleted"
    );

    let (status, deleted) = t.json("DELETE", &uri, json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(deleted["deck_id"].is_i64());
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM card WHERE id = {}",
            cards[2]
        ))
        .await,
        1,
        "a proxy bought in a lot is kept"
    );
    assert_eq!(
        t.json("GET", &uri, json!(null)).await.0,
        StatusCode::NOT_FOUND
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
    let proxies: Vec<bool> = deck["main"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["is_proxy"].as_bool().unwrap())
        .collect();
    assert_eq!(
        proxies.iter().filter(|p| !**p).count(),
        2,
        "both owned copies are reused"
    );
    assert_eq!(proxies.len(), 3);
    assert_eq!(
        deck["side"][0]["is_proxy"], true,
        "the owned Kuriboh is a proxy, so a new proxy is made"
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

#[tokio::test]
async fn saving_zones_deletes_unused_proxies() {
    let t = TestApp::new().await;
    let (version, _) = t.deck_fixture().await;
    // no owned Kuriboh: only saving the zones can remove this proxy
    let (status, proxy) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{version}/proxies"),
            json!({ "name": "Kuriboh", "zone": "main" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let proxy = proxy["id"].as_i64().unwrap();

    let (status, _) = t
        .json(
            "PATCH",
            &format!("/api/deck-versions/{version}"),
            json!({ "main": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        t.scalar(&format!("SELECT count(*) FROM card WHERE id = {proxy}"))
            .await,
        0
    );
}

#[tokio::test]
async fn new_cards_replace_deck_proxies_in_the_background() {
    let t = TestApp::new().await;
    let (version, _) = t.deck_fixture().await;
    let (status, _) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{version}/proxies"),
            json!({ "name": "Dark Magician", "zone": "main" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    // by name, not by id: SQLite reuses the id of a deleted card
    let proxies_in_main = format!(
        "SELECT count(*) FROM deck_version_main_deck m JOIN card ON card.id = m.card_id
         WHERE m.deck_version_id = {version} AND card.is_proxy = 1 AND card.card_data_id = 1"
    );
    assert_eq!(t.scalar(&proxies_in_main).await, 1);

    // saving new cards runs the background task: link, translate, then
    // replace the proxy by an owned Dark Magician and delete it
    t.post("/lots/add", LOT).await;
    for _ in 0..100 {
        if t.scalar(&proxies_in_main).await == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(t.scalar(&proxies_in_main).await, 0, "proxy replaced");
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_version_main_deck m JOIN card ON card.id = m.card_id
             WHERE m.deck_version_id = {version} AND card.is_proxy = 0 AND card.card_data_id = 1"
        ))
        .await,
        1,
        "by an owned copy"
    );
    assert_eq!(
        t.scalar(
            "SELECT count(*) FROM card WHERE is_proxy = 1 AND card_data_id = 1
             AND id NOT IN (SELECT card_id FROM unite)"
        )
        .await,
        0,
        "the unused proxy is deleted"
    );
}
