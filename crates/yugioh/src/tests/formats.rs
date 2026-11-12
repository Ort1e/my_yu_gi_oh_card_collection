//! Formats: their pages, a deck version playing one, its check (no
//! network: the sets' dates and the card data's sets are inserted).

use serde_json::json;

use super::*;
use crate::testing::{tiles, DM, KURIBOH};

/// Konami's list of 2010-03-01 (Dark Magician limited, Kuriboh forbidden),
/// two sets, the card data's sets: Dark Magician from 2002, Kuriboh 2012.
async fn konami_list(t: &TestApp) {
    t.exec(
        r#"INSERT INTO advanced_ban_list (id, date) VALUES (1, '2010-03-01');
           INSERT INTO ban_list_entry (ban_list_id, card_data_id, status)
               VALUES (1, 1, 'Limited'), (1, 2, 'Banned');
           INSERT INTO ygo_set (name, code, tcg_date) VALUES
               ('Legend of Blue Eyes White Dragon', 'LOB', '2002-03-08'),
               ('A Late Set', 'LATE', '2012-01-01'),
               ('An OCG Set', 'OCG', NULL);
           UPDATE card_data SET json_data = '{"name": "Dark Magician",
               "card_sets": [{"set_name": "Legend of Blue Eyes White Dragon"}]}' WHERE id = 1;
           UPDATE card_data SET json_data = '{"name": "Kuriboh",
               "card_sets": [{"set_name": "A Late Set"}, {"set_name": "An OCG Set"}]}' WHERE id = 2;"#,
    )
    .await;
}

async fn edison(t: &TestApp, cutoff: &str) -> i64 {
    let (status, location) = t
        .post(
            "/formats/add",
            &[
                ("name", "Edison"),
                ("cutoff", cutoff),
                ("based_on", "1"),
                ("notes", ""),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    location.strip_prefix("/formats/").unwrap().parse().unwrap()
}

#[tokio::test]
async fn a_format_from_a_konami_list_and_its_versions() {
    let t = TestApp::new().await;
    konami_list(&t).await;
    let (status, page) = t.get("/formats/add").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains(r#"<option value="Legend of Blue Eyes White Dragon">08/03/2002</option>"#)
    );

    // the cutoff from a set's date; the list copied from Konami's
    let (status, location) = t
        .post(
            "/formats/add",
            &[
                ("name", "Edison"),
                ("cutoff", ""),
                ("cutoff_set", "Legend of Blue Eyes White Dragon"),
                ("based_on", "1"),
                ("notes", "The community's rules"),
            ],
        )
        .await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/formats/1")
    );
    assert_eq!(
        t.text("SELECT cutoff || ' ' || version FROM deck_format")
            .await
            .as_deref(),
        Some("2002-03-08 1")
    );
    assert_eq!(t.scalar("SELECT count(*) FROM deck_format_entry").await, 2);
    let (_, page) = t.get("/formats/1").await;
    for text in [
        "Edison v1",
        "08/03/2002",
        "Dark Magician",
        "The community&#x27;s rules",
    ] {
        assert!(page.contains(text), "{text}: {page}");
    }
    // Kuriboh (2012) is listed but out of the pool anyway
    assert!(page.contains(r#"class="format-out""#), "{page}");
    // a read page: the forms are on the edit page
    assert!(page.contains(r#"href="/formats/1/edit""#), "{page}");
    assert!(!page.contains("<form"), "{page}");
    let (status, page) = t.get("/formats/1/edit").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    for text in [
        r#"action="/formats/1/edit""#,
        r#"action="/formats/1/cards""#,
        r#"action="/formats/1/new-version""#,
        r#"action="/formats/1/delete""#,
        r#"<option value="Dark Magician">"#,
    ] {
        assert!(page.contains(text), "{text}: {page}");
    }

    // a second format of that name, a set without a date: refused
    let (status, page) = t
        .post(
            "/formats/add",
            &[("name", "Edison"), ("cutoff_set", "An OCG Set")],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("already named"), "{page}");
    assert!(page.contains("no TCG date known for the set"), "{page}");

    // a status changed, a card off the list, a card added by its name
    t.post("/formats/1/cards/1", &[("status", "Semi-Limited")])
        .await;
    t.post("/formats/1/cards/2", &[("status", "")]).await;
    assert_eq!(
        t.text("SELECT group_concat(card_data_id || ':' || status) FROM deck_format_entry")
            .await
            .as_deref(),
        Some("1:Semi-Limited")
    );
    let (status, _) = t
        .post(
            "/formats/1/cards",
            &[("name", "Kuriboh"), ("status", "Limited")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM deck_format_entry").await, 2);

    // a new version: a copy, the old one unchanged by the next edits
    let (status, location) = t.post("/formats/1/new-version", &[]).await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/formats/2/edit")
    );
    t.post("/formats/2/cards/1", &[("status", "Banned")]).await;
    let status_in = |format: i64| {
        format!(
            "SELECT status FROM deck_format_entry WHERE format_id = {format} AND card_data_id = 1"
        )
    };
    assert_eq!(t.text(&status_in(1)).await.as_deref(), Some("Semi-Limited"));
    assert_eq!(t.text(&status_in(2)).await.as_deref(), Some("Banned"));
    let (_, list) = t.get("/formats").await;
    assert!(
        list.contains("Edison v2") && list.contains("Edison v1"),
        "{list}"
    );
    let (_, page) = t.get("/formats/2").await;
    assert!(page.contains(r#"<a href="/formats/1">v1</a>"#), "{page}");

    // edited; deleted (nothing plays it)
    t.post(
        "/formats/2/edit",
        &[
            ("name", "Edison"),
            ("cutoff", "2010-03-01"),
            ("notes", "v2 notes"),
        ],
    )
    .await;
    assert_eq!(
        t.text("SELECT cutoff FROM deck_format WHERE id = 2")
            .await
            .as_deref(),
        Some("2010-03-01")
    );
    let (status, location) = t.post("/formats/2/delete", &[]).await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/formats")
    );
    assert_eq!(t.scalar("SELECT count(*) FROM deck_format").await, 1);
}

#[tokio::test]
async fn a_deck_version_plays_a_format() {
    let t = TestApp::new().await;
    konami_list(&t).await;
    let (version, _) = t.deck_fixture().await;
    let format = edison(&t, "2010-03-01").await;
    let url = format!("/api/deck-versions/{version}");

    // both Dark Magicians (limited), a Kuriboh not owned (2012, forbidden)
    let (status, deck) = t
        .json(
            "PATCH",
            &url,
            json!({ "main": tiles(&[(DM, "Magicien Sombre", 2)]), "side": tiles(&[(KURIBOH, "Kuriboh", 1)]),
                    "format_id": format, "ban_list_id": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{deck}");
    assert_eq!(deck["format_id"], format);
    assert_eq!(deck["formats"][0]["label"], "Edison v1");
    assert_eq!(deck["formats"][0]["cutoff"], "2010-03-01");
    assert_eq!(deck["main"][0]["ban"][format!("f{format}")], "Limited");
    assert_eq!(deck["main"][0]["ban"]["1"], "Limited");
    assert_eq!(deck["main"][0]["first_release"], "2002-03-08");
    assert_eq!(deck["side"][0]["first_release"], "2012-01-01");

    // the missing Kuriboh is looked up for errata (a plain list holds no
    // copy to judge: the Dark Magicians are not)
    let (status, check) = t.json("GET", &format!("{url}/check"), json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        check,
        json!({ "format": "Edison v1", "problems": [
            "Dark Magician: 2 copies, 1 allowed",
            "Kuriboh: released on 01/01/2012, after the format's cutoff",
            "Kuriboh: forbidden (1 in the deck)",
        ], "errata": [], "errata_pending": 1, "wants": "1 Kuriboh",
                "wants_note": check["wants_note"] })
    );

    // a ban list and a format: refused, nothing changed
    let (status, answer) = t.json("PATCH", &url, json!({ "ban_list_id": 1 })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(
        t.scalar(&format!(
            "SELECT format_id FROM deck_version WHERE id = {version}"
        ))
        .await,
        format
    );

    // the read-only page names the format (its problems are the builder's)
    let (_, page) = t.get(&format!("/deck-versions/{version}")).await;
    assert!(
        page.contains(r#"Format: <a href="/formats/1" class="on-card">Edison v1</a>"#),
        "{page}"
    );

    // a copy keeps the format; the format can't be deleted while played
    let (status, _) = t
        .json("POST", &format!("{url}/clone"), json!({ "name": "v2" }))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version WHERE format_id = 1")
            .await,
        2
    );
    let (status, page) = t.post("/formats/1/delete", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("2 deck versions play this format"), "{page}");

    // back to a ban list: no format check any more (its cards' errata are
    // still looked up, at the list's date)
    let (status, _) = t
        .json(
            "PATCH",
            &url,
            json!({ "ban_list_id": 1, "format_id": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, check) = t.json("GET", &format!("{url}/check"), json!(null)).await;
    assert_eq!(
        check,
        json!({ "format": null, "problems": [], "errata": [], "errata_pending": 1, "wants": "1 Kuriboh",
                "wants_note": check["wants_note"] })
    );
}

#[tokio::test]
async fn reading_formats_is_public_changing_them_needs_the_login() {
    let t = TestApp::with_password(Some("secret")).await;
    konami_list(&t).await;
    t.exec("INSERT INTO deck_format (id, name, cutoff) VALUES (1, 'Goat', '2005-04-01')")
        .await;

    for page in ["/formats", "/formats/1"] {
        assert_eq!(t.get(page).await.0, StatusCode::OK, "{page}");
    }
    assert_eq!(t.get("/formats/add").await.0, StatusCode::SEE_OTHER);
    assert_eq!(t.get("/formats/1/edit").await.0, StatusCode::SEE_OTHER);
    for (url, form) in [
        (
            "/formats/add",
            vec![("name", "X"), ("cutoff", "2005-01-01")],
        ),
        (
            "/formats/1/edit",
            vec![("name", "X"), ("cutoff", "2005-01-01")],
        ),
        (
            "/formats/1/cards",
            vec![("name", "Kuriboh"), ("status", "Banned")],
        ),
        ("/formats/1/new-version", vec![]),
        ("/formats/1/delete", vec![]),
    ] {
        assert_eq!(t.post(url, &form).await.0, StatusCode::SEE_OTHER, "{url}");
    }
    assert_eq!(t.scalar("SELECT count(*) FROM deck_format").await, 1);
    assert_eq!(
        t.text("SELECT name FROM deck_format").await.as_deref(),
        Some("Goat")
    );
}
