//! Several wishlists: one permission each, only the lists filled from the
//! collection ticked off by new copies, a deleted list's grants and cards.

use super::pokemon::{fixture, HORSEA_IR};
use super::*;
use common::web::auth::{AuthConfig, SeedUser};

/// An admin, `ortie` (the collection and the list "Ortie", id 1), and
/// `neri`, who has no right yet: the tests give her her list once it exists.
async fn app() -> TestApp {
    let mut config = AuthConfig::password(Some("admin-pw"));
    config.users.push(SeedUser::new(
        "ortie",
        "ortie-pw",
        &["pokemon", "pokemon_wishlist:1"],
    ));
    config
        .users
        .push(SeedUser::new("neri", "neri-pw", &["books"]));
    let t = TestApp::with_config(config).await;
    fixture(&t).await;
    t
}

/// Create the list "Neri" as the admin and grant it to `neri`.
async fn neris_list(t: &TestApp, admin: &str) -> i64 {
    let (status, _) = t
        .post_as(admin, "/pokemon/wishlists/add", &[("name", "Neri")])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let id = t
        .scalar("SELECT id FROM pokemon_wishlist WHERE name = 'Neri'")
        .await;
    t.exec(&format!(
        "INSERT INTO app_user_permission (user_id, permission)
         SELECT id, 'pokemon_wishlist:{id}' FROM app_user WHERE name = 'neri'"
    ))
    .await;
    t.state.auth.reload(&t.state.pool).await.unwrap();
    id
}

const WISH: &[(&str, &str)] = &[
    ("card", "sv06.5-067"),
    ("catalogue", "intl"),
    ("language", ""),
    ("variant", ""),
    ("priority", "1"),
];

#[tokio::test]
async fn each_list_is_changed_by_its_own_user() {
    let t = app().await;
    let admin = t.login("admin", "admin-pw").await;
    let ortie = t.login("ortie", "ortie-pw").await;
    let neri = t.login("neri", "neri-pw").await;
    let neris = neris_list(&t, &admin).await;
    assert_eq!(neris, 2);

    // Neri adds to hers, not to Ortie's
    let (status, location) = t
        .post_as(&neri, &format!("/pokemon/wishlists/{neris}/add"), WISH)
        .await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/pokemon/wishlists/2")
    );
    let (status, _) = t.post_as(&neri, "/pokemon/wishlists/1/add", WISH).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        t.get_as(&neri, "/pokemon/wishlists/1/add").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM pokemon_wish WHERE wishlist_id = 1")
            .await,
        0
    );

    // Ortie adds to hers, not to Neri's
    let (status, _) = t.post_as(&ortie, "/pokemon/wishlists/1/add", WISH).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _) = t.post_as(&ortie, "/pokemon/wishlists/2/add", WISH).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // a wish is changed with its list's permission
    let ortie_wish = t
        .scalar("SELECT id FROM pokemon_wish WHERE wishlist_id = 1")
        .await;
    let neri_wish = t
        .scalar("SELECT id FROM pokemon_wish WHERE wishlist_id = 2")
        .await;
    for (who, wish, allowed) in [(&neri, ortie_wish, false), (&neri, neri_wish, true)] {
        let edit = format!("/pokemon/wishes/{wish}/edit");
        assert_eq!(
            t.get_as(who, &edit).await.0 == StatusCode::OK,
            allowed,
            "{edit}"
        );
        let (status, _) = t
            .post_as(who, &format!("/pokemon/wishes/{wish}/got"), &[("got", "1")])
            .await;
        assert_eq!(status == StatusCode::SEE_OTHER, allowed);
    }
    assert_eq!(
        t.post_as(&neri, &format!("/pokemon/wishes/{ortie_wish}/delete"), &[])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // her own list: ticked off without a copy, then wanted again
    assert!(t
        .text(&format!(
            "SELECT got_on FROM pokemon_wish WHERE id = {neri_wish}"
        ))
        .await
        .is_some());
    t.post_as(&neri, &format!("/pokemon/wishes/{neri_wish}/got"), &[])
        .await;
    assert!(t
        .text(&format!(
            "SELECT got_on FROM pokemon_wish WHERE id = {neri_wish}"
        ))
        .await
        .is_none());

    // the collection and the lists' management are not hers
    assert_eq!(
        t.get_as(&neri, "/pokemon/add").await.0,
        StatusCode::FORBIDDEN
    );
    for uri in ["/pokemon/wishlists/add", "/pokemon/wishlists/2/delete"] {
        let (status, _) = t.post_as(&neri, uri, &[("name", "Mine")]).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
        let (status, _) = t.post_as(&ortie, uri, &[("name", "Mine")]).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }

    // reading stays public, and edit buttons only show to who may use them
    let (status, page) = t.get("/pokemon/wishlists/2").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!page.contains("/pokemon/wishes/"), "{page}");
    let (_, page) = t.get_as(&neri, "/pokemon/wishlists/2").await;
    assert!(page.contains(&format!("/pokemon/wishes/{neri_wish}/edit")));
    let (_, page) = t.get_as(&neri, "/pokemon/wishlists/1").await;
    assert!(!page.contains("/pokemon/wishes/"), "{page}");
    let (_, page) = t.get("/pokemon/wishlist").await;
    assert!(page.contains("Ortie") && page.contains("Neri"));
    assert!(!page.contains("Delete Neri"));
    let (_, page) = t.get_as(&admin, "/pokemon/wishlist").await;
    assert!(page.contains("Delete Neri"));
}

#[tokio::test]
async fn a_copy_ticks_off_only_the_lists_filled_from_the_collection() {
    let t = app().await;
    let admin = t.login("admin", "admin-pw").await;
    let ortie = t.login("ortie", "ortie-pw").await;
    let neris = neris_list(&t, &admin).await;
    for list in [1, neris] {
        let (status, _) = t
            .post_as(&admin, &format!("/pokemon/wishlists/{list}/add"), WISH)
            .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    // a copy of the card: offered for Ortie's wish only
    let (_, location) = t.post_as(&ortie, "/pokemon/add", HORSEA_IR).await;
    let copy: i64 = location
        .strip_prefix("/pokemon/add?added=")
        .unwrap()
        .parse()
        .unwrap();
    let (_, page) = t
        .get_as(&ortie, &format!("/pokemon/add?added={copy}"))
        .await;
    assert_eq!(page.matches("was on your wishlist").count(), 1, "{page}");
    // the admin may change both lists, and still sees only the filled one
    let (_, page) = t
        .get_as(&admin, &format!("/pokemon/add?added={copy}"))
        .await;
    assert_eq!(page.matches("was on your wishlist").count(), 1, "{page}");

    // with the flag on Neri's list too, both are offered
    let (status, _) = t
        .post_as(
            &admin,
            &format!("/pokemon/wishlists/{neris}/edit"),
            &[("name", "Neri"), ("fills_from_collection", "1")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (_, page) = t
        .get_as(&admin, &format!("/pokemon/add?added={copy}"))
        .await;
    assert_eq!(page.matches("was on your wishlist").count(), 2, "{page}");
    // but whoever may not change a list isn't offered its wish
    let (_, page) = t
        .get_as(&ortie, &format!("/pokemon/add?added={copy}"))
        .await;
    assert_eq!(page.matches("was on your wishlist").count(), 1, "{page}");

    // marking it got keeps the copy and the date
    let wish = t
        .scalar("SELECT id FROM pokemon_wish WHERE wishlist_id = 1")
        .await;
    let (status, _) = t
        .post_as(
            &ortie,
            &format!("/pokemon/wishes/{wish}/got"),
            &[("copy_id", &copy.to_string())],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.scalar(&format!(
            "SELECT got_copy_id = {copy} AND got_on IS NOT NULL FROM pokemon_wish WHERE id = {wish}"
        ))
        .await,
        1
    );
}

#[tokio::test]
async fn deleting_a_list_removes_its_grants_wishes_and_orphan_cards() {
    let t = app().await;
    let admin = t.login("admin", "admin-pw").await;
    let neris = neris_list(&t, &admin).await;
    let key = format!("pokemon_wishlist:{neris}");
    // the list's own card, nobody owns it
    t.post_as(
        &admin,
        &format!("/pokemon/wishlists/{neris}/add"),
        &[
            ("card", "sv06.5-010"),
            ("catalogue", "intl"),
            ("priority", "2"),
        ],
    )
    .await;
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_card").await, 2);
    let users_page = t.get_as(&admin, "/users").await.1;
    assert!(users_page.contains("wishlist “Neri”") || users_page.contains("wishlist &#8220;Neri"));
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM app_user_permission WHERE permission = '{key}'"
        ))
        .await,
        1
    );

    let (status, _) = t
        .post_as(&admin, &format!("/pokemon/wishlists/{neris}/delete"), &[])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_wishlist").await, 1);
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_wish").await, 0);
    // both cards were only wished: no owner left
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_card").await, 0);
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM app_permission WHERE key = '{key}'"
        ))
        .await,
        0
    );
    assert_eq!(
        t.scalar(&format!(
            "SELECT count(*) FROM app_user_permission WHERE permission = '{key}'"
        ))
        .await,
        0
    );
    // the page no longer offers it, and neri's grant is gone at once
    let neri = t.login("neri", "neri-pw").await;
    assert_eq!(
        t.get_as(&neri, &format!("/pokemon/wishlists/{neris}/add"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert!(!t.get_as(&admin, "/users").await.1.contains("Neri”"));
}

#[tokio::test]
async fn a_list_needs_a_free_name_and_can_be_renamed() {
    let t = app().await;
    let admin = t.login("admin", "admin-pw").await;
    for name in ["", "ortie", "  "] {
        let (status, _) = t
            .post_as(&admin, "/pokemon/wishlists/add", &[("name", name)])
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{name:?}");
    }
    let (status, _) = t
        .post_as(&admin, "/pokemon/wishlists/1/edit", &[("name", "Lise")])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.text("SELECT name FROM pokemon_wishlist WHERE id = 1")
            .await
            .as_deref(),
        Some("Lise")
    );
    // its permission's label follows
    assert!(t.get_as(&admin, "/users").await.1.contains("Lise"));
}

#[tokio::test]
async fn wishes_got_before_the_migration_keep_their_date() {
    // 0030's backfill, run again on rows shaped like the old ones
    let t = TestApp::new().await;
    fixture(&t).await;
    t.exec(
        "INSERT INTO pokemon_copy (id, card_id, language, variant, source, obtained_on, added_on)
             VALUES (1, 1, 'fr', 'holo', 'single', '2026-09-01', '2026-09-02');
         INSERT INTO pokemon_wish (id, wishlist_id, card_id, priority, added_on, got_copy_id)
             VALUES (1, 1, 1, 1, '2026-08-01', 1);",
    )
    .await;
    let migration = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../migrations/0030_users_and_wishlists.sql"
    ))
    .unwrap();
    let backfill = &migration[migration.find("UPDATE pokemon_wish SET got_on").unwrap()..];
    let backfill = &backfill[..backfill.find(';').unwrap() + 1];
    t.exec(backfill).await;
    assert_eq!(
        t.text("SELECT got_on FROM pokemon_wish WHERE id = 1")
            .await
            .as_deref(),
        Some("2026-09-01")
    );
}
