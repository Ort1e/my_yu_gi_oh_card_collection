//! Every page renders; static files; the deck builder page.

use super::*;

#[tokio::test]
async fn pages_render() {
    let t = TestApp::new().await;
    for uri in [
        "/",
        "/budget",
        "/cards",
        "/lots",
        "/sellers",
        "/decks",
        "/lots/add",
        "/decks/add",
        "/budget/manage",
        "/rarities",
        "/collection/value",
    ] {
        let (status, body) = t.get(uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    assert_eq!(t.get("/lots/12345").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn static_files_are_revalidated() {
    let t = TestApp::new().await;
    let response = t
        .router()
        .oneshot(
            Request::get("/static/css/style.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
}

#[tokio::test]
async fn deck_builder_page() {
    let t = TestApp::new().await;
    let (v, _) = t.deck_fixture().await;
    let (status, page) = t.get(&format!("/deck-builder/{v}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(&format!(r#"data-deck-version-id="{v}""#)));
    assert_eq!(t.get("/deck-builder/999").await.0, StatusCode::NOT_FOUND);
}
