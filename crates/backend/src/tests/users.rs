//! The users page: admins only, permissions that take effect at once, the
//! last admin kept, a new password that logs the user out.

use super::security::{deployed, get_with, login, send_with};
use super::*;

fn form(uri: &str) -> axum::http::request::Builder {
    Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
}

async fn user_id(t: &TestApp, name: &str) -> i64 {
    t.scalar(&format!("SELECT id FROM app_user WHERE name = '{name}'"))
        .await
}

#[tokio::test]
async fn only_admins_manage_the_users() {
    let t = TestApp::with_config(deployed()).await;
    let (_, friend) = login(&t, "friend-pw").await;
    let friend = friend.unwrap();
    let (_, admin) = login(&t, "owner-pw").await;
    let admin = admin.unwrap();

    assert_eq!(get_with(&t, "/users", &friend).await, StatusCode::FORBIDDEN);
    let (status, _) = send_with(
        &t,
        form("/users/add"),
        "name=x&password=longenough",
        &friend,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(t.scalar("SELECT count(*) FROM app_user").await, 2);
    assert_eq!(get_with(&t, "/users", "").await, StatusCode::SEE_OTHER);

    let (status, page) = send_with(&t, Request::get("/users"), "", &admin).await;
    assert_eq!(status, StatusCode::OK);
    for expected in [
        "friend",
        "admin",
        "the Yu-Gi-Oh! cards",
        "Ortie",
        "Delete friend",
    ] {
        assert!(page.contains(expected), "{expected}");
    }
    // the login page links admins to it
    let (_, page) = send_with(&t, Request::get("/login"), "", &admin).await;
    assert!(page.contains("/users"));
}

#[tokio::test]
async fn a_new_user_logs_in_with_what_was_ticked() {
    let t = TestApp::with_config(deployed()).await;
    let (_, admin) = login(&t, "owner-pw").await;
    let admin = admin.unwrap();

    // refused: a short password, a bad name, a taken name
    for body in [
        "name=neri&password=short&perm=books",
        "name=Ne+ri&password=longenough",
        "name=Friend&password=longenough",
    ] {
        let (status, _) = send_with(&t, form("/users/add"), body, &admin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(t.scalar("SELECT count(*) FROM app_user").await, 2);

    let (status, _) = send_with(
        &t,
        form("/users/add"),
        "name=Neri&password=longenough&perm=books&perm=nonsense",
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.scalar("SELECT count(*) FROM app_user_permission WHERE user_id = (SELECT id FROM app_user WHERE name = 'neri')")
            .await,
        1
    );

    let response = t
        .router()
        .oneshot(
            form("/login")
                .body(Body::from("name=NERI&password=longenough&next=%2F"))
                .unwrap(),
        )
        .await
        .unwrap();
    let neri = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(neri.starts_with("session=neri."), "{neri}");
    assert_eq!(get_with(&t, "/books/add", &neri).await, StatusCode::OK);
    assert_eq!(
        get_with(&t, "/lots/add", &neri).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(get_with(&t, "/users", &neri).await, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_permission_change_takes_effect_at_once() {
    let t = TestApp::with_config(deployed()).await;
    let (_, admin) = login(&t, "owner-pw").await;
    let admin = admin.unwrap();
    let (_, friend) = login(&t, "friend-pw").await;
    let friend = friend.unwrap();
    let id = user_id(&t, "friend").await;
    assert_eq!(
        get_with(&t, "/pokemon/add", &friend).await,
        StatusCode::FORBIDDEN
    );

    let save = |body: String| {
        let (t, admin) = (&t, &admin);
        async move {
            send_with(t, form(&format!("/users/{id}")), &body, admin)
                .await
                .0
        }
    };
    assert_eq!(
        save("name=friend&password=&perm=books&perm=pokemon".into()).await,
        StatusCode::SEE_OTHER
    );
    assert_eq!(get_with(&t, "/pokemon/add", &friend).await, StatusCode::OK);

    // taken away again, and the same session notices
    assert_eq!(
        save("name=friend&perm=books".into()).await,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get_with(&t, "/pokemon/add", &friend).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(get_with(&t, "/books/add", &friend).await, StatusCode::OK);
}

#[tokio::test]
async fn a_new_password_logs_that_user_out() {
    let t = TestApp::with_config(deployed()).await;
    let (_, admin) = login(&t, "owner-pw").await;
    let admin = admin.unwrap();
    let (_, friend) = login(&t, "friend-pw").await;
    let friend = friend.unwrap();
    let id = user_id(&t, "friend").await;
    assert_eq!(get_with(&t, "/cards", &friend).await, StatusCode::OK);

    let (status, _) = send_with(
        &t,
        form(&format!("/users/{id}")),
        "name=friend&password=brand-new-pw&perm=books",
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(get_with(&t, "/cards", &friend).await, StatusCode::SEE_OTHER);
    assert_eq!(login(&t, "friend-pw").await.0, StatusCode::UNAUTHORIZED);

    // an admin changing their own password keeps their session
    let admin_id = user_id(&t, "admin").await;
    let response = t
        .router()
        .oneshot(
            form(&format!("/users/{admin_id}"))
                .header(header::COOKIE, admin.split(';').next().unwrap())
                .body(Body::from("name=admin&password=another-new-pw&admin=1"))
                .unwrap(),
        )
        .await
        .unwrap();
    let fresh = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(get_with(&t, "/users", &fresh).await, StatusCode::OK);
    assert_eq!(get_with(&t, "/users", &admin).await, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn the_last_admin_and_yourself_are_kept() {
    let t = TestApp::with_config(deployed()).await;
    let (_, admin) = login(&t, "owner-pw").await;
    let admin = admin.unwrap();
    let admin_id = user_id(&t, "admin").await;
    let friend_id = user_id(&t, "friend").await;

    // not an admin any more: refused, the last one
    let (status, page) = send_with(
        &t,
        form(&format!("/users/{admin_id}")),
        "name=admin&perm=books",
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(page.contains("last admin"), "{page}");
    assert_eq!(
        t.scalar("SELECT is_admin FROM app_user WHERE name = 'admin'")
            .await,
        1
    );

    let (status, _) = send_with(&t, form(&format!("/users/{admin_id}/delete")), "", &admin).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // another admin: now one may step down, and the other can be deleted
    let (status, _) = send_with(
        &t,
        form(&format!("/users/{friend_id}")),
        "name=friend&admin=1",
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _) = send_with(&t, form(&format!("/users/{friend_id}/delete")), "", &admin).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM app_user").await, 1);
    assert_eq!(
        t.scalar("SELECT count(*) FROM app_user_permission").await,
        0
    );

    // a renamed user keeps logging in under the new name
    let (status, _) = send_with(
        &t,
        form(&format!("/users/{admin_id}")),
        "name=boss&admin=1",
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.text("SELECT name FROM app_user").await.as_deref(),
        Some("boss")
    );
}
