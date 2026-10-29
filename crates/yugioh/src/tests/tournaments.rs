//! Tournaments: the form (rounds, games, labels), the pages, the labels
//! page, the deck labels and the deck builder's summary
//! (`routes/tournaments/`, `store/tournaments.rs`).

use serde_json::json;

use super::*;

/// A tournament with two rounds (a loss with 4 games, one a draw, and a bye
/// typed at a later index), played with `version`.
fn form(version: &str) -> Vec<(&'static str, String)> {
    let pairs = vec![
        ("name", "Locals".to_string()),
        ("date", "2026-09-20".into()),
        ("location", "Rennes".into()),
        ("version_id", version.into()),
        ("placement", "2".into()),
        ("players", "6".into()),
        ("is_team", "on".into()),
        ("conclusion", "Good deck.".into()),
        ("rounds-0-result", "loss".into()),
        ("rounds-0-die", "won".into()),
        ("rounds-0-team_result", "win".into()),
        ("rounds-0-opponent_deck", "mitsu/K9".into()),
        ("rounds-0-labels", "Mitsurugi, K9, k9".into()),
        ("rounds-0-comment", "Close one.".into()),
        ("rounds-0-games-0-result", "won".into()),
        ("rounds-0-games-0-turn", "first".into()),
        ("rounds-0-games-1-result", "draw".into()),
        ("rounds-0-games-2-result", "lost".into()),
        ("rounds-0-games-2-turn", "second".into()),
        ("rounds-0-games-2-comment", "Bricked.".into()),
        ("rounds-0-games-3-result", "lost".into()),
        // a blank round and a blank game are skipped
        ("rounds-1-result", "".into()),
        ("rounds-1-opponent_deck", " ".into()),
        ("rounds-0-games-7-result", "".into()),
        ("rounds-4-result", "bye".into()),
    ];
    pairs
}

async fn post_form(t: &TestApp, uri: &str, pairs: &[(&str, String)]) -> (StatusCode, String) {
    let pairs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    t.post(uri, &pairs).await
}

#[tokio::test]
async fn adding_a_tournament_with_rounds_games_and_labels() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;

    let (status, location) = post_form(&t, "/tournaments/add", &form(&v.to_string())).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(location, "/tournaments/1");

    assert_eq!(
        t.scalar("SELECT tournament_id FROM deck_version WHERE id = 1")
            .await,
        1
    );
    // the deck played: its read-only page, from the tournament and the list
    let link = format!(r#"href="/deck-versions/{v}""#);
    assert!(t.get("/tournaments/1").await.1.contains(&link));
    assert!(t.get("/tournaments").await.1.contains(&link));
    assert_eq!(
        t.text("SELECT group_concat(result, ',') FROM (SELECT result FROM tournament_round ORDER BY number)")
            .await
            .as_deref(),
        Some("loss,bye"),
        "numbered in order, the blank round skipped"
    );
    assert_eq!(
        t.text("SELECT group_concat(result || ':' || coalesce(went_first, '-'), ',') FROM (SELECT result, went_first FROM tournament_game ORDER BY number)")
            .await
            .as_deref(),
        Some("won:1,draw:-,lost:0,lost:-"),
        "four games, the draw included, the blank one skipped"
    );
    assert_eq!(
        t.text("SELECT group_concat(name, ',') FROM (SELECT name FROM deck_label ORDER BY name)")
            .await
            .as_deref(),
        Some("K9,Mitsurugi"),
        "k9 and K9 are one label"
    );

    let (status, page) = t.get("/tournaments/1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("against mitsu/K9"), "{page}");
    assert!(page.contains("K9, Mitsurugi"));
    assert!(page.contains("Team: Win"));
    assert!(page.contains("going first"));
    assert!(page.contains("Bricked."));
    assert!(page.contains("2nd of 6"));
    assert!(page.contains("0-1, 1 bye"));
    assert!(!page.contains("Old notes"), "no old notes to show");

    // the list and its stats
    let (status, page) = t.get("/tournaments").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Locals"));
    assert!(page.contains("Test - v1"));
    assert!(page.contains("Matchups"));
}

#[tokio::test]
async fn editing_replaces_the_rounds() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    post_form(&t, "/tournaments/add", &form(&v.to_string())).await;

    let (status, page) = t.get("/tournaments/1/edit").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("value=\"mitsu/K9\""));
    assert!(page.contains("value=\"K9, Mitsurugi\""));

    // only the bye left, the team event unticked: the labels go too
    let (status, location) = t
        .post(
            "/tournaments/1/edit",
            &[
                ("name", "Locals"),
                ("date", "2026-09-20"),
                ("version_id", ""),
                ("rounds-3-result", "bye"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(t.scalar("SELECT count(*) FROM tournament_round").await, 1);
    assert_eq!(t.scalar("SELECT number FROM tournament_round").await, 1);
    assert_eq!(t.scalar("SELECT count(*) FROM tournament_game").await, 0);
    assert_eq!(t.scalar("SELECT count(*) FROM deck_label").await, 0);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version WHERE tournament_id IS NOT NULL")
            .await,
        0,
        "the version is no longer the one played"
    );
}

#[tokio::test]
async fn invalid_forms_come_back_with_the_values() {
    let t = TestApp::new().await;
    t.deck_fixture().await;

    let (status, page) = t
        .post(
            "/tournaments/add",
            &[
                ("name", "Locals"),
                ("date", "2026-09-20"),
                ("placement", "9"),
                ("players", "8"),
                ("rounds-0-opponent_deck", "Tenpai"),
                ("rounds-1-result", "bye"),
                ("rounds-1-opponent_deck", "Snake-Eye"),
                ("rounds-2-result", "win"),
                ("rounds-2-team_result", "win"),
                ("rounds-2-games-0-turn", "first"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    for error in [
        "Placement: 9 is more than the 8 players.",
        "Round 1: choose its result.",
        "Round 2: a bye has no opponent, labels, die nor games.",
        "Round 3: a team result is only for team events",
        "Round 3, game 1: choose its result.",
    ] {
        assert!(page.contains(error), "missing {error:?}");
    }
    assert!(page.contains("value=\"Tenpai\""), "values kept");
    assert_eq!(t.scalar("SELECT count(*) FROM tournament").await, 0);
}

#[tokio::test]
async fn deleting_keeps_the_version() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    post_form(&t, "/tournaments/add", &form(&v.to_string())).await;

    let (status, location) = t.post("/tournaments/1/delete", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, "/tournaments");
    for table in [
        "tournament",
        "tournament_round",
        "tournament_game",
        "deck_label",
    ] {
        assert_eq!(
            t.scalar(&format!("SELECT count(*) FROM {table}")).await,
            0,
            "{table}"
        );
    }
    assert_eq!(t.scalar("SELECT count(*) FROM deck_version").await, 1);
    assert_eq!(
        t.post("/tournaments/1/delete", &[]).await.0,
        StatusCode::NOT_FOUND
    );
}

async fn label(t: &TestApp, name: &str) -> i64 {
    t.scalar(&format!("SELECT id FROM deck_label WHERE name = '{name}'"))
        .await
}

#[tokio::test]
async fn labels_are_renamed_and_merged() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    post_form(&t, "/tournaments/add", &form(&v.to_string())).await;
    // a second spelling, on the deck
    t.post(
        "/decks/1/edit",
        &[("name", "Test"), ("labels", "Mitsu, Orcust")],
    )
    .await;

    let (_, page) = t.get("/tournaments/labels").await;
    assert!(page.contains("Mitsu") && page.contains("Orcust"));

    // renaming to a name taken (any case) is refused
    let mitsu = label(&t, "Mitsu").await;
    let (status, page) = t
        .post(
            &format!("/tournaments/labels/{mitsu}/rename"),
            &[("name", "mitsurugi")],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("merge them instead"));

    // merged: the deck now has Mitsurugi
    let mitsurugi = label(&t, "Mitsurugi").await;
    let (status, _) = t
        .post(
            &format!("/tournaments/labels/{mitsu}/merge"),
            &[("into", &mitsurugi.to_string())],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_label WHERE name = 'Mitsu'")
            .await,
        0
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM deck_deck_label WHERE deck_id = 1 AND label_id = {mitsurugi}"
        ))
        .await,
        1
    );

    let orcust = label(&t, "Orcust").await;
    let (status, _) = t
        .post(
            &format!("/tournaments/labels/{orcust}/rename"),
            &[("name", "Orcustrated")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (_, page) = t.get("/decks/1").await;
    assert!(page.contains("Labels: Mitsurugi, Orcustrated"), "{page}");
    assert!(page.contains("/tournaments?deck=1"));
    let (_, page) = t.get("/decks/1/edit").await;
    assert!(page.contains("value=\"Mitsurugi, Orcustrated\""));
}

#[tokio::test]
async fn the_list_filters_and_the_old_notes() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    post_form(&t, "/tournaments/add", &form(&v.to_string())).await;
    sqlx::query(
        "INSERT INTO tournament (id, name, date, notes) VALUES (2, 'Regionals', '2025-01-05', 'vs DDD 1/2');",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();

    let (_, page) = t.get("/tournaments").await;
    assert!(page.contains("Locals") && page.contains("Regionals"));
    let (_, page) = t.get("/tournaments?since=2026-01-01&deck=").await;
    assert!(page.contains("Locals") && !page.contains("Regionals"));
    let (_, page) = t.get("/tournaments?deck=1&since=nonsense").await;
    assert!(page.contains("Locals") && !page.contains("Regionals"));

    let (_, page) = t.get("/tournaments/2").await;
    assert!(page.contains("Old notes") && page.contains("vs DDD 1/2"));
    let (_, page) = t.get("/tournaments/2/edit").await;
    assert!(page.contains("vs DDD 1/2"));
    assert!(page.contains("Clear the old notes"));

    // cleared only when asked
    t.post(
        "/tournaments/2/edit",
        &[("name", "Regionals"), ("date", "2025-01-05")],
    )
    .await;
    assert!(t
        .text("SELECT notes FROM tournament WHERE id = 2")
        .await
        .is_some());
    t.post(
        "/tournaments/2/edit",
        &[
            ("name", "Regionals"),
            ("date", "2025-01-05"),
            ("clear_notes", "on"),
        ],
    )
    .await;
    assert!(t
        .text("SELECT notes FROM tournament WHERE id = 2")
        .await
        .is_none());
}

#[tokio::test]
async fn the_deck_builder_shows_a_summary_and_copies_are_not_played() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    let uri = format!("/api/deck-versions/{v}");
    let (_, deck) = t.json("GET", &uri, json!(null)).await;
    assert_eq!(deck["tournament"], json!(null));

    post_form(&t, "/tournaments/add", &form(&v.to_string())).await;
    let (_, deck) = t.json("GET", &uri, json!(null)).await;
    assert_eq!(
        deck["tournament"],
        json!({ "id": 1, "name": "Locals", "date": "2026-09-20", "location": "Rennes",
                "placement": 2, "players": 6 })
    );

    let (status, created) = t
        .json("POST", &format!("{uri}/clone"), json!({ "name": "v2" }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (_, copy) = t
        .json(
            "GET",
            &format!("/api/deck-versions/{}", created["id"]),
            json!(null),
        )
        .await;
    assert_eq!(copy["tournament"], json!(null));
}
