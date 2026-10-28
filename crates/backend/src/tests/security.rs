//! The protections of the deployed app (deploy/README.md, "Security"): private
//! mode, two users and what each may change, the cookie, the limit on wrong
//! passwords, the security headers, the media folder.

use super::*;
use common::testing::read;
use common::web::auth::{AuthConfig, Changes, User, MAX_FAILURES};

/// The deployed settings: the owner and a friend who changes the books
/// only, everything private, HTTPS.
fn deployed() -> AuthConfig {
    AuthConfig {
        users: vec![
            User::new("friend", "friend-pw", Changes::books()),
            User::new("owner", "owner-pw", Changes::Everything),
        ],
        secure_cookies: true,
        public_reads: false,
    }
}

/// Log in with `password`: the answer's status and its session cookie.
async fn login(t: &TestApp, password: &str) -> (StatusCode, Option<String>) {
    let response = t
        .router()
        .oneshot(
            Request::post("/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("password={password}&next=%2F")))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .map(|c| c.to_str().unwrap().to_string());
    (response.status(), cookie)
}

/// Send `request` with the session `cookie`: (status, Location or body).
async fn send_with(
    t: &TestApp,
    request: axum::http::request::Builder,
    body: &str,
    cookie: &str,
) -> (StatusCode, String) {
    let cookie = cookie.split(';').next().unwrap();
    let response = t
        .router()
        .oneshot(
            request
                .header(header::COOKIE, cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    read(response).await
}

async fn get_with(t: &TestApp, uri: &str, cookie: &str) -> StatusCode {
    let cookie = cookie.split(';').next().unwrap();
    t.router()
        .oneshot(
            Request::get(uri)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// A page of each kind, in each collection.
const PAGES: &[&str] = &[
    "/",
    "/cards",
    "/cards/1",
    "/lots",
    "/sellers",
    "/decks",
    "/budget",
    "/collection/value",
    "/tournaments",
    "/rarities",
    "/advent-calendar",
    "/books",
    "/books/authors",
    "/books/advent-calendar",
    "/pokemon",
    "/pokemon/openings",
    "/pokemon/wishlist",
    "/pokemon/rarities",
];

#[tokio::test]
async fn private_mode_puts_every_page_behind_the_login() {
    let t = TestApp::with_config(deployed()).await;
    t.exec(
        "INSERT INTO card (id, name, en_name, code, card_data_id, is_proxy)
             VALUES (1, 'Magicien Sombre', 'dark magician', 'LOB-FR005', 1, 0)",
    )
    .await;
    std::fs::create_dir_all(t.state.media_dir.join("card_images")).unwrap();
    std::fs::write(
        t.state.media_dir.join("card_images/dark_magician.jpg"),
        "jpg",
    )
    .unwrap();

    for page in PAGES {
        let (status, location) = t.get(page).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{page}");
        assert!(location.starts_with("/login?next="), "{page}: {location}");
    }
    for file in ["/card-image/1", "/media/card_images/dark_magician.jpg"] {
        assert_eq!(t.get(file).await.0, StatusCode::SEE_OTHER, "{file}");
    }
    for api in ["/api/deck-versions/1", "/pokemon/api/search?q=pikachu"] {
        assert_eq!(t.get(api).await.0, StatusCode::UNAUTHORIZED, "{api}");
    }
    // the login page itself stays open
    assert_eq!(t.get("/login").await.0, StatusCode::OK);

    // logged in, the pages open
    let (_, cookie) = login(&t, "owner-pw").await;
    let cookie = cookie.unwrap();
    for page in PAGES {
        assert_eq!(get_with(&t, page, &cookie).await, StatusCode::OK, "{page}");
    }
    assert_eq!(
        get_with(&t, "/media/card_images/dark_magician.jpg", &cookie).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn each_user_has_a_password_and_a_secure_cookie() {
    let t = TestApp::with_config(deployed()).await;

    let (status, cookie) = login(&t, "friend-pw").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let cookie = cookie.unwrap();
    assert!(cookie.starts_with("session=friend."), "{cookie}");
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    assert!(cookie.ends_with("; Secure"), "{cookie}");
    assert_eq!(get_with(&t, "/cards", &cookie).await, StatusCode::OK);

    // a session forged for the other user doesn't open anything
    let forged = cookie.replacen("session=friend.", "session=owner.", 1);
    assert_eq!(get_with(&t, "/cards", &forged).await, StatusCode::SEE_OTHER);

    // logging out clears the cookie the same way
    let response = t
        .router()
        .oneshot(Request::post("/logout").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let cleared = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(
        cleared.contains("Max-Age=0") && cleared.ends_with("; Secure"),
        "{cleared}"
    );
}

#[tokio::test]
async fn the_friend_sees_everything_and_changes_the_books_only() {
    let t = TestApp::with_config(deployed()).await;
    t.exec(
        "INSERT INTO card (id, name, en_name, code, card_data_id, is_proxy)
             VALUES (1, 'Magicien Sombre', 'dark magician', 'LOB-FR005', 1, 0)",
    )
    .await;
    // the rights are checked before the handler: a missing version will do
    let version_id = 999;
    let (_, friend) = login(&t, "friend-pw").await;
    let friend = friend.unwrap();

    for page in PAGES {
        assert_eq!(get_with(&t, page, &friend).await, StatusCode::OK, "{page}");
    }
    let (_, page) = send_with(&t, Request::get("/login"), "", &friend).await;
    assert!(page.contains("change the books"), "{page}");

    // the books, their calendars and the ISBN lookup's page: allowed
    for page in ["/books/add", "/books/advent-calendar/add"] {
        assert_eq!(get_with(&t, page, &friend).await, StatusCode::OK, "{page}");
    }
    let form = Request::post("/books/add")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    let (status, _) = send_with(&t, form, "title=Dune&authors=Frank+Herbert", &friend).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM book").await, 1);

    // the other collections' changes: refused, the reads still open
    for page in [
        "/lots/add",
        "/advent-calendar/add",
        "/pokemon/add",
        "/pokemon/wishlist/add",
    ] {
        let (status, body) = send_with(&t, Request::get(page), "", &friend).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{page}");
        assert!(
            body.contains("can see everything, and change the books"),
            "{page}: {body}"
        );
    }
    let form = Request::post("/sellers/1/edit")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    assert_eq!(
        send_with(&t, form, "name=Renamed", &friend).await.0,
        StatusCode::FORBIDDEN
    );
    let patch = Request::patch(format!("/api/deck-versions/{version_id}"))
        .header(header::CONTENT_TYPE, "application/json");
    let (status, body) = send_with(&t, patch, r#"{"name": "mine"}"#, &friend).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body.contains(r#""error":"Your login can't change the Yu-Gi-Oh! cards.""#),
        "{body}"
    );
    let deck = format!("/api/deck-versions/{version_id}");
    // a read: through to the handler
    assert_eq!(get_with(&t, &deck, &friend).await, StatusCode::NOT_FOUND);

    // the owner changes everything
    let (_, owner) = login(&t, "owner-pw").await;
    let owner = owner.unwrap();
    for page in ["/lots/add", "/pokemon/add", "/books/add"] {
        assert_eq!(get_with(&t, page, &owner).await, StatusCode::OK, "{page}");
    }
}

#[tokio::test]
async fn app_access_gives_a_user_more() {
    let mut config = deployed();
    config.users[0].changes = Changes::Only(vec!["books".into(), "pokemon".into()]);
    let t = TestApp::with_config(config).await;
    let (_, friend) = login(&t, "friend-pw").await;
    let friend = friend.unwrap();
    assert_eq!(get_with(&t, "/pokemon/add", &friend).await, StatusCode::OK);
    assert_eq!(
        get_with(&t, "/lots/add", &friend).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn too_many_wrong_passwords_lock_the_login() {
    let t = TestApp::with_config(deployed()).await;
    for _ in 0..MAX_FAILURES {
        assert_eq!(login(&t, "guess").await.0, StatusCode::UNAUTHORIZED);
    }

    // even the right password is refused for a while
    let (status, cookie) = login(&t, "owner-pw").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(cookie.is_none());
    let (_, page) = t.post("/login", &[("password", "owner-pw")]).await;
    assert!(page.contains("Too many wrong passwords"), "{page}");
}

#[tokio::test]
async fn answers_carry_the_security_headers() {
    let t = TestApp::new().await;
    for uri in ["/", "/login", "/api/deck-versions/999", "/no-such-page"] {
        let response = t
            .router()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let headers = response.headers();
        let csp = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(
            csp.contains("script-src 'self' https://cdn.jsdelivr.net"),
            "{uri}"
        );
        assert!(csp.contains("frame-ancestors 'none'"), "{uri}");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff", "{uri}");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY", "{uri}");
    }
}

#[tokio::test]
async fn the_media_folder_cannot_be_left() {
    let t = TestApp::new().await;
    std::fs::create_dir_all(&t.state.media_dir).unwrap();
    let secret = t.state.media_dir.parent().unwrap().join("secret.txt");
    std::fs::write(&secret, "the database's neighbour").unwrap();

    for uri in [
        "/media/../secret.txt",
        "/media/%2e%2e/secret.txt",
        "/media/..%2fsecret.txt",
        "/media/%2e%2e%2fsecret.txt",
        "/media/card_images/../../secret.txt",
        "/static/../secret.txt",
    ] {
        let (status, body) = t.get(uri).await;
        assert_ne!(status, StatusCode::OK, "{uri}");
        assert!(!body.contains("neighbour"), "{uri}");
    }
}
