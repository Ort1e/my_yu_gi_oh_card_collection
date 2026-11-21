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

    // the list played, saved by the form
    assert_eq!(
        t.scalar("SELECT version_id FROM deck_snapshot WHERE tournament_id = 1")
            .await,
        v
    );
    let snapshot = t
        .scalar("SELECT id FROM deck_snapshot WHERE tournament_id = 1")
        .await;
    // linked from the tournament and the list
    let link = format!(r#"href="/deck-snapshots/{snapshot}""#);
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
    let (status, page) = t.get("/tournaments/stats").await;
    assert_eq!(status, StatusCode::OK);
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
    // in sections, Save in sight; the lost round's games folded, the bye's
    // (no game) open
    for text in [
        "<legend>The event</legend>",
        "<legend>The list played</legend>",
        r#"<h2 class="form-section">The rounds</h2>"#,
        r#"<div class="sticky-save">"#,
    ] {
        assert!(page.contains(text), "{text}: {page}");
    }
    assert_eq!(
        page.matches(r#"<details class="round-games" >"#).count(),
        1,
        "{page}"
    );
    assert!(
        page.contains(r#"<details class="round-games" open>"#),
        "{page}"
    );

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
        t.scalar("SELECT count(*) FROM deck_snapshot WHERE tournament_id IS NOT NULL")
            .await,
        0,
        "no list played any more"
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
    assert!(page.contains("/tournaments/stats?deck=1"));
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

    // the list has no stats any more: they are on their own page, which
    // follows the same filter
    let (_, page) = t.get("/tournaments").await;
    assert!(!page.contains("By deck version"), "{page}");
    assert!(page.contains("/tournaments/stats\""), "{page}");
    let (_, page) = t.get("/tournaments?since=2026-01-01&deck=1").await;
    assert!(
        page.contains("/tournaments/stats?since=2026-01-01&amp;deck=1"),
        "{page}"
    );
    let (status, page) = t.get("/tournaments/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("By deck version") && page.contains("Overall"),
        "{page}"
    );
    assert!(page.contains("action=\"/tournaments/stats\""), "{page}");
    let (_, page) = t.get("/tournaments/stats?deck=1&since=nonsense").await;
    assert!(page.contains("By deck version"), "{page}");

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

// ---------- live notes ----------

fn live_round(result: &str, labels: &[&str], games: &[&str]) -> serde_json::Value {
    json!({
        "opponent_deck": labels.join(" / "),
        "labels": labels,
        "die": "won",
        "result": result,
        "games": games.iter().map(|g| json!({ "result": g, "went_first": null })).collect::<Vec<_>>(),
    })
}

#[tokio::test]
async fn the_live_start_creates_today_s_tournament_then_redirects_to_it() {
    let t = TestApp::new().await;
    let today = crate::store::db::today().to_string();

    let (status, page) = t.get("/tournaments/live").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Start today"));

    let (status, location) = t
        .post(
            "/tournaments/live",
            &[
                ("name", "Locals"),
                ("location", "Rennes"),
                ("is_team", "on"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    assert_eq!(location, "/tournaments/1/live");
    assert_eq!(
        t.text("SELECT date FROM tournament WHERE id = 1").await,
        Some(today)
    );
    assert_eq!(
        t.scalar("SELECT is_team FROM tournament WHERE id = 1")
            .await,
        1
    );

    let (status, location) = t.get("/tournaments/live").await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/tournaments/1/live")
    );
    let (status, page) = t.get("/tournaments/1/live").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(r#"id="live-data""#));

    // a name is needed
    let (status, page) = t.post("/tournaments/live", &[("name", " ")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Name"));
}

#[tokio::test]
async fn posting_to_discord_is_offered_only_with_a_webhook() {
    let t = TestApp::new().await;
    let (_, page) = t.get("/tournaments/live").await;
    assert!(!page.contains("post_to_discord"));
    // the switch can't be forced without a webhook
    t.post(
        "/tournaments/live",
        &[("name", "Locals"), ("post_to_discord", "on")],
    )
    .await;
    assert_eq!(t.scalar("SELECT post_to_discord FROM tournament").await, 0);

    let t = TestApp::with_discord().await;
    let (_, page) = t.get("/tournaments/live").await;
    assert!(page.contains(r#"name="post_to_discord""#), "{page}");
    t.post("/tournaments/live", &[("name", "Off")]).await;
    let (_, page) = t.get("/tournaments/1/live").await;
    assert!(!page.contains("📣 Discord"));
    t.post(
        "/tournaments/live",
        &[("name", "Locals"), ("post_to_discord", "on")],
    )
    .await;
    assert_eq!(
        t.scalar("SELECT post_to_discord FROM tournament WHERE name = 'Locals'")
            .await,
        1
    );
    let (_, page) = t.get("/tournaments/2/live").await;
    assert!(page.contains("📣 Discord"), "{page}");

    // saves go on as before (the messages are written after the commit; an
    // old page's `share_line` / `share_text` are ignored)
    let mut round = live_round("win", &["K9"], &["won", "won"]);
    round["share_line"] = json!("R1: W 2-0 vs K9");
    let (status, _) = t.json("PUT", "/api/tournaments/2/rounds/1", round).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = t
        .json(
            "PATCH",
            "/api/tournaments/2",
            json!({ "placement": 1, "share_text": "R1: W 2-0 vs K9" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn the_start_form_is_filled_from_the_last_tournament() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    post_form(&t, "/tournaments/add", &form(&v.to_string())).await;

    let (_, page) = t.get("/tournaments/live").await;
    assert!(page.contains(r#"value="Locals""#), "{page}");
    assert!(page.contains(r#"value="Rennes""#));
    assert!(page.contains("checked"));
    assert!(page.contains(&format!(r#"<option value="{v}" selected"#)));
}

#[tokio::test]
async fn a_live_round_is_replaced_not_duplicated_and_has_no_gaps() {
    let t = TestApp::new().await;
    t.post("/tournaments/live", &[("name", "Locals")]).await;
    let put = |n: i64, body: serde_json::Value| {
        let t = &t;
        async move {
            t.json("PUT", &format!("/api/tournaments/1/rounds/{n}"), body)
                .await
        }
    };

    let (status, _) = put(1, live_round("win", &["Mitsurugi"], &["won", "won"])).await;
    assert_eq!(status, StatusCode::OK);
    // the same round sent again with other values
    let (status, _) = put(1, live_round("loss", &["K9"], &["lost", "lost"])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(t.scalar("SELECT count(*) FROM tournament_round").await, 1);
    assert_eq!(t.scalar("SELECT count(*) FROM tournament_game").await, 2);
    assert_eq!(
        t.text("SELECT result FROM tournament_round")
            .await
            .as_deref(),
        Some("loss")
    );
    // Mitsurugi was only in the replaced round: its label is gone
    assert_eq!(
        t.text("SELECT group_concat(name) FROM deck_label")
            .await
            .as_deref(),
        Some("K9")
    );

    // round 3 with only 1 noted
    let (status, error) = put(3, live_round("win", &[], &["won"])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    let (status, _) = put(2, live_round("bye", &[], &[])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(t.scalar("SELECT count(*) FROM tournament_round").await, 2);

    let (status, _) = put(1, json!({ "result": "nope" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = t
        .json(
            "PUT",
            "/api/tournaments/9/rounds/1",
            live_round("win", &[], &[]),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_live_end_of_the_day_changes_only_what_is_sent() {
    let t = TestApp::new().await;
    t.post("/tournaments/live", &[("name", "Locals")]).await;

    let (status, _) = t
        .json(
            "PATCH",
            "/api/tournaments/1",
            json!({ "placement": 3, "players": 12, "conclusion": "Fun." }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = t
        .json("PATCH", "/api/tournaments/1", json!({ "placement": 2 }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(t.scalar("SELECT placement FROM tournament").await, 2);
    assert_eq!(t.scalar("SELECT players FROM tournament").await, 12);
    assert_eq!(
        t.text("SELECT conclusion FROM tournament").await.as_deref(),
        Some("Fun.")
    );

    // 0 and "" empty a field
    t.json(
        "PATCH",
        "/api/tournaments/1",
        json!({ "placement": 0, "conclusion": "" }),
    )
    .await;
    assert_eq!(t.text("SELECT placement FROM tournament").await, None,);
    assert_eq!(t.text("SELECT conclusion FROM tournament").await, None);
    let (status, _) = t
        .json("PATCH", "/api/tournaments/1", json!({ "players": -1 }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_chips_come_most_met_first() {
    let t = TestApp::new().await;
    t.post("/tournaments/live", &[("name", "Locals")]).await;
    for (n, labels) in [(1, ["Rare"]), (2, ["Often"]), (3, ["Often"])] {
        t.json(
            "PUT",
            &format!("/api/tournaments/1/rounds/{n}"),
            live_round("win", &labels, &["won"]),
        )
        .await;
    }

    let (_, page) = t.get("/tournaments/1/live").await;
    assert!(page.contains(r#""chips":["Often","Rare"]"#), "{page}");
}

#[tokio::test]
async fn the_live_page_and_its_api_need_the_login() {
    let t = TestApp::with_password(Some("secret")).await;
    for uri in ["/tournaments/live", "/tournaments/1/live"] {
        assert_eq!(t.get(uri).await.0, StatusCode::SEE_OTHER, "{uri}");
    }
    assert_eq!(
        t.post("/tournaments/live", &[("name", "x")]).await.0,
        StatusCode::SEE_OTHER
    );
    let (status, answer) = t
        .json(
            "PUT",
            "/api/tournaments/1/rounds/1",
            live_round("win", &[], &[]),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(answer["error"].is_string());
    let (status, _) = t.json("PATCH", "/api/tournaments/1", json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, answer) = t.json("GET", "/api/tournaments/1/live", json!(null)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(answer["error"].is_string());
}

#[tokio::test]
async fn the_live_api_gives_the_page_s_data() {
    let t = TestApp::new().await;
    t.post("/tournaments/live", &[("name", "Locals")]).await;
    t.json(
        "PUT",
        "/api/tournaments/1/rounds/1",
        live_round("win", &["Mitsurugi"], &["won", "lost", "won"]),
    )
    .await;

    let (status, data) = t.json("GET", "/api/tournaments/1/live", json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["name"], "Locals");
    assert_eq!(data["rounds"][0]["number"], 1);
    assert_eq!(data["rounds"][0]["labels"], json!(["Mitsurugi"]));
    assert_eq!(data["rounds"][0]["games"].as_array().unwrap().len(), 3);
    assert_eq!(data["chips"], json!(["Mitsurugi"]));

    let (status, _) = t.json("GET", "/api/tournaments/9/live", json!(null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------- archetypes ----------

#[tokio::test]
async fn archetypes_are_offered_and_give_their_spelling_to_the_labels() {
    let t = TestApp::new().await;
    t.post("/tournaments/live", &[("name", "Locals")]).await;
    // labels typed before the archetypes were known
    for (n, label) in [
        (1, "snake eye"),
        (2, "Snake eye"),
        (3, "Snake-Eye Fire King"),
    ] {
        t.json(
            "PUT",
            &format!("/api/tournaments/1/rounds/{n}"),
            live_round("win", &[label], &["won"]),
        )
        .await;
    }
    assert_eq!(t.scalar("SELECT count(*) FROM deck_label").await, 2);

    let names: Vec<String> = ["Snake-Eye", "Labrynth", "Tenpai Dragon"]
        .iter()
        .map(|n| n.to_string())
        .collect();
    let report = crate::store::card_data::store_archetypes(&t.state.pool, &names)
        .await
        .unwrap();
    assert_eq!((report.archetypes, report.relabelled), (3, 1));
    // renamed, the deck name left as typed
    assert_eq!(
        t.text("SELECT group_concat(name, '|') FROM (SELECT name FROM deck_label ORDER BY name)")
            .await
            .as_deref(),
        Some("Snake-Eye|Snake-Eye Fire King")
    );

    // a round typed now takes the archetype's spelling
    t.json(
        "PUT",
        "/api/tournaments/1/rounds/4",
        live_round("loss", &["LABRYNTH", "tenpai-dragon"], &["lost"]),
    )
    .await;
    let (_, data) = t.json("GET", "/api/tournaments/1/live", json!(null)).await;
    assert_eq!(
        data["rounds"][3]["labels"],
        json!(["Labrynth", "Tenpai Dragon"])
    );

    // the suggestions: our labels and the archetypes, each once
    assert_eq!(
        data["labels"],
        json!([
            "Labrynth",
            "Snake-Eye",
            "Snake-Eye Fire King",
            "Tenpai Dragon"
        ])
    );
    let (_, page) = t.get("/tournaments/add").await;
    assert!(page.contains(r#"<option value="Snake-Eye Fire King">"#));

    // a label spelt otherwise next to the archetype's own: merged into it
    t.json(
        "PUT",
        "/api/tournaments/1/rounds/5",
        live_round("win", &["Snake-Eye"], &["won"]),
    )
    .await;
    t.exec("INSERT INTO deck_label (name) VALUES ('snake  eye')")
        .await;
    let typo = t
        .scalar("SELECT id FROM deck_label WHERE name = 'snake  eye'")
        .await;
    t.exec(&format!(
        "INSERT INTO tournament_round_label (round_id, label_id) SELECT id, {typo} FROM tournament_round WHERE number = 3"
    ))
    .await;
    let report = crate::store::card_data::store_archetypes(&t.state.pool, &names)
        .await
        .unwrap();
    assert_eq!(report.relabelled, 1);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_label WHERE name LIKE 'snake%eye'")
            .await,
        1
    );
    assert_eq!(
        t.scalar(
            "SELECT count(*) FROM tournament_round_label rl JOIN deck_label l ON l.id = rl.label_id
             WHERE l.name = 'Snake-Eye'"
        )
        .await,
        4
    );
}
