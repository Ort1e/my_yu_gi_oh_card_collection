//! The "add lot" page: typed in, or pre-filled from a Cardmarket shipment
//! PDF. The PDF is parsed in memory and kept as a *pending* file
//! (`crate::store::shipment_files`) until the lot is saved; its text
//! (personal data) is never stored or logged.

use std::path::PathBuf;

use askama::Template;
use axum::extract::{Multipart, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use sqlx::{Sqlite, SqliteConnection, Transaction};
use yugioh_domain::cardmarket::{parse_shipment_pdf, Shipment};

use super::{sellers, validate_lot, LotValues};
use crate::store::card_data::spawn_link_cards;
use crate::store::cards::insert_lot_cards;
use crate::store::db::{begin_write, exists, Money};
use crate::store::shipment_files::{self, pending_path, Attached};
use crate::web::card_rows::{CardRowValues, CardRows};
use crate::web::error::{AppError, AppResult};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

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

    /// The form again, as typed, with its errors.
    async fn with_errors(
        state: &AppState,
        form: FormData,
        pending: Option<String>,
    ) -> AppResult<AddLotPage> {
        let mut page = AddLotPage::new(state).await?;
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
        page.pending = pending;
        page.errors = form.errors;
        Ok(page)
    }

    fn blank_row(&self) -> CardRowValues {
        CardRowValues::default()
    }

    fn statuses(&self) -> [yugioh_domain::card::CardStatus; 5] {
        yugioh_domain::card::CardStatus::ALL
    }

    fn rarities(&self) -> [yugioh_domain::rarity::Rarity; 34] {
        yugioh_domain::rarity::Rarity::ALL
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
        if card.rarity.is_none() {
            if let Some(code) = &card.rarity_code {
                // a code, not personal data
                tracing::warn!(code, "unknown Cardmarket rarity code");
            }
        }
        let row = CardRowValues {
            name: card.name.clone(),
            en_name: card.name,
            code: card.code,
            status,
            rarity: card
                .rarity
                .map(|r| r.label().to_string())
                .unwrap_or_default(),
            is_proxy: false,
            price: card.price.to_string(),
            quantity: String::new(),
        };
        match rows.iter_mut().find(|(r, _)| {
            (&r.name, &r.code, &r.status, &r.rarity, &r.price)
                == (&row.name, &row.code, &row.status, &row.rarity, &row.price)
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
    let seller = seller_choice(&mut form);
    let cards = form.new_cards();
    let pending = form.get("pending").map(str::to_string);
    let pending_file = pending
        .as_deref()
        .and_then(|token| pending_shipment(&state, token, &mut form));

    let (Some(lot), Some(seller), true) = (lot, seller, form.errors.is_empty()) else {
        let pending = pending.filter(|_| pending_file.is_some());
        return Ok(AddLotPage::with_errors(&state, form, pending)
            .await?
            .into_response());
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

    // the PDF is moved last, right before committing, and put back on failure
    let attached = match pending_file {
        Some(from) => Some(attach_shipment(&mut tx, &state, lot_id, from).await?),
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

/// The seller chosen in the list, or the new one typed; an error when
/// neither.
fn seller_choice(form: &mut FormData) -> Option<SellerChoice> {
    let seller_id = form.id("seller_select", "Seller");
    let seller_name = form.get("seller_name").map(str::to_string);
    let source = form.id("seller_source", "Seller source");

    match (seller_id, seller_name) {
        (Some(id), _) => Some(SellerChoice::Existing(id)),
        (None, Some(name)) => Some(SellerChoice::New {
            name,
            is_person: form.checked("seller_is_person"),
            source,
        }),
        (None, None) => {
            form.errors
                .push("Select a seller or enter a new seller name.".into());
            None
        }
    }
}

/// The uploaded shipment PDF waiting under `token`; an error when it
/// expired (pending files are deleted after a day).
fn pending_shipment(state: &AppState, token: &str, form: &mut FormData) -> Option<PathBuf> {
    let path = pending_path(&state.uploads_dir, token).filter(|path| path.exists());
    if path.is_none() {
        form.errors
            .push("The uploaded shipment file expired, upload it again.".into());
    }
    path
}

/// Move the pending PDF to the lot's shipment file and record it (in `tx`);
/// the file is put back if recording fails.
async fn attach_shipment(
    tx: &mut Transaction<'static, Sqlite>,
    state: &AppState,
    lot_id: i64,
    from: PathBuf,
) -> AppResult<Attached> {
    let attached = shipment_files::attach(&state.media_dir, lot_id, from).await?;
    let saved = sqlx::query("UPDATE lot SET shipment_file = ? WHERE id = ?")
        .bind(&attached.relative)
        .bind(lot_id)
        .execute(&mut **tx)
        .await;

    match saved {
        Ok(_) => Ok(attached),
        Err(e) => {
            attached.undo().await;
            Err(e.into())
        }
    }
}
