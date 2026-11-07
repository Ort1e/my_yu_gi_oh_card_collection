//! A deck's history: the snapshots saved with the zones, named, played in a
//! tournament, compared, deleted, made a version again
//! (`store/snapshots.rs`, `routes/decks/snapshots.rs`).

use serde_json::json;

use super::*;

async fn snapshots(t: &TestApp) -> i64 {
    t.scalar("SELECT count(*) FROM deck_snapshot").await
}

/// `count` copies by snapshot zone, `main:Magicien Sombre:2,…`.
async fn cards_of(t: &TestApp, id: i64) -> String {
    t.text(&format!(
        "SELECT group_concat(zone || ':' || name || ':' || count, ',') FROM
         (SELECT * FROM deck_snapshot_card WHERE snapshot_id = {id} ORDER BY zone, position)"
    ))
    .await
    .unwrap_or_default()
}

#[tokio::test]
async fn saving_the_zones_keeps_a_history() {
    let t = TestApp::new().await;
    let (version, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{version}");
    assert_eq!(snapshots(&t).await, 0, "an empty new version has none");

    let (status, body) = t
        .json(
            "PATCH",
            &uri,
            json!({ "main": [cards[0], cards[1]], "side": [cards[2]] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(snapshots(&t).await, 1);
    let first = t.scalar("SELECT id FROM deck_snapshot").await;
    assert_eq!(
        cards_of(&t, first).await,
        "main:Magicien Sombre:2,side:Kuriboh:1",
        "by card, with counts"
    );

    // the same cards: nothing new
    t.json("PATCH", &uri, json!({ "main": [cards[1], cards[0]] }))
        .await;
    assert_eq!(snapshots(&t).await, 1);

    // changed in the same session: the automatic one follows
    t.json("PATCH", &uri, json!({ "main": [cards[0]] })).await;
    assert_eq!(snapshots(&t).await, 1);
    assert_eq!(
        cards_of(&t, first).await,
        "main:Magicien Sombre:1,side:Kuriboh:1"
    );

    // named: kept, the next change is a new one
    let (status, body) = t
        .json("PATCH", &uri, json!({ "snapshot_name": "Before locals" }))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(snapshots(&t).await, 1, "the current one is named");
    assert_eq!(
        t.text(&format!(
            "SELECT name FROM deck_snapshot WHERE id = {first}"
        ))
        .await
        .as_deref(),
        Some("Before locals")
    );
    t.json("PATCH", &uri, json!({ "main": [cards[0], cards[1]] }))
        .await;
    assert_eq!(snapshots(&t).await, 2);

    // the deck's page lists them; the snapshot's page shows the changes
    let deck = t.scalar("SELECT id FROM deck").await;
    let (_, page) = t.get(&format!("/decks/{deck}")).await;
    assert!(page.contains("Before locals"), "{page}");
    assert!(page.contains("1 automatic save"), "{page}");
    assert!(page.contains("+1 −0"), "{page}");
    let last = t.scalar("SELECT max(id) FROM deck_snapshot").await;
    let (status, page) = t.get(&format!("/deck-snapshots/{last}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Before locals"), "compared with it: {page}");
    assert!(page.contains("1 → 2"), "{page}");
    assert!(page.contains("×2"), "{page}");

    // compared with the version now: the same
    let (_, page) = t
        .get(&format!("/deck-snapshots/{last}?vs=version:{version}"))
        .await;
    assert!(page.contains("The same cards as v1 now"), "{page}");

    // a version gone, its history stays
    t.json("DELETE", &uri, json!({})).await;
    assert_eq!(snapshots(&t).await, 2);
    let (_, page) = t.get(&format!("/deck-snapshots/{last}")).await;
    assert!(page.contains("From a version since deleted"), "{page}");
}

#[tokio::test]
async fn only_automatic_ones_are_deleted() {
    let t = TestApp::new().await;
    let (version, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{version}");
    t.json("PATCH", &uri, json!({ "main": [cards[0]] })).await;
    let id = t.scalar("SELECT id FROM deck_snapshot").await;

    let (status, _) = t
        .json(
            "POST",
            &format!("{uri}/history/{id}/name"),
            json!({ "name": "Keep me" }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // another deck's version can't name it
    t.post("/decks/add", &[("name", "Other"), ("version_name", "v1")])
        .await;
    let other = t.scalar("SELECT MAX(id) FROM deck_version").await;
    let (status, _) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{other}/history/{id}/name"),
            json!({ "name": "Not mine" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, page) = t.get(&format!("/deck-snapshots/{id}")).await;
    assert!(page.contains("Named “Keep me”"), "{page}");
    assert!(
        page.contains(&format!("/deck-builder/{version}#history")),
        "{page}"
    );
    let (status, _) = t.post(&format!("/deck-snapshots/{id}/delete"), &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(snapshots(&t).await, 1);

    t.json(
        "POST",
        &format!("{uri}/history/{id}/name"),
        json!({ "name": " " }),
    )
    .await;
    let (status, location) = t.post(&format!("/deck-snapshots/{id}/delete"), &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(snapshots(&t).await, 0);
    assert_eq!(
        t.post(&format!("/deck-snapshots/{id}/delete"), &[]).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_tournament_freezes_the_list_played() {
    let t = TestApp::new().await;
    let (version, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{version}");
    t.json("PATCH", &uri, json!({ "main": [cards[0], cards[1]] }))
        .await;

    let v = version.to_string();
    let (status, location) = t
        .post(
            "/tournaments/add",
            &[
                ("name", "Locals"),
                ("date", "2026-09-20"),
                ("version_id", &v),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    let frozen = t
        .scalar("SELECT id FROM deck_snapshot WHERE tournament_id = 1")
        .await;
    assert_eq!(cards_of(&t, frozen).await, "main:Magicien Sombre:2");

    // the version changes afterwards: the list played doesn't
    t.json(
        "PATCH",
        &uri,
        json!({ "main": [cards[0]], "side": [cards[2]] }),
    )
    .await;
    assert_eq!(cards_of(&t, frozen).await, "main:Magicien Sombre:2");
    let (_, page) = t.get(&format!("/deck-snapshots/{frozen}")).await;
    assert!(page.contains("Played at"), "{page}");

    // edited with the list kept: the same one
    let (_, form) = t.get("/tournaments/1/edit").await;
    assert!(form.contains(r#"value="keep" selected"#), "{form}");
    t.post(
        "/tournaments/1/edit",
        &[
            ("name", "Locals"),
            ("date", "2026-09-20"),
            ("version_id", "keep"),
        ],
    )
    .await;
    assert_eq!(
        t.scalar("SELECT id FROM deck_snapshot WHERE tournament_id = 1")
            .await,
        frozen
    );

    // the version chosen again: its list now
    t.post(
        "/tournaments/1/edit",
        &[
            ("name", "Locals"),
            ("date", "2026-09-20"),
            ("version_id", &v),
        ],
    )
    .await;
    let again = t
        .scalar("SELECT id FROM deck_snapshot WHERE tournament_id = 1")
        .await;
    assert_ne!(again, frozen);
    assert_eq!(
        cards_of(&t, again).await,
        "main:Magicien Sombre:1,side:Kuriboh:1"
    );
}

#[tokio::test]
async fn a_saved_list_becomes_a_version_again() {
    let t = TestApp::new().await;
    let (version, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{version}");
    t.json(
        "PATCH",
        &uri,
        json!({ "main": [cards[0], cards[1]], "side": [cards[2]] }),
    )
    .await;
    let id = t.scalar("SELECT id FROM deck_snapshot").await;

    let (status, location) = t
        .post(
            &format!("/deck-snapshots/{id}/branch"),
            &[("version_name", "v2")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    let new = t
        .scalar("SELECT id FROM deck_version WHERE version_name = 'v2'")
        .await;
    assert_eq!(location, format!("/deck-builder/{new}"));
    // the owned copies (free: v1 isn't built), a new proxy for the proxy
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_version_main_deck z JOIN card ON card.id = z.card_id
             WHERE deck_version_id = {new} AND card.is_proxy = 0"
        ))
        .await,
        2
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_version_side_deck z JOIN card ON card.id = z.card_id
             WHERE deck_version_id = {new} AND card.is_proxy = 1"
        ))
        .await,
        1
    );
    // and its own history starts
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_snapshot WHERE version_id = {new}"
        ))
        .await,
        1
    );

    let (status, _) = t
        .post(
            &format!("/deck-snapshots/{id}/branch"),
            &[("version_name", "")],
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_builder_compares_the_screen_with_a_saved_list() {
    let t = TestApp::new().await;
    let (version, cards) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{version}");
    t.json("PATCH", &uri, json!({ "main": [cards[0]] })).await;

    let (status, list) = t.json("GET", &format!("{uri}/history"), json!(null)).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["version_name"], "v1");
    assert_eq!(list[0]["main"], 1);
    let id = list[0]["id"].as_i64().unwrap();

    // unsaved: one more copy, the proxy in the side deck
    let (status, changes) = t
        .json(
            "POST",
            &format!("{uri}/compare"),
            json!({ "snapshot_id": id, "main": [cards[0], cards[1]], "extra": [], "side": [cards[2]] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changes}");
    assert_eq!(
        changes,
        json!([
            { "zone": "main", "name": "Magicien Sombre", "before": 1, "after": 2 },
            { "zone": "side", "name": "Kuriboh", "before": 0, "after": 1 },
        ])
    );

    // another deck's list: not found
    t.post("/decks/add", &[("name", "Other"), ("version_name", "v1")])
        .await;
    let other = t.scalar("SELECT max(id) FROM deck_version").await;
    let (status, _) = t
        .json(
            "POST",
            &format!("/api/deck-versions/{other}/compare"),
            json!({ "snapshot_id": id, "main": [], "extra": [], "side": [] }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
