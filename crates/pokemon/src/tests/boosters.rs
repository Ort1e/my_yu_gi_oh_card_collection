//! The wishlist's booster suggestions: Cardmarket's files (synthetic, never
//! downloaded) applied by `store::boosters::apply`, then the page.

use std::collections::HashMap;

use chrono::NaiveDate;
use common_domain::cardmarket::ProductPrices;
use pokemon_domain::booster::Product;
use rust_decimal_macros::dec;

use super::*;
use crate::store::boosters::{self, Report};

/// Five wished cards: two of Surging Sparks, one of Lost Origin's Trainer
/// Gallery (sold in Lost Origin boosters), one of Pitch Black, a promo; and
/// a Surging Sparks card come after the refresh (no product id read yet).
async fn fixture(t: &TestApp) {
    let card = |id: i64, set: &str, set_name: &str, product: &str| {
        format!(
            "({id}, 'intl', '{set}-{id}', '{set}', '{set_name}', '{id}', 'Carte {id}', \
             'Card {id}', 'Pokemon', '', \
             '{{\"pricing\":{{\"cardmarket\":{{\"idProduct\":{product}}}}}}}', date('now'))"
        )
    };
    let cards = [
        card(1, "sv08", "Étincelles Déferlantes", "101"),
        card(2, "sv08", "Étincelles Déferlantes", "102"),
        card(3, "swsh11tg", "Origine Perdue Galerie de Dresseurs", "201"),
        card(4, "me05", "Nuit Noire", "301"),
        card(5, "svp", "Promos SVP", "401"),
        card(6, "sv08", "Étincelles Déferlantes", "null"),
    ];
    sqlx::query(&format!(
        "INSERT INTO pokemon_card (id, catalogue, tcgdex_id, set_id, set_name, local_id, name_fr,
             name_en, category, variants, json, fetched_at) VALUES {};
         INSERT INTO pokemon_wish (card_id, priority, added_on)
             SELECT id, 2, date('now') FROM pokemon_card;",
        cards.join(", ")
    ))
    .execute(&t.state.pool)
    .await
    .unwrap();
}

fn product(id: i64, name: &str, category: &str, expansion: i64) -> Product {
    Product {
        id,
        name: name.into(),
        category: category.into(),
        expansion,
    }
}

async fn refresh(t: &TestApp) -> Report {
    let singles = [
        product(101, "Pikachu ex", "", 5879),
        product(102, "Latias ex", "", 5879),
        product(201, "Giratina V", "", 5093),
        product(301, "Mega Dragonite ex", "", 6569),
        product(401, "Pikachu", "", 5241),
    ];
    let booster = "Pokémon Booster";
    let others = [
        product(1001, "Surging Sparks Booster", booster, 5879),
        product(1002, "Surging Sparks Sleeved Booster", booster, 5879),
        product(2001, "Lost Origin Booster", booster, 5093),
        product(3001, "Pitch Black Booster", booster, 6569),
        product(9001, "Unwished Booster", booster, 1),
    ];
    let price = |trend| ProductPrices {
        trend: Some(trend),
        ..ProductPrices::default()
    };
    let prices = HashMap::from([
        (1001, price(dec!(6.37))),
        (1002, price(dec!(42.79))),
        (2001, price(dec!(11.84))),
        (3001, price(dec!(5.16))),
    ]);
    let date = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
    boosters::apply(&t.state.pool, date, &singles, &others, &prices, false)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_wishlist_suggests_boosters() {
    let t = TestApp::new().await;
    fixture(&t).await;
    let (_, page) = t.get("/pokemon/wishlist").await;
    assert!(!page.contains("Boosters to open"), "nothing refreshed yet");

    let report = refresh(&t).await;
    assert_eq!(
        report,
        Report {
            linked: 5,
            unlinked: 1,
            boosters: 3,
            priced: 3,
        }
    );

    let (status, page) = t.get("/pokemon/wishlist").await;
    assert_eq!(status, StatusCode::OK);
    let section = &page[page.find("Boosters to open").unwrap()..page.find("Cheapest").unwrap()];
    // the card come since counts for its set's booster; the promo for none
    let surging = section.find("Surging Sparks Booster").unwrap();
    let pitch = section.find("Pitch Black Booster").unwrap();
    let lost = section.find("Lost Origin Booster").unwrap();
    assert!(surging < pitch && pitch < lost, "{section}");
    assert!(
        section.contains("Surging Sparks Booster</strong> · 6.37 € · 3 wished cards"),
        "{section}"
    );
    assert!(section.contains("Origine Perdue Galerie de Dresseurs"));
    assert!(!page.contains("Sleeved") && !page.contains("Unwished"));

    let cheapest = &page[page.find("Cheapest").unwrap()..];
    let pitch = cheapest.find("Pitch Black Booster").unwrap();
    let surging = cheapest.find("Surging Sparks Booster").unwrap();
    assert!(pitch < surging, "{cheapest}");
}
