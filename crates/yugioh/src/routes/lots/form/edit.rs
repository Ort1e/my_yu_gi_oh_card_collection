//! Lot edition: its fields and its unites' prices.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;

use super::{sellers, validate_lot, LotValues};
use crate::store::db::{begin_write, exists, Money};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// A unite of the edited lot, its price as typed.
#[derive(Debug, sqlx::FromRow)]
pub struct UniteValues {
    card_id: i64,
    card_name: String,
    price: String,
}

#[derive(Template)]
#[template(path = "lots/edit.html")]
pub struct EditLotPage {
    id: i64,
    errors: Vec<String>,
    lot: LotValues,
    is_cancelled: bool,
    seller_select: String,
    unites: Vec<UniteValues>,
    sellers: Vec<(i64, String)>,
}

impl EditLotPage {
    fn lot_type_is(&self, code: &str) -> bool {
        self.lot.lot_type == code
    }

    fn seller_is(&self, id: &i64) -> bool {
        self.seller_select == id.to_string()
    }
}

async fn lot_unites(state: &AppState, lot_id: i64) -> sqlx::Result<Vec<UniteValues>> {
    sqlx::query_as(
        "SELECT unite.card_id, card.name AS card_name, coalesce(unite.price, '') AS price
         FROM unite JOIN card ON card.id = unite.card_id
         WHERE unite.lot_id = ? ORDER BY unite.id",
    )
    .bind(lot_id)
    .fetch_all(&state.pool)
    .await
}

#[derive(sqlx::FromRow)]
struct StoredLot {
    lot_type: String,
    price: Money,
    buy_date: String,
    received_date: Option<String>,
    no_card_price: Money,
    refund: Money,
    is_cancelled: bool,
    seller_id: Option<i64>,
}

pub async fn edit_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<EditLotPage> {
    let lot: StoredLot = sqlx::query_as(
        "SELECT lot_type, price, buy_date, received_date, no_card_price, refund, is_cancelled,
                seller_id
         FROM lot WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()?;

    Ok(EditLotPage {
        id,
        errors: Vec::new(),
        lot: LotValues {
            lot_type: lot.lot_type,
            price: lot.price.0.to_string(),
            buy_date: lot.buy_date,
            received_date: lot.received_date.unwrap_or_default(),
            no_card_price: lot.no_card_price.0.to_string(),
            refund: lot.refund.0.to_string(),
        },
        is_cancelled: lot.is_cancelled,
        seller_select: lot.seller_id.map(|id| id.to_string()).unwrap_or_default(),
        unites: lot_unites(&state, id).await?,
        sellers: sellers(&state).await?,
    })
}

/// Name of the price field of the unite of `card_id`.
fn unite_price_field(card_id: i64) -> String {
    format!("unite-{card_id}-price")
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let lot = validate_lot(&mut form);
    let seller_id = form.id("seller_select", "Seller");

    let mut unites = lot_unites(&state, id).await?;
    let mut prices = Vec::with_capacity(unites.len());
    for unite in &mut unites {
        let field = unite_price_field(unite.card_id);
        let label = format!("Price of {}", unite.card_name);
        prices.push((unite.card_id, form.decimal(&field, &label)));
        unite.price = form.text(&field);
    }

    let (Some(lot), true) = (lot, form.errors.is_empty()) else {
        return Ok(EditLotPage {
            id,
            lot: LotValues::from_form(&form),
            is_cancelled: form.checked("is_cancelled"),
            seller_select: form.text("seller_select"),
            unites,
            sellers: sellers(&state).await?,
            errors: form.errors,
        }
        .into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    if let Some(seller_id) = seller_id {
        exists(&mut tx, "seller", seller_id).await?;
    }
    let updated = sqlx::query(
        "UPDATE lot SET lot_type = ?, price = ?, buy_date = ?, received_date = ?,
            no_card_price = ?, refund = ?, is_cancelled = ?, seller_id = ?
         WHERE id = ?",
    )
    .bind(lot.lot_type.code())
    .bind(Money(lot.price))
    .bind(lot.buy_date.to_string())
    .bind(lot.received_date.map(|d| d.to_string()))
    .bind(Money(lot.no_card_price))
    .bind(Money(lot.refund))
    .bind(form.checked("is_cancelled"))
    .bind(seller_id)
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    for (card_id, price) in prices {
        sqlx::query("UPDATE unite SET price = ? WHERE lot_id = ? AND card_id = ?")
            .bind(price.map(Money))
            .bind(id)
            .bind(card_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    tracing::info!(lot_id = id, "lot edited");

    Ok(Redirect::to(&format!("/lots/{id}")).into_response())
}
