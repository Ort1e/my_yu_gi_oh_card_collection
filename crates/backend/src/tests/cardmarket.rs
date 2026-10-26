//! Cardmarket's price guide (`store::prices::cardmarket`): links, prices, the value
//! page and the card edit form. Synthetic catalogue, no network.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rust_decimal_macros::dec;
use yugioh_domain::cardmarket_catalog::{Product, ProductPrices};

use super::*;
use crate::store::card_data::SetInfo;
use crate::store::prices::cardmarket::{apply, Report};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn product(id: i64, name: &str, expansion: i64, added: NaiveDate) -> Product {
    Product {
        id,
        name: name.into(),
        expansion,
        added,
    }
}

/// Dark Magician in LOB (expansion 100, product 500) and in its OCG twin
/// (expansion 200, added a year before, product 501), Kuriboh (502), and a
/// card we don't have (503).
fn catalogue() -> (Vec<Product>, HashMap<i64, ProductPrices>) {
    let products = vec![
        product(500, "Dark Magician", 100, date(2002, 3, 1)),
        product(501, "Dark Magician", 200, date(2001, 1, 1)),
        product(502, "Kuriboh", 300, date(2002, 3, 1)),
        product(503, "Raigeki", 100, date(2002, 3, 1)),
    ];
    let price = |trend| ProductPrices {
        trend: Some(trend),
        avg30: None,
        low: Some(dec!(1.00)),
    };
    let prices = HashMap::from([
        (500, price(dec!(12.50))),
        (501, price(dec!(3.00))),
        (502, price(dec!(0.20))),
    ]);
    (products, prices)
}

fn lob() -> HashMap<String, SetInfo> {
    HashMap::from([(
        "LOB".to_string(),
        SetInfo {
            names: HashSet::from(["dark magician".to_string()]),
            release: Some(date(2002, 3, 8)),
        },
    )])
}

async fn refresh(t: &TestApp, today: NaiveDate) -> Report {
    let (products, prices) = catalogue();
    apply(&t.state.pool, today, &products, &prices, &lob(), false)
        .await
        .unwrap()
}

#[tokio::test]
async fn links_the_copies_to_their_printing() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;

    let report = refresh(&t, date(2026, 10, 19)).await;
    assert_eq!(report.products, 3, "Raigeki isn't one of our cards");
    assert_eq!((report.sets_checked, report.sets_found), (1, 1));
    // the 2 Dark Magicians (LOB-FR005) to the LOB product, not the OCG one;
    // the Kuriboh proxy has no code
    assert_eq!((report.linked, report.unlinked), (2, 1));
    assert_eq!(report.prices, 1);
    assert_eq!(
        t.scalar(&format!(
            "SELECT cardmarket_product_id FROM card WHERE id = {}",
            cards[0]
        ))
        .await,
        500
    );
    assert_eq!(
        t.scalar("SELECT id_expansion FROM cardmarket_expansion WHERE set_code = 'LOB'")
            .await,
        100
    );
    assert_eq!(
        t.text("SELECT price FROM cardmarket_price WHERE product_id = 500")
            .await
            .as_deref(),
        Some("12.50")
    );
}

#[tokio::test]
async fn a_product_chosen_by_hand_is_kept() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;
    refresh(&t, date(2026, 10, 12)).await;
    sqlx::query(
        "UPDATE card SET cardmarket_product_id = 501, cardmarket_product_manual = 1 WHERE id = ?",
    )
    .bind(cards[0])
    .execute(&t.state.pool)
    .await
    .unwrap();

    refresh(&t, date(2026, 10, 19)).await;
    let linked: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, cardmarket_product_id FROM card WHERE is_proxy = 0 ORDER BY id")
            .fetch_all(&t.state.pool)
            .await
            .unwrap();
    assert_eq!(linked, vec![(cards[0], 501), (cards[1], 500)]);
    // both products have their price history
    assert_eq!(
        t.scalar("SELECT count(*) FROM cardmarket_price WHERE date = '2026-10-19'")
            .await,
        2
    );
}

#[tokio::test]
async fn copies_are_valued_at_their_printings_price() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;
    sqlx::query("INSERT INTO card_price (card_data_id, date, cardmarket_price) VALUES (1, '2026-10-19', '0.41')")
        .execute(&t.state.pool)
        .await
        .unwrap();
    refresh(&t, date(2026, 10, 19)).await;

    // 2 French NM copies at 12.50 × 0.9, not at ygoprodeck's 0.41
    let (status, body) = t.get("/collection/value").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("22.50 €"), "{body}");

    let (_, card) = t.get(&format!("/cards/{}", cards[0])).await;
    assert!(card.contains("Market (Cardmarket):"), "{card}");
    assert!(card.contains("12.50 € on 2026-10-19"), "{card}");
}

#[tokio::test]
async fn the_edit_form_picks_the_product() {
    let t = TestApp::new().await;
    let (_, cards) = t.deck_fixture().await;
    refresh(&t, date(2026, 10, 19)).await;
    let uri = format!("/cards/{}/edit", cards[0]);
    let link = || async {
        sqlx::query_as::<_, (Option<i64>, bool)>(
            "SELECT cardmarket_product_id, cardmarket_product_manual FROM card WHERE id = ?",
        )
        .bind(cards[0])
        .fetch_one(&t.state.pool)
        .await
        .unwrap()
    };

    let (status, page) = t.get(&uri).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Automatic (now 500)"), "{page}");
    assert!(
        page.contains("501 · expansion 200 · added 2001-01-01 · trend 3.00 €"),
        "{page}"
    );
    assert!(page.contains("500 · expansion 100 (LOB)"), "{page}");

    let fields = |product: &'static str| {
        [
            ("name", "Magicien Sombre"),
            ("en_name", "Dark Magician"),
            ("code", "LOB-FR005"),
            ("status", "NM"),
            ("cardmarket_product", product),
        ]
    };
    let (status, _) = t.post(&uri, &fields("501")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(link().await, (Some(501), true));

    // a product of another card: refused
    let (status, page) = t.post(&uri, &fields("502")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Unknown Cardmarket product."), "{page}");

    // back to automatic: the next refresh guesses again
    t.post(&uri, &fields("")).await;
    assert_eq!(link().await, (None, false));
    refresh(&t, date(2026, 10, 26)).await;
    assert_eq!(link().await, (Some(500), false));
}
