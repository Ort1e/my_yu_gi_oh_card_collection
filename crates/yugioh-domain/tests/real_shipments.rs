//! Checks the Cardmarket parser against the real shipment PDFs of the `data`
//! submodule, using the lots they were imported into (`data/app.db`) as expected
//! values. Ignored by default (needs the private data):
//!
//!   cargo test -p yugioh-domain --test real_shipments -- --ignored
//!
//! The PDFs contain personal data: this test only reports counts, lot ids
//! and rarity codes (`UR`…), never extracted text or names.

use std::collections::BTreeMap;
use std::path::Path;

use yugioh_domain::cardmarket::parse_shipment_pdf;
use yugioh_domain::lot::LotType;
use rust_decimal::Decimal;
use sqlx::Row;

const ACCOUNT: &str = "0rtie";
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

#[tokio::test]
#[ignore]
async fn real_shipments_match_their_lots() {
    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{ROOT}/data/app.db?mode=ro"))
        .await
        .expect("data/app.db");
    let lots = sqlx::query(
        "SELECT l.id, l.lot_type, l.price, l.buy_date, l.shipment_file,
                (SELECT count(*) FROM unite u WHERE u.lot_id = l.id) AS unites
         FROM lot l WHERE l.shipment_file IS NOT NULL ORDER BY l.id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let (mut parsed, mut total_ok, mut type_ok, mut date_ok, mut cards_ok) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    // rarity code -> (cards, recognised)
    let mut rarity_codes: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    let mut without_rarity = 0;
    for lot in &lots {
        let id: i64 = lot.get("id");
        let file: String = lot.get("shipment_file");
        let Ok(bytes) = std::fs::read(Path::new(ROOT).join("data/media").join(&file)) else {
            failures.push(format!("lot {id}: file missing"));
            continue;
        };
        let shipment = match parse_shipment_pdf(&bytes, ACCOUNT) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("lot {id}: {e}"));
                continue;
            }
        };
        parsed += 1;
        for card in &shipment.cards {
            if let Some(code) = &card.rarity_code {
                let entry = rarity_codes.entry(code.clone()).or_insert((
                    0,
                    yugioh_domain::rarity::Rarity::from_cardmarket_code(code).is_some(),
                ));
                entry.0 += 1;
            }
            if card.rarity.is_none() {
                without_rarity += 1;
            }
        }

        let price: Decimal = lot.get::<String, _>("price").parse().unwrap();
        let lot_type = LotType::from_code(lot.get("lot_type")).unwrap();
        let buy_date: String = lot.get("buy_date");
        let unites: i64 = lot.get("unites");

        let checks = [
            (shipment.total == price, &mut total_ok, "total"),
            (shipment.lot_type == lot_type, &mut type_ok, "type"),
            (
                shipment.paid.map(|d| d.to_string()).as_deref() == Some(buy_date.as_str()),
                &mut date_ok,
                "date",
            ),
            (
                shipment.cards.len() as i64 == unites,
                &mut cards_ok,
                "cards",
            ),
        ];
        let mut wrong = Vec::new();
        for (ok, counter, name) in checks {
            if ok {
                *counter += 1;
            } else {
                wrong.push(name);
            }
        }
        if !wrong.is_empty() {
            let days = shipment
                .paid
                .map(|d| (d - buy_date.parse::<chrono::NaiveDate>().unwrap()).num_days());
            failures.push(format!(
                "lot {id} ({}): mismatch {wrong:?} — total diff {}, paid date diff {days:?} days, cards {} vs {unites}",
                lot_type.code(),
                shipment.total - price,
                shipment.cards.len()
            ));
        }
    }

    let n = lots.len();
    eprintln!("lots with a PDF: {n}, parsed: {parsed}");
    eprintln!("total ok: {total_ok}, type ok: {type_ok}, paid date ok: {date_ok}, card count ok: {cards_ok}");
    for f in &failures {
        eprintln!("  {f}");
    }
    eprintln!("rarity codes (cards, recognised): {rarity_codes:?}");
    eprintln!("cards without a rarity: {without_rarity}");
    // Totals, dates and card counts can legitimately differ: some lots were
    // edited by hand after import, and some PDFs have no paid/arrived date.
    assert_eq!(parsed, n, "some PDFs could not be parsed");
    assert_eq!(type_ok, n, "purchase/sale detection failed");
    assert!(cards_ok * 100 >= n * 95, "card parsing regressed");
}
