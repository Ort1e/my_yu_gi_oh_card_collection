//! Lot creation (Django: `AddLotView`, `LotDetailView.post`) and edition
//! (Django: admin only). Lots and their unites are never deleted.
//!
//! - [`add`]: the "add lot" page, the shipment PDF upload that pre-fills it,
//!   and the creation;
//! - [`edit`]: the lot's fields and its unites' prices;
//! - [`cards`]: cards added to an existing lot (from its page).
//!
//! The lot fields shared by both forms are here.

mod add;
mod cards;
mod edit;

pub use add::{create, page, upload};
pub use cards::add_cards;
pub use edit::{edit_page, update};

use yugioh_domain::lot::LotType;

use crate::web::forms::FormData;
use crate::AppState;

/// Lot fields as typed.
#[derive(Debug, Clone)]
pub struct LotValues {
    pub(super) lot_type: String,
    pub(super) price: String,
    pub(super) buy_date: String,
    pub(super) received_date: String,
    pub(super) no_card_price: String,
}

impl Default for LotValues {
    fn default() -> Self {
        LotValues {
            lot_type: LotType::Purchase.code().to_string(),
            price: String::new(),
            buy_date: String::new(),
            received_date: String::new(),
            no_card_price: "0.00".to_string(),
        }
    }
}

impl LotValues {
    pub(super) fn from_form(form: &FormData) -> LotValues {
        LotValues {
            lot_type: form.text("lot_type"),
            price: form.text("price"),
            buy_date: form.text("buy_date"),
            received_date: form.text("received_date"),
            no_card_price: form.text("no_card_price"),
        }
    }
}

/// Validated lot fields.
pub(super) struct ValidLot {
    pub(super) lot_type: LotType,
    pub(super) price: rust_decimal::Decimal,
    pub(super) buy_date: chrono::NaiveDate,
    pub(super) received_date: Option<chrono::NaiveDate>,
    pub(super) no_card_price: rust_decimal::Decimal,
}

/// Validate the lot fields. Errors go to `form.errors`: callers must also
/// check it is empty (an invalid optional field still gives `Some`).
pub(super) fn validate_lot(form: &mut FormData) -> Option<ValidLot> {
    let lot_type = LotType::from_code(form.get("lot_type").unwrap_or_default());
    if lot_type.is_none() {
        form.errors.push("Lot type is required.".into());
    }
    let price = form.required_decimal("price", "Price");
    let buy_date = form.required_date("buy_date", "Buy date");
    let received_date = form.date("received_date", "Received date");
    let no_card_price = form.decimal("no_card_price", "No card price");
    Some(ValidLot {
        lot_type: lot_type?,
        price: price?,
        buy_date: buy_date?,
        received_date,
        no_card_price: no_card_price.unwrap_or_default(),
    })
}

/// Every seller, by name (the forms' seller list).
pub(super) async fn sellers(state: &AppState) -> sqlx::Result<Vec<(i64, String)>> {
    sqlx::query_as("SELECT id, name FROM seller ORDER BY name COLLATE NOCASE")
        .fetch_all(&state.pool)
        .await
}
