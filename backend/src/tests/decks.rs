//! Decks: creation, edition, versions, deletion.

use super::*;

#[tokio::test]
async fn create_deck_with_first_version() {
    let t = TestApp::new().await;
    let (status, location) = t
        .post(
            "/decks/add",
            &[
                ("name", "Exodia"),
                ("description", ""),
                ("version_name", "v1"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let deck_id = t.scalar("SELECT id FROM deck").await;
    assert_eq!(location, format!("/decks/{deck_id}"));
    assert_eq!(
        t.scalar("SELECT count(*) FROM deck_version WHERE version_name = 'v1'")
            .await,
        1
    );
    assert_eq!(t.text("SELECT description FROM deck").await, None);

    let (status, page) = t
        .post("/decks/add", &[("name", "X"), ("version_name", "")])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Version name is required."));
}

#[tokio::test]
async fn deck_edit_versions_and_delete() {
    let t = TestApp::new().await;
    let (version, _) = t.deck_fixture().await;
    let deck = t.scalar("SELECT id FROM deck").await;
    assert_eq!(
        t.get(&format!("/decks/{deck}/edit")).await.0,
        StatusCode::OK
    );

    let (status, _) = t
        .post(
            &format!("/decks/{deck}/edit"),
            &[("name", "Renamed"), ("description", "desc")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.text("SELECT name || ' ' || description FROM deck")
            .await
            .as_deref(),
        Some("Renamed desc")
    );
    let (status, page) = t
        .post(&format!("/decks/{deck}/edit"), &[("name", " ")])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Name is required."));

    let (status, location) = t
        .post(
            &format!("/decks/{deck}/versions"),
            &[("version_name", "v2")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let v2 = t
        .scalar("SELECT id FROM deck_version WHERE version_name = 'v2'")
        .await;
    assert_eq!(location, format!("/deck-builder/{v2}"));
    let (status, page) = t
        .post(&format!("/decks/{deck}/versions"), &[("version_name", "")])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Version name is required."));
    assert_eq!(
        t.post("/decks/999/versions", &[("version_name", "v")])
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // a proxy only used by this deck goes with it; owned cards stay
    let mut conn = t.state.pool.acquire().await.unwrap();
    let proxy = crate::cards::new_proxy(&mut conn, "Exodia", None)
        .await
        .unwrap();
    drop(conn);
    sqlx::query("INSERT INTO deck_version_main_deck VALUES (?, ?)")
        .bind(version)
        .bind(proxy)
        .execute(&t.state.pool)
        .await
        .unwrap();
    let cards = t.scalar("SELECT count(*) FROM card").await;

    let (status, location) = t.post(&format!("/decks/{deck}/delete"), &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, "/decks");
    assert_eq!(t.scalar("SELECT count(*) FROM deck_version").await, 0);
    assert_eq!(t.scalar("SELECT count(*) FROM card").await, cards - 1);
    assert_eq!(
        t.post(&format!("/decks/{deck}/delete"), &[]).await.0,
        StatusCode::NOT_FOUND
    );
}
