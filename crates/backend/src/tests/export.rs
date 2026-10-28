//! The public export (`export-public`) shows the collections and nothing
//! private (deploy/README.md, "Security").

use super::*;

#[tokio::test]
async fn the_public_copy_holds_nothing_private() {
    let t = TestApp::new().await;
    // the collections, with private things around them
    t.exec(
        "INSERT INTO seller (id, name, is_person) VALUES (1, 'Jeanne Privatename', 1);
         INSERT INTO lot (id, lot_type, price, buy_date, seller_id, no_card_price)
             VALUES (1, 'purchase', '123.45', '2025-06-01', 1, '0.00');
         INSERT INTO card (id, name, en_name, code, rarity, last_known_status, card_data_id, is_proxy)
             VALUES (1, 'Magicien Sombre', 'dark magician', 'LOB-FR005', 'Ultra Rare', 'NM', 1, 0),
                    (2, 'Kuriboh Proxy', 'kuriboh', 'MRD-FR071', NULL, 'NM', 2, 1);
         INSERT INTO unite (price, lot_id, card_id) VALUES ('98.76', 1, 1);
         INSERT INTO deck (id, name) VALUES (1, 'Secret deck');
         INSERT INTO tournament (id, name, date) VALUES (1, 'Secret tournament', '2025-07-01');
         INSERT INTO monthly_budget (id, month, amount) VALUES (1, '2025-06-01', '55.55');
         INSERT INTO pokemon_card (id, catalogue, tcgdex_id, set_id, set_name, local_id, name_fr,
                                   name_en, rarity, category, variants, json, fetched_at)
             VALUES (1, 'intl', 'sv06.5-067', 'sv06.5', 'Fable Nébuleuse', '067', 'Hypotrempe',
                     'Horsea', 'Illustration rare', 'Pokemon', 'holo', '{}', date('now')),
                    (2, 'intl', 'sv06.5-068', 'sv06.5', 'Fable Nébuleuse', '068', 'Hypocéan',
                     'Seadra', 'Common', 'Pokemon', 'normal', '{}', date('now'));
         INSERT INTO pokemon_copy (card_id, language, variant, condition, source, price, note, added_on)
             VALUES (1, 'fr', 'holo', 'NM', 'single', '44.44', 'secret note', '2025-06-01');
         INSERT INTO pokemon_wish (card_id, max_price, priority, note, added_on)
             VALUES (2, '33.33', 1, 'wish note', '2025-06-01');
         INSERT INTO book (id, isbn, title, notes, added_on)
             VALUES (1, '9780553573398', 'Assassin''s Apprentice', 'book note', '2025-06-01');",
    )
    .await;

    // a picture the app has (the copy never downloads one in tests)
    std::fs::create_dir_all(t.state.media_dir.join("card_images")).unwrap();
    std::fs::write(
        t.state.media_dir.join("card_images/dark_magician.jpg"),
        common::photos::test_png(600, 870),
    )
    .unwrap();

    let out = t.state.media_dir.parent().unwrap().join("site");
    let static_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../static"));
    crate::commands::export::export(&t.state, &out, static_dir)
        .await
        .unwrap();

    // the app's pages: the lists, a card's, a copy's, a book's, the statistics
    for page in [
        "index.html",
        "cards.html",
        "cards-view-table.html",
        "cards-1.html",
        "collection-stats.html",
        "rarities.html",
        "pokemon.html",
        "pokemon-1.html",
        "pokemon-wishlist.html",
        "books.html",
        "books-1.html",
        "img/card-image-1.webp",
        "static/css/style.css",
        "static/js/card_preview.js",
        "static/js/card_zoom.js",
        "static/js/back_link.js",
    ] {
        assert!(out.join(page).exists(), "{page} is missing");
    }
    let mut pages = String::new();
    for entry in std::fs::read_dir(&out).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "html") {
            pages += &std::fs::read_to_string(&path).unwrap();
        }
    }
    // shown: the collections, the app's layout
    for shown in [
        "Magicien Sombre",
        "Hypotrempe",
        "Hypocéan",
        "Assassin&#x27;s Apprentice",
        "Collection statistics",
        r#"href="collection-stats.html""#,
        r#"src="img/card-image-1.webp""#,
    ] {
        assert!(pages.contains(shown), "{shown} is missing");
    }
    // a page of the copy links only to the copy's files
    for (i, _) in pages.match_indices("href=\"") {
        let link = &pages[i + 6..];
        let link = &link[..link.find('"').unwrap()];
        let inside = link == "#"
            || link.starts_with("https://")
            || out.join(link.split('#').next().unwrap()).exists();
        assert!(inside, "{link}: not in the copy");
    }
    // never: sellers, prices paid, lots, decks, tournaments, budget, notes,
    // wishes' maximum price, ISBNs, proxies, links to the app
    for private in [
        "Privatename",
        "123.45",
        "98.76",
        "44.44",
        "33.33",
        "55.55",
        "Secret deck",
        "Secret tournament",
        "secret note",
        "wish note",
        "book note",
        "9780553573398",
        "Kuriboh Proxy",
        "/lots",
        "/login",
        "lots-",
        "sellers",
        "Budget",
        "href=\"/",
        "src=\"/",
        "Edit",
        "your max",
    ] {
        assert!(!pages.contains(private), "{private} is in the public copy");
    }

    // a second export replaces the first; a folder of something else is refused
    crate::commands::export::export(&t.state, &out, static_dir)
        .await
        .unwrap();
    let other = t.state.media_dir.parent().unwrap().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("keep.txt"), "mine").unwrap();
    assert!(
        crate::commands::export::export(&t.state, &other, static_dir)
            .await
            .is_err()
    );
    assert!(other.join("keep.txt").exists());
}
