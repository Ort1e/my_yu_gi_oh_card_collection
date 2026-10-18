//! Lot creation (Django: `AddLotView`, `LotDetailView.post`) and edition
//! (Django: admin only). Lots and their unites are never deleted.
//!
//! A Cardmarket shipment PDF can be uploaded first: it is parsed in memory to
//! pre-fill the form and kept as a *pending* file (`crate::store::shipment_files`)
//! until the lot is saved. The PDF text (personal data) is never stored or
//! logged.

use askama::Template;
use axum::extract::{Multipart, Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use domain::cardmarket::{parse_shipment_pdf, Shipment};
use domain::lot::LotType;
use sqlx::SqliteConnection;

use crate::store::card_data::spawn_link_cards;
use crate::store::cards::insert_lot_cards;
use crate::store::db::{begin_write, exists, Money};
use crate::store::shipment_files::{self, pending_path};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{CardRowValues, FormData, Pairs};
use crate::AppState;

/// Lot fields as typed.
#[derive(Debug, Clone)]
pub struct LotValues {
    lot_type: String,
    price: String,
    buy_date: String,
    received_date: String,
    no_card_price: String,
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
    fn from_form(form: &FormData) -> LotValues {
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
struct ValidLot {
    lot_type: LotType,
    price: rust_decimal::Decimal,
    buy_date: chrono::NaiveDate,
    received_date: Option<chrono::NaiveDate>,
    no_card_price: rust_decimal::Decimal,
}

/// Validate the lot fields. Errors go to `form.errors`: callers must also
/// check it is empty (an invalid optional field still gives `Some`).
fn validate_lot(form: &mut FormData) -> Option<ValidLot> {
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

/// New-seller fields as typed.
#[derive(Debug, Clone, Default)]
pub struct SellerValues {
    name: String,
    is_person: bool,
    source: String,
}

#[derive(Template)]
#[template(path = "lots/add.html")]
pub struct AddLotPage {
    errors: Vec<String>,
    upload_error: Option<String>,
    /// Token of the uploaded shipment PDF waiting for this lot.
    pending: Option<String>,
    lot: LotValues,
    seller_select: String,
    new_seller: SellerValues,
    rows: Vec<CardRowValues>,
    sellers: Vec<(i64, String)>,
    sources: Vec<(i64, String)>,
}

impl AddLotPage {
    async fn new(state: &AppState) -> AppResult<AddLotPage> {
        Ok(AddLotPage {
            errors: Vec::new(),
            upload_error: None,
            pending: None,
            lot: LotValues::default(),
            seller_select: String::new(),
            new_seller: SellerValues::default(),
            rows: vec![CardRowValues::default()],
            sellers: sellers(state).await?,
            sources: sqlx::query_as("SELECT id, name FROM seller_source ORDER BY name")
                .fetch_all(&state.pool)
                .await?,
        })
    }

    fn blank_row(&self) -> CardRowValues {
        CardRowValues::default()
    }

    fn statuses(&self) -> [domain::card::CardStatus; 5] {
        domain::card::CardStatus::ALL
    }

    fn lot_type_is(&self, code: &str) -> bool {
        self.lot.lot_type == code
    }

    fn seller_is(&self, id: &i64) -> bool {
        self.seller_select == id.to_string()
    }

    fn source_is(&self, id: &i64) -> bool {
        self.new_seller.source == id.to_string()
    }
}

pub async fn page(State(state): State<AppState>) -> AppResult<AddLotPage> {
    AddLotPage::new(&state).await
}

// ---------- shipment upload ----------

pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<AddLotPage> {
    let mut page = AddLotPage::new(&state).await?;

    let mut pdf = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Internal(format!("upload: {e}")))?
    {
        if field.name() == Some("shipment_file") {
            pdf = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::Internal(format!("upload: {e}")))?,
            );
        }
    }
    let Some(pdf) = pdf.filter(|b| !b.is_empty()) else {
        page.upload_error = Some("Choose a shipment PDF to upload.".into());
        return Ok(page);
    };

    let shipment = match parse_shipment_pdf(&pdf, &state.cardmarket_account) {
        Ok(s) => s,
        Err(e) => {
            // `e` never contains PDF text
            tracing::warn!("shipment PDF not parsed: {e}");
            page.upload_error = Some(format!("Failed to process the shipment PDF: {e}."));
            return Ok(page);
        }
    };

    let token = shipment_files::save_pending(&state.uploads_dir, &pdf).await?;

    // counts only: the PDF holds personal data (CLAUDE.md, "Privacy rule")
    tracing::info!(
        lot_type = shipment.lot_type.code(),
        cards = shipment.cards.len(),
        "shipment PDF parsed"
    );
    prefill(&state, &mut page, shipment).await?;
    page.pending = Some(token);
    Ok(page)
}

async fn prefill(state: &AppState, page: &mut AddLotPage, shipment: Shipment) -> AppResult<()> {
    let date = |d: Option<chrono::NaiveDate>| d.map(|d| d.to_string()).unwrap_or_default();
    page.lot = LotValues {
        lot_type: shipment.lot_type.code().to_string(),
        price: shipment.total.to_string(),
        buy_date: date(shipment.paid),
        received_date: date(shipment.arrived),
        no_card_price: "0.00".to_string(),
    };

    if let Some(name) = shipment.counterparty {
        let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM seller WHERE name = ?")
            .bind(&name)
            .fetch_optional(&state.pool)
            .await?;
        match existing {
            Some(id) => page.seller_select = id.to_string(),
            None => {
                let cardmarket: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM seller_source WHERE name = 'Cardmarket'")
                        .fetch_optional(&state.pool)
                        .await?;
                page.new_seller = SellerValues {
                    name,
                    is_person: false,
                    source: cardmarket.map(|id| id.to_string()).unwrap_or_default(),
                };
            }
        }
    }

    // One row per distinct card, with its quantity.
    let mut rows: Vec<(CardRowValues, u32)> = Vec::new();
    for card in shipment.cards {
        let status = card
            .status()
            .map(|s| s.code().to_string())
            .unwrap_or_default();
        let row = CardRowValues {
            name: card.name.clone(),
            en_name: card.name,
            code: card.code,
            status,
            is_proxy: false,
            price: card.price.to_string(),
            quantity: String::new(),
        };
        match rows.iter_mut().find(|(r, _)| {
            (&r.name, &r.code, &r.status, &r.price)
                == (&row.name, &row.code, &row.status, &row.price)
        }) {
            Some((_, n)) => *n += 1,
            None => rows.push((row, 1)),
        }
    }
    if !rows.is_empty() {
        page.rows = rows
            .into_iter()
            .map(|(mut row, n)| {
                row.quantity = n.to_string();
                row
            })
            .collect();
    }
    Ok(())
}

// ---------- lot creation ----------

/// The seller of a new lot: one picked in the list, or a new name.
enum SellerChoice {
    Existing(i64),
    New {
        name: String,
        is_person: bool,
        source: Option<i64>,
    },
}

impl SellerChoice {
    /// The seller's id; a new name that already exists reuses that seller.
    async fn find_or_create(self, conn: &mut SqliteConnection) -> AppResult<i64> {
        match self {
            SellerChoice::Existing(id) => {
                exists(conn, "seller", id).await?;
                Ok(id)
            }
            SellerChoice::New {
                name,
                is_person,
                source,
            } => {
                let existing: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM seller WHERE name = ?")
                        .bind(&name)
                        .fetch_optional(&mut *conn)
                        .await?;
                if let Some(id) = existing {
                    return Ok(id);
                }
                Ok(
                    sqlx::query("INSERT INTO seller (name, is_person, source_id) VALUES (?, ?, ?)")
                        .bind(&name)
                        .bind(is_person)
                        .bind(source)
                        .execute(&mut *conn)
                        .await?
                        .last_insert_rowid(),
                )
            }
        }
    }
}

pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);

    let lot = validate_lot(&mut form);
    let seller_id = form.id("seller_select", "Seller");
    let seller_name = form.get("seller_name").map(str::to_string);
    let seller_source = form.id("seller_source", "Seller source");
    let cards = form.new_cards();
    let seller = match (seller_id, seller_name) {
        (Some(id), _) => Some(SellerChoice::Existing(id)),
        (None, Some(name)) => Some(SellerChoice::New {
            name,
            is_person: form.checked("seller_is_person"),
            source: seller_source,
        }),
        (None, None) => {
            form.errors
                .push("Select a seller or enter a new seller name.".into());
            None
        }
    };
    let pending = form.get("pending").map(str::to_string);
    let pending_file = match &pending {
        Some(token) => match pending_path(&state.uploads_dir, token) {
            Some(path) if path.exists() => Some(path),
            _ => {
                form.errors
                    .push("The uploaded shipment file expired, upload it again.".into());
                None
            }
        },
        None => None,
    };

    let (Some(lot), Some(seller), true) = (lot, seller, form.errors.is_empty()) else {
        let mut page = AddLotPage::new(&state).await?;
        page.lot = LotValues::from_form(&form);
        page.seller_select = form.text("seller_select");
        page.new_seller = SellerValues {
            name: form.text("seller_name"),
            is_person: form.checked("seller_is_person"),
            source: form.text("seller_source"),
        };
        page.rows = form.card_rows();
        if page.rows.is_empty() {
            page.rows.push(CardRowValues::default());
        }
        page.pending = pending.filter(|_| pending_file.is_some());
        page.errors = form.errors;
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;

    let seller_id = seller.find_or_create(&mut tx).await?;

    let lot_id: i64 = sqlx::query(
        "INSERT INTO lot (lot_type, price, buy_date, received_date, seller_id, no_card_price)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(lot.lot_type.code())
    .bind(Money(lot.price))
    .bind(lot.buy_date.to_string())
    .bind(lot.received_date.map(|d| d.to_string()))
    .bind(seller_id)
    .bind(Money(lot.no_card_price))
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    let card_ids = insert_lot_cards(&mut tx, lot_id, &cards).await?;

    // Move the PDF last, right before committing; put it back if that fails.
    let attached = match pending_file {
        Some(from) => {
            let attached = shipment_files::attach(&state.media_dir, lot_id, from).await?;
            let saved = sqlx::query("UPDATE lot SET shipment_file = ? WHERE id = ?")
                .bind(&attached.relative)
                .bind(lot_id)
                .execute(&mut *tx)
                .await;
            if let Err(e) = saved {
                attached.undo().await;
                return Err(e.into());
            }
            Some(attached)
        }
        None => None,
    };
    let with_shipment = attached.is_some();
    if let Err(e) = tx.commit().await {
        if let Some(attached) = attached {
            attached.undo().await;
        }
        return Err(e.into());
    }
    tracing::info!(
        lot_id,
        seller_id,
        cards = card_ids.len(),
        with_shipment,
        "lot created"
    );

    spawn_link_cards(state, card_ids);
    Ok(Redirect::to(&format!("/lots/{lot_id}")).into_response())
}

// ---------- lot edition ----------

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

async fn sellers(state: &AppState) -> sqlx::Result<Vec<(i64, String)>> {
    sqlx::query_as("SELECT id, name FROM seller ORDER BY name COLLATE NOCASE")
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
    is_cancelled: bool,
    seller_id: Option<i64>,
}

pub async fn edit_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<EditLotPage> {
    let lot: StoredLot = sqlx::query_as(
        "SELECT lot_type, price, buy_date, received_date, no_card_price, is_cancelled, seller_id
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
            no_card_price = ?, is_cancelled = ?, seller_id = ?
         WHERE id = ?",
    )
    .bind(lot.lot_type.code())
    .bind(Money(lot.price))
    .bind(lot.buy_date.to_string())
    .bind(lot.received_date.map(|d| d.to_string()))
    .bind(Money(lot.no_card_price))
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

// ---------- add cards to an existing lot ----------

pub async fn add_cards(
    State(state): State<AppState>,
    Path(lot_id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let cards = form.new_cards();
    if cards.is_empty() && form.errors.is_empty() {
        form.errors.push("Card 1: name is required.".into());
    }
    if !form.errors.is_empty() {
        let rows = form.card_rows();
        let page = super::load_detail(&state.pool, lot_id, form.errors, rows).await?;
        return Ok(page.into_response());
    }

    let mut tx = begin_write(&state.pool).await?;
    exists(&mut tx, "lot", lot_id).await?;
    let card_ids = insert_lot_cards(&mut tx, lot_id, &cards).await?;
    tx.commit().await?;
    tracing::info!(lot_id, cards = card_ids.len(), "cards added to lot");

    spawn_link_cards(state, card_ids);
    Ok(Redirect::to(&format!("/lots/{lot_id}")).into_response())
}
