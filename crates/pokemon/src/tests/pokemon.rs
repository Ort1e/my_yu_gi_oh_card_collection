//! The Pokémon pages: the collection, a copy, the add / booster / edit
//! forms, the wishlist, the finder's card JSON and the login. TCGdex is
//! never called: its cards, prices and set are inserted first (fresh, so
//! they are cache hits).

use super::*;

/// Two cards of Shrouded Fable (French set code FNE), their prices, the set.
async fn fixture(t: &TestApp) {
    sqlx::query(
        "INSERT INTO pokemon_card (id, catalogue, tcgdex_id, set_id, set_name, local_id, name_fr,
             name_en, rarity, category, types, dex_id, variants, image, image_fr, json, fetched_at)
         VALUES
             (1, 'intl', 'sv06.5-067', 'sv06.5', 'Fable Nébuleuse', '067', 'Hypotrempe', 'Horsea',
              'Illustration rare', 'Pokemon', 'Water', 116, 'holo',
              'https://assets.tcgdex.net/en/sv/sv06.5/067', NULL, '{}', date('now')),
             (2, 'intl', 'sv06.5-010', 'sv06.5', 'Fable Nébuleuse', '010', 'Hypotrempe', 'Horsea',
              'Common', 'Pokemon', 'Water', 116, 'normal,holo,reverse', NULL, NULL, '{}', date('now'));
         INSERT INTO pokemon_price (card_id, date, prices) VALUES
             (1, '2026-10-01', '{\"holo\":\"30\"}'),
             (1, date('now'), '{\"holo\":\"34.43\"}'),
             (2, date('now'), '{\"normal\":\"0.03\",\"reverse\":\"0.1\"}');
         INSERT INTO pokemon_set (language, id, name, code, official, cards, fetched_at) VALUES
             ('fr', 'sv06.5', 'Fable Nébuleuse', 'FNE', 64,
              '[{\"id\":\"sv06.5-010\",\"localId\":\"010\",\"name\":\"Hypotrempe\"},
                {\"id\":\"sv06.5-067\",\"localId\":\"067\",\"name\":\"Hypotrempe\"}]',
              date('now'));",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();
}

const HORSEA_IR: &[(&str, &str)] = &[
    ("card", "sv06.5-067"),
    ("language", "fr"),
    ("variant", "holo"),
    ("condition", "EX"),
    ("full_art", "1"),
    ("source", "single"),
    ("obtained_on", "2026-10-20"),
    ("price", "29,90"),
    ("note", "From the shop in town"),
];

async fn add(t: &TestApp, form: &[(&str, &str)]) -> i64 {
    let (status, location) = t.post("/pokemon/add", form).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    location
        .strip_prefix("/pokemon/add?added=")
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn pages_render_empty() {
    let t = TestApp::new().await;
    for uri in [
        "/pokemon",
        "/pokemon?view=table",
        "/pokemon/rarities",
        "/pokemon/wishlist",
        "/pokemon/add",
        "/pokemon/add/booster",
        "/pokemon/wishlist/add",
    ] {
        let (status, page) = t.get(uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {page}");
    }
    let (_, page) = t.get("/pokemon").await;
    assert!(page.contains("No card yet"));
    let (_, page) = t.get("/pokemon/add").await;
    assert!(
        page.contains(r#"<option value="NM" selected>Near Mint</option>"#),
        "near mint by default"
    );
    assert_eq!(t.get("/pokemon/9").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_single_is_added_valued_and_shown() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let id = add(&t, HORSEA_IR).await;

    let (_, page) = t.get(&format!("/pokemon/add?added={id}")).await;
    assert!(page.contains("Added:"));
    assert!(page.contains(&format!("<a href=\"/pokemon/{id}\">Hypotrempe</a>")));

    let (status, page) = t.get(&format!("/pokemon/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    for text in [
        "Hypotrempe",
        "Horsea",
        "Fable Nébuleuse · No. 067",
        "/pokemon/rarities?rarity=Illustration+rare",
        "Bought as a single on 2026-10-20 · paid 29.90 €",
        // 34.43 × 0.8 (excellent)
        "Value: 27.54 €",
        "market 34.43 €",
        "From the shop in town",
        "/pokemon/card-image/1/fr",
    ] {
        assert!(page.contains(text), "{text}: {page}");
    }
    assert!(
        page.contains("<td>2026-10-01</td><td>30.00</td>"),
        "price history"
    );

    let (_, list) = t.get("/pokemon").await;
    assert!(
        list.contains("Market value: <strong>27.54 €</strong>"),
        "{list}"
    );
    assert!(list.contains("Full art"));
    let (_, table) = t.get("/pokemon?view=table&type=Water").await;
    assert!(table.contains("<td>27.54</td>"), "{table}");
    let (_, none) = t.get("/pokemon?type=Fire").await;
    assert!(none.contains("No card matches these filters"));
}

#[tokio::test]
async fn wrong_fields_are_explained() {
    let t = TestApp::new().await;
    let (status, page) = t
        .post(
            "/pokemon/add",
            &[
                ("language", "french"),
                ("variant", "shiny"),
                ("source", "single"),
                ("grade", "10"),
                ("sold_price", "5"),
                ("price", "abc"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    for error in [
        "Card (find it above) is required.",
        "Language: &quot;french&quot; is not a language code",
        "Variant: invalid choice.",
        "Grade: give the grading company too",
        "Sold for: give the date it was sold.",
        "Price paid: &quot;abc&quot; is not a valid amount.",
    ] {
        assert!(page.contains(error), "{error}: {page}");
    }
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_copy").await, 0);
}

#[tokio::test]
async fn a_booster_opening_is_previewed_then_saved() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let opening = |pulls| {
        vec![
            ("set", "fne"),
            ("language", "fr"),
            ("obtained_on", "2026-10-22"),
            ("source", "booster"),
            ("pulls", pulls),
            ("step", "preview"),
        ]
    };

    let (_, page) = t.post("/pokemon/add/booster", &opening("10 6x7 abc")).await;
    assert!(page.contains("not a card number: abc"), "{page}");
    let (_, page) = t.post("/pokemon/add/booster", &opening("10 68")).await;
    assert!(page.contains("no number 68 in Fable Nébuleuse"), "{page}");

    let (status, page) = t.post("/pokemon/add/booster", &opening("10r, 67")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Fable Nébuleuse: 2 cards"), "{page}");
    assert!(
        page.contains(r#"<option value="reverse" selected>"#),
        "the r of 10r"
    );
    assert!(page.contains("Secret"), "067/064 is above the set size");
    assert!(
        page.contains(r#"name="rows-1-full_art" value="1" checked"#),
        "an illustration rare"
    );
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_copy").await, 0);

    let (status, location) = t
        .post(
            "/pokemon/add/booster",
            &[
                ("step", "save"),
                ("set", "fne"),
                ("language", "fr"),
                ("obtained_on", "2026-10-22"),
                ("source", "booster"),
                ("rows-0-card", "sv06.5-010"),
                ("rows-0-variant", "reverse"),
                ("rows-0-count", "2"),
                ("rows-0-condition", "NM"),
                ("rows-1-card", "sv06.5-067"),
                ("rows-1-variant", "holo"),
                ("rows-1-count", "1"),
                ("rows-1-full_art", "1"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location, "/pokemon/openings/1?added=1,2,3");
    assert_eq!(
        t.text(
            "SELECT group_concat(card_id || variant || full_art || source || obtained_on, ' ')
             FROM pokemon_copy"
        )
        .await
        .as_deref(),
        Some("2reverse0booster2026-10-22 2reverse0booster2026-10-22 1holo1booster2026-10-22")
    );
    let (_, page) = t.get(&location).await;
    assert!(
        page.contains("Added:") && page.contains("/pokemon/3"),
        "{page}"
    );
    assert_eq!(
        t.text("SELECT set_code || ' ' || set_name || ' ' || opened_on FROM pokemon_opening")
            .await
            .as_deref(),
        Some("fne fne 2026-10-22"),
        "no set_name field posted: the code"
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM pokemon_copy WHERE opening_id = 1")
            .await,
        3
    );

    // 0.1 + 0.1 + 34.43
    let (_, list) = t.get("/pokemon").await;
    assert!(
        list.contains("Market value: <strong>34.63 €</strong>"),
        "{list}"
    );
}

#[tokio::test]
async fn copies_are_sold_not_deleted() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let id = add(&t, HORSEA_IR).await;

    let (_, page) = t.get(&format!("/pokemon/{id}/edit")).await;
    assert!(page.contains(r#"value="sv06.5-067""#) && page.contains(r#"value="29.9""#));

    let mut form = HORSEA_IR.to_vec();
    form.extend([("sold_on", "2026-10-23"), ("sold_price", "40")]);
    let (status, location) = t.post(&format!("/pokemon/{id}/edit"), &form).await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/pokemon/1")
    );

    let (_, page) = t.get("/pokemon/1").await;
    assert!(
        page.contains("<strong>Sold</strong> on 2026-10-23 for 40.00 €"),
        "{page}"
    );
    let (_, list) = t.get("/pokemon").await;
    assert!(
        list.contains("Pokémon cards (0/1)"),
        "sold copies are hidden"
    );
    let (_, list) = t.get("/pokemon?sold=sold").await;
    assert!(list.contains("Pokémon cards (1/1)"));

    assert_eq!(
        t.post("/pokemon/1/delete", &[]).await.0,
        StatusCode::NOT_FOUND,
        "no delete route"
    );
}

#[tokio::test]
async fn the_wishlist_compares_prices_and_is_fulfilled() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let wish = |max: &'static str, variant: &'static str| {
        vec![
            ("card", "sv06.5-067"),
            ("catalogue", "intl"),
            ("language", ""),
            ("variant", variant),
            ("max_price", max),
            ("priority", "1"),
        ]
    };
    let (status, location) = t.post("/pokemon/wishlist/add", &wish("40", "holo")).await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/pokemon/wishlist")
    );
    t.post("/pokemon/wishlist/add", &wish("10", "")).await;

    let (_, page) = t.get("/pokemon/wishlist").await;
    assert!(
        page.contains("1 card is at or under your price today."),
        "{page}"
    );
    assert!(page.contains("Under your price") && page.contains("Over your price"));
    assert!(page.contains("/pokemon/add?card=sv06.5-067&amp;language=fr&amp;variant=holo"));

    // a French holo copy fulfils both (the second wants any variant)
    let copy = add(&t, HORSEA_IR).await;
    let (_, page) = t.get(&format!("/pokemon/add?added={copy}")).await;
    assert_eq!(page.matches("was on your wishlist").count(), 2, "{page}");

    let (status, location) = t
        .post(
            "/pokemon/wishlist/1/got",
            &[("copy_id", "1"), ("back", "/pokemon/1")],
        )
        .await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/pokemon/1")
    );
    let (_, page) = t.get("/pokemon/wishlist").await;
    assert!(page.contains("Wishlist (1)") && page.contains("Wishes got (1)"));
    let (_, page) = t.get("/pokemon/1").await;
    assert!(page.contains("Got from the"));

    // a wish of a card nobody owns: deleting it deletes the card too
    sqlx::query("UPDATE pokemon_wish SET card_id = 2 WHERE id = 2")
        .execute(&t.state.pool)
        .await
        .unwrap();
    let (status, _) = t.post("/pokemon/wishlist/2/delete", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_card").await, 1);
    assert_eq!(
        t.scalar("SELECT count(*) FROM pokemon_price WHERE card_id = 2")
            .await,
        0
    );
}

#[tokio::test]
async fn the_finder_describes_a_card() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let (status, card) = t
        .json(
            "GET",
            "/pokemon/api/card?id=sv06.5-010&lang=en",
            serde_json::json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(card["name"], "Horsea");
    assert_eq!(card["other_name"], "Hypotrempe");
    assert_eq!(card["full_art"], false);
    assert_eq!(card["default_variant"], "normal");
    assert_eq!(card["rarity_href"], "/pokemon/rarities?rarity=Common");
    let variants: Vec<_> = card["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["code"].as_str().unwrap(), v["price"].as_str()))
        .collect();
    assert_eq!(
        variants,
        [
            ("normal", Some("0.03")),
            ("holo", None),
            ("reverse", Some("0.1"))
        ]
    );

    let (status, answer) = t
        .json(
            "GET",
            "/pokemon/api/search?q=067&lang=fr",
            serde_json::json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(answer["error"]
        .as_str()
        .unwrap()
        .contains("Add the set code"));

    let (status, answer) = t
        .json(
            "GET",
            "/pokemon/api/search?q=FNE%20067%2F064&lang=fr",
            serde_json::json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["cards"][0]["id"], "sv06.5-067");
    assert_eq!(answer["cards"][0]["secret"], true);
}

#[tokio::test]
async fn writes_and_lookups_need_the_login() {
    let t = TestApp::with_password(Some("secret")).await;
    fixture(&t).await;
    let (status, location) = t.post("/pokemon/add", HORSEA_IR).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location.starts_with("/login"), "{location}");
    let (status, _) = t
        .json(
            "GET",
            "/pokemon/api/card?id=sv06.5-067",
            serde_json::json!(null),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        t.get("/pokemon").await.0,
        StatusCode::OK,
        "reads are public"
    );
    assert_eq!(t.get("/pokemon/wishlist").await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_refresh_counts_the_cards_to_price() {
    let t = TestApp::new().await;
    fixture(&t).await;
    add(&t, HORSEA_IR).await;
    // one card owned, already priced today: nothing to ask; the other unused
    let report =
        crate::store::prices::refresh(&t.state.pool, &t.state.http, common::db::today(), true)
            .await
            .unwrap();
    assert_eq!((report.asked, report.orphans_deleted), (0, 1));
    assert_eq!(
        t.scalar("SELECT count(*) FROM pokemon_card").await,
        2,
        "a dry run deletes nothing"
    );
}

/// 30th Celebration: two sets share the printed code 30C (the main set, with
/// the Mew cards numbered by a letter, and its Classic Collection).
async fn celebration(t: &TestApp) {
    sqlx::query(
        "INSERT INTO pokemon_card (catalogue, tcgdex_id, set_id, set_name, local_id, name_fr,
             name_en, rarity, category, variants, json, fetched_at)
         VALUES
             ('intl', '30th-001', '30th', '30ᵉ Anniversaire', '001', 'Noeunoeuf', 'Exeggcute',
              'Common', 'Pokemon', 'normal', '{}', date('now')),
             ('intl', '30th-B', '30th', '30ᵉ Anniversaire', 'B', 'Mew', 'Mew',
              'RGB Rare', 'Pokemon', 'holo', '{}', date('now')),
             ('intl', '30th-c-001', '30th-c', 'Collection Classique', '001', 'Dracaufeu',
              'Charizard', NULL, 'Pokemon', 'holo', '{}', date('now')),
             ('intl', '30th-c-008', '30th-c', 'Collection Classique', '008',
              'Pikachu et Zekrom GX', 'Pikachu & Zekrom GX', NULL, 'Pokemon', 'holo', '{}',
              date('now'));
         INSERT INTO pokemon_set (language, id, name, code, official, cards, fetched_at) VALUES
             ('fr', '30th', '30ᵉ Anniversaire', '30C', 128,
              '[{\"id\":\"30th-001\",\"localId\":\"001\",\"name\":\"Noeunoeuf\"},
                {\"id\":\"30th-033\",\"localId\":\"033\",\"name\":\"Pikachu\"},
                {\"id\":\"30th-056\",\"localId\":\"056\",\"name\":\"Zekrom\"},
                {\"id\":\"30th-B\",\"localId\":\"B\",\"name\":\"Mew\"}]', date('now')),
             ('fr', '30th-c', 'Collection Classique', '30C', 30,
              '[{\"id\":\"30th-c-001\",\"localId\":\"001\",\"name\":\"Dracaufeu\"},
                {\"id\":\"30th-c-008\",\"localId\":\"008\",\"name\":\"Pikachu et Zekrom GX\"}]',
              date('now'));",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();
}

async fn search(t: &TestApp, q: &str) -> (StatusCode, serde_json::Value) {
    let q: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
    t.json(
        "GET",
        &format!("/pokemon/api/search?lang=fr&q={q}"),
        serde_json::json!(null),
    )
    .await
}

#[tokio::test]
async fn sets_sharing_a_code_are_told_apart() {
    let t = TestApp::new().await;
    celebration(&t).await;
    // the number alone: both cards, the main set first
    let (status, answer) = search(&t, "30C 001").await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let ids: Vec<_> = answer["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].clone())
        .collect();
    assert_eq!(ids, ["30th-001", "30th-c-001"]);
    assert_eq!(answer["cards"][1]["set_name"], "Collection Classique");

    // the set size printed after it settles it; a letter is a number
    let (_, answer) = search(&t, "30C 001/030").await;
    assert_eq!(answer["cards"].as_array().unwrap().len(), 1);
    assert_eq!(answer["cards"][0]["id"], "30th-c-001");
    let (_, answer) = search(&t, "30c B").await;
    assert_eq!(answer["cards"][0]["id"], "30th-B");

    // the booster form: 1 is in both sets (a choice), B and 1/30 are not
    let (status, page) = t
        .post(
            "/pokemon/add/booster",
            &[
                ("set", "30C"),
                ("language", "fr"),
                ("obtained_on", "2026-10-23"),
                ("source", "booster"),
                ("pulls", "1 B 1/30"),
                ("step", "preview"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!page.contains("class=\"error\""), "{page}");
    assert!(
        page.contains("30ᵉ Anniversaire / Collection Classique: 3 cards"),
        "{page}"
    );
    assert!(page.contains("This number is in 2 sets with this code"));
    assert!(page.contains(
        r#"<option value="30th-c-001">Collection Classique No. 001 · Dracaufeu</option>"#
    ));
    assert!(page.contains(r#"name="rows-1-card" value="30th-B""#));
    assert!(page.contains(r#"name="rows-2-card" value="30th-c-001""#));
}

#[tokio::test]
async fn reprints_are_found_by_name_or_offered() {
    let t = TestApp::new().await;
    celebration(&t).await;

    // a code and a name: that set's cards; & is et
    let (status, answer) = search(&t, "30C Zekrom").await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let ids: Vec<_> = answer["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].clone())
        .collect();
    assert_eq!(ids, ["30th-056", "30th-c-008"]);
    let (_, answer) = search(&t, "30c pikachu & zekrom").await;
    assert_eq!(answer["cards"][0]["id"], "30th-c-008");

    // the reprint's old number (Team Up 33/181): the Classic Collection is offered
    let (status, answer) = search(&t, "30C 33/181").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        answer["note"]
            .as_str()
            .unwrap()
            .contains("keeps the number"),
        "{answer}"
    );
    assert_eq!(answer["cards"].as_array().unwrap().len(), 2);

    // the booster form: 33 is no card of the main set: choose; near mint by default
    let (_, page) = t
        .post(
            "/pokemon/add/booster",
            &[
                ("set", "30C"),
                ("language", "fr"),
                ("obtained_on", "2026-10-23"),
                ("source", "booster"),
                ("pulls", "B 33/181"),
                ("step", "preview"),
            ],
        )
        .await;
    assert!(!page.contains("class=\"error\""), "{page}");
    assert!(
        page.contains("a Classic Collection card keeps the number"),
        "{page}"
    );
    assert!(page.contains(r#"<option value="" selected>— choose the card —</option>"#));
    assert!(page.contains(r#"<option value="NM" selected>Near Mint</option>"#));

    let save = |card: &'static str| {
        vec![
            ("step", "save"),
            ("set", "30C"),
            ("set_name", "30ᵉ Anniversaire / Collection Classique"),
            ("language", "fr"),
            ("obtained_on", "2026-10-23"),
            ("source", "booster"),
            ("rows-0-card", "30th-B"),
            ("rows-0-variant", "holo"),
            ("rows-0-count", "1"),
            ("rows-0-condition", "NM"),
            ("rows-1-card", card),
            ("rows-1-variant", "holo"),
            ("rows-1-count", "1"),
            ("rows-1-condition", "NM"),
        ]
    };
    let (status, page) = t.post("/pokemon/add/booster", &save("")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Card 2: choose which card it is"), "{page}");
    let (status, location) = t.post("/pokemon/add/booster", &save("30th-c-008")).await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/pokemon/openings/1?added=1,2")
    );
    assert_eq!(
        t.text("SELECT group_concat(condition, ' ') FROM pokemon_copy")
            .await
            .as_deref(),
        Some("NM NM")
    );
}

#[tokio::test]
async fn openings_are_listed_and_grow() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let opening = [
        ("step", "save"),
        ("set", "FNE"),
        ("set_name", "Fable Nébuleuse"),
        ("language", "fr"),
        ("obtained_on", "2026-10-22"),
        ("source", "booster"),
        ("rows-0-card", "sv06.5-067"),
        ("rows-0-variant", "holo"),
        ("rows-0-count", "1"),
    ];
    t.post("/pokemon/add/booster", &opening).await;

    let (status, page) = t.get("/pokemon/openings").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Booster openings (1)") && page.contains("Fable Nébuleuse"));
    assert!(
        page.contains(r#"<td class="text">FNE</td>"#),
        "the set code"
    );
    assert!(page.contains("<td>34.43</td>"), "{page}");

    // a forgotten card, by number: the form is the opening's
    let (_, page) = t.get("/pokemon/add/booster?opening=1").await;
    assert!(
        page.contains("Add cards to an opening") && page.contains(r#"name="opening" value="1""#)
    );
    let mut more = opening.to_vec();
    more.extend([("opening", "1")]);
    more[6] = ("rows-0-card", "sv06.5-010");
    more[7] = ("rows-0-variant", "reverse");
    let (status, location) = t.post("/pokemon/add/booster", &more).await;
    assert_eq!(
        (status, location.as_str()),
        (StatusCode::SEE_OTHER, "/pokemon/openings/1?added=2")
    );
    assert_eq!(t.scalar("SELECT count(*) FROM pokemon_opening").await, 1);

    // one card with the finder, into the opening
    let mut single = HORSEA_IR.to_vec();
    single.push(("opening", "1"));
    let (_, location) = t.post("/pokemon/add", &single).await;
    assert_eq!(location, "/pokemon/openings/1?added=3");
    let (_, page) = t.get("/pokemon/openings/1").await;
    assert!(
        page.contains("3 cards") && page.contains("value <strong>"),
        "{page}"
    );
    let (_, page) = t.get("/pokemon/3").await;
    assert!(page.contains("/pokemon/openings/1"));

    // an unknown opening is refused; the date moves its cards' too
    let mut wrong = HORSEA_IR.to_vec();
    wrong.push(("opening", "9"));
    let (_, page) = t.post("/pokemon/add", &wrong).await;
    assert!(page.contains("Booster opening: invalid choice."));
    let (status, _) = t
        .post(
            "/pokemon/openings/1/edit",
            &[("opened_on", "2026-10-21"), ("note", "At the shop")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.scalar("SELECT count(*) FROM pokemon_copy WHERE obtained_on = '2026-10-21'")
            .await,
        3
    );

    // an English booster saved as French: its cards, set code and name follow
    sqlx::query(
        "INSERT INTO pokemon_set (language, id, name, code, official, cards, fetched_at)
         VALUES ('en', 'sv06.5', 'Shrouded Fable', 'SFA', 64, '[]', date('now'))",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();
    let (_, page) = t.get("/pokemon/openings/1/edit").await;
    assert!(
        page.contains(r#"<option value="en" >English</option>"#),
        "{page}"
    );
    assert!(!page.contains("Japanese"), "not the other catalogue");
    let (status, _) = t
        .post(
            "/pokemon/openings/1/edit",
            &[("opened_on", "2026-10-21"), ("language", "en")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.text("SELECT language || ' ' || set_code || ' ' || set_name FROM pokemon_opening")
            .await
            .as_deref(),
        Some("en SFA Shrouded Fable")
    );
    assert_eq!(
        t.scalar("SELECT count(*) FROM pokemon_copy WHERE language = 'en'")
            .await,
        3
    );
    let (_, page) = t
        .post("/pokemon/openings/1/edit", &[("language", "ja")])
        .await;
    assert!(page.contains("Japanese cards are other cards"));

    // the next opening starts in the last one's language
    let (_, page) = t.get("/pokemon/add/booster").await;
    assert!(
        page.contains(r#"<option value="en" selected>English</option>"#),
        "{page}"
    );
}

#[tokio::test]
async fn trainer_galleries_answer_to_their_main_code() {
    let t = TestApp::new().await;
    sqlx::query(
        "INSERT INTO pokemon_card (catalogue, tcgdex_id, set_id, set_name, local_id, name_fr,
             name_en, rarity, category, variants, json, fetched_at)
         VALUES ('intl', 'swsh10tg-TG29', 'swsh10tg', 'Astres Radieux Galerie de Dresseurs',
                 'TG29', 'Sylveroy Cavalier du Froid VMAX', 'Ice Rider Calyrex VMAX',
                 'Secret Rare', 'Pokemon', 'holo', '{}', date('now')),
                -- the booster's pull 1, here so that nothing asks TCGdex
                ('intl', 'swsh10-001', 'swsh10', 'Astres Radieux', '001', 'Mimigal', 'Spinarak',
                 'Common', 'Pokemon', 'normal', '{}', date('now'));
         INSERT INTO pokemon_set (language, id, name, code, official, cards, fetched_at) VALUES
             ('fr', 'swsh10', 'Astres Radieux', 'ASR', 189,
              '[{\"id\":\"swsh10-001\",\"localId\":\"001\",\"name\":\"Mimigal\"}]', date('now')),
             ('fr', 'swsh10tg', 'Astres Radieux Galerie de Dresseurs', 'ASR:TG', 30,
              '[{\"id\":\"swsh10tg-TG29\",\"localId\":\"TG29\",
                 \"name\":\"Sylveroy Cavalier du Froid VMAX\"}]', date('now'));",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();

    let (status, answer) = search(&t, "ASR TG29/TG30").await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["cards"][0]["id"], "swsh10tg-TG29");
    let (_, answer) = search(&t, "asr sylveroy").await;
    assert_eq!(answer["cards"][0]["id"], "swsh10tg-TG29");

    // the booster form too; a number in neither is an error, not the gallery's list
    let opening = |pulls| {
        vec![
            ("set", "ASR"),
            ("language", "fr"),
            ("obtained_on", "2026-10-23"),
            ("source", "booster"),
            ("pulls", pulls),
            ("step", "preview"),
        ]
    };
    let (_, page) = t.post("/pokemon/add/booster", &opening("1 TG29")).await;
    assert!(!page.contains("class=\"error\""), "{page}");
    assert!(
        page.contains(r#"name="rows-1-card" value="swsh10tg-TG29""#),
        "{page}"
    );
    let (_, page) = t.post("/pokemon/add/booster", &opening("300")).await;
    assert!(page.contains("no number 300 in"), "{page}");
}

#[tokio::test]
async fn japanese_codes_with_their_rarity_letters() {
    let t = TestApp::new().await;
    sqlx::query(
        "INSERT INTO pokemon_card (catalogue, tcgdex_id, set_id, set_name, local_id, name_ja,
             name_fr, name_en, category, dex_id, variants, json, fetched_at)
         VALUES ('ja', 'SV3a-071', 'SV3a', 'レイジングサーフ', '071', 'イベルタル', 'Yveltal',
                 'Yveltal', 'Pokemon', 717, 'holo', '{}', date('now'));
         INSERT INTO pokemon_set (language, id, name, code, official, cards, fetched_at)
         VALUES ('ja', 'SV3a', 'レイジングサーフ', 'SV3a', 62,
                 '[{\"id\":\"SV3a-071\",\"localId\":\"071\",\"name\":\"イベルタル\"}]', date('now'));",
    )
    .execute(&t.state.pool)
    .await
    .unwrap();

    for q in ["sv3a 071/062ar", "SV3a 071/062 AR", "SV3a 71"] {
        let q: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
        let (status, answer) = t
            .json(
                "GET",
                &format!("/pokemon/api/search?lang=ja&q={q}"),
                serde_json::json!(null),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{q}: {answer}");
        assert_eq!(answer["cards"][0]["id"], "SV3a-071");
        assert_eq!(answer["catalogue"], "ja");
    }
}

#[tokio::test]
async fn cards_already_owned_are_flagged() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let card = |except: &str| format!("/pokemon/api/card?id=sv06.5-067&lang=fr{except}");
    let (_, answer) = t.json("GET", &card(""), serde_json::json!(null)).await;
    assert_eq!(answer["owned"], serde_json::json!([]), "none yet");

    let id = add(&t, HORSEA_IR).await;
    let (_, answer) = t.json("GET", &card(""), serde_json::json!(null)).await;
    assert_eq!(
        answer["owned"],
        serde_json::json!([{"id": id, "language": "French", "variant": "Holo"}])
    );
    // its own edit page: not a duplicate of itself
    let (_, answer) = t
        .json(
            "GET",
            &card(&format!("&except={id}")),
            serde_json::json!(null),
        )
        .await;
    assert_eq!(answer["owned"], serde_json::json!([]));
    let (_, page) = t.get(&format!("/pokemon/{id}/edit")).await;
    assert!(page.contains(&format!(r#"data-copy-id="{id}""#)));

    // the booster preview tags it; a sold copy is not counted
    let preview = [
        ("set", "FNE"),
        ("language", "fr"),
        ("obtained_on", "2026-10-26"),
        ("source", "booster"),
        ("pulls", "67 10"),
        ("step", "preview"),
    ];
    let (_, page) = t.post("/pokemon/add/booster", &preview).await;
    assert_eq!(page.matches("You have 1").count(), 1, "{page}");
    sqlx::query("UPDATE pokemon_copy SET sold_on = '2026-10-26'")
        .execute(&t.state.pool)
        .await
        .unwrap();
    let (_, page) = t.post("/pokemon/add/booster", &preview).await;
    assert!(!page.contains("You have 1"));
}
