//! Login.

use super::*;
use serde_json::json;

#[tokio::test]
async fn login_guards_writes_and_shipment_files() {
    let t = TestApp::with_password(Some("secret")).await;
    std::fs::create_dir_all(t.state.media_dir.join("shipment_files")).unwrap();
    std::fs::write(t.state.media_dir.join("shipment_files/s.pdf"), "pdf").unwrap();
    let send = |request: Request<Body>| async { t.router().oneshot(request).await.unwrap() };
    let form = |uri: &str, body: &str, cookie: &str| {
        Request::post(uri)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, cookie)
            .header(header::REFERER, "http://host/decks/add?x=1")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    // reads stay public
    assert_eq!(t.get("/lots").await.0, StatusCode::OK);
    assert_eq!(t.get("/login").await.0, StatusCode::OK);

    // writes: pages redirect to the login form, the API answers 401
    assert_eq!(
        t.get("/lots/add?a=1").await,
        (
            StatusCode::SEE_OTHER,
            "/login?next=%2Flots%2Fadd%3Fa%3D1".into()
        )
    );
    let response = send(form("/decks/add", "name=X&version_name=v1", "")).await;
    assert_eq!(
        response.headers()[header::LOCATION],
        "/login?next=%2Fdecks%2Fadd%3Fx%3D1"
    );
    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 0);
    let (status, body) = t.json("DELETE", "/api/deck-versions/1", json!(null)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "Log in to make changes.");
    assert_eq!(
        t.get("/media/shipment_files/s.pdf").await.0,
        StatusCode::SEE_OTHER
    );

    // wrong password, then the right one
    let (status, page) = t
        .post("/login", &[("password", "nope"), ("next", "/decks/add")])
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(page.contains("Wrong password."));
    let response = send(form("/login", "password=secret&next=%2F%2Fevil", "")).await;
    assert_eq!(response.headers()[header::LOCATION], "/");
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let response = send(form("/decks/add", "name=X&version_name=v1", &cookie)).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM deck").await, 1);
    let response = send(
        Request::get("/media/shipment_files/s.pdf")
            .header(header::COOKIE, format!("theme=dark; {cookie}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // other media stay public
    std::fs::write(t.state.media_dir.join("other.txt"), "x").unwrap();
    assert_eq!(t.get("/media/other.txt").await.0, StatusCode::OK);

    // a tampered cookie is refused
    let tampered = cookie.replace("session=", "session=1");
    let response = send(form("/decks/add", "name=Y&version_name=v1", &tampered)).await;
    assert!(response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .starts_with("/login"));
}

#[tokio::test]
async fn login_disabled_without_password() {
    let t = TestApp::new().await;
    let (status, page) = t.get("/login").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("login is disabled"));
    assert_eq!(
        t.post("/login", &[("password", "")]).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(t.get("/lots/add").await.0, StatusCode::OK);
}
