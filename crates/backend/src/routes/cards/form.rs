//! Card edition (Django: admin only). Cards are never deleted: they are the
//! history of the collection.
//!
//! When the English name changes, the card is unlinked from its card data and
//! relinked in the background, like a new card.
//!
//! The Cardmarket product (its printing, for its market price) is guessed by
//! the price refresh (`store::prices::cardmarket`); choosing one here keeps it.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use yugioh_domain::card::{normalize_card_name, CardStatus};
use yugioh_domain::rarity::Rarity;

use crate::store::card_data::spawn_link_cards;
use crate::store::db::begin_write;
use crate::store::prices::cardmarket::{candidates, Candidate};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// Card fields as typed.
#[derive(Debug, Default, sqlx::FromRow)]
pub struct CardValues {
    name: String,
    en_name: String,
    code: String,
    status: String,
    rarity: String,
    is_proxy: bool,
}

#[derive(Template)]
#[template(path = "cards/edit.html")]
pub struct EditCardPage {
    id: i64,
    errors: Vec<String>,
    card: CardValues,
    /// The products the card can be linked to (named like its card data).
    products: Vec<Candidate>,
    /// The product chosen by hand (its id), `""` for the refresh's guess.
    product: String,
    /// The refresh's guess, shown with the "automatic" choice.
    guess: Option<i64>,
}

/// The card's link to its Cardmarket product.
#[derive(sqlx::FromRow)]
struct ProductLink {
    card_data_id: Option<i64>,
    cardmarket_product_id: Option<i64>,
    cardmarket_product_manual: bool,
}

async fn product_link(state: &AppState, id: i64) -> AppResult<ProductLink> {
    sqlx::query_as(
        "SELECT card_data_id, cardmarket_product_id, cardmarket_product_manual
         FROM card WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()
}

async fn product_candidates(state: &AppState, link: &ProductLink) -> AppResult<Vec<Candidate>> {
    match link.card_data_id {
        Some(card_data_id) => Ok(candidates(&state.pool, card_data_id).await?),
        None => Ok(Vec::new()),
    }
}

impl EditCardPage {
    fn statuses(&self) -> [CardStatus; 5] {
        CardStatus::ALL
    }

    fn rarities(&self) -> [Rarity; 34] {
        Rarity::ALL
    }

    fn guess_label(&self) -> String {
        match self.guess {
            Some(id) => format!("Automatic (now {id})"),
            None => "Automatic (none found yet)".to_string(),
        }
    }
}

pub async fn page(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<EditCardPage> {
    let card = sqlx::query_as(
        "SELECT name, coalesce(en_name, '') AS en_name, coalesce(code, '') AS code,
            coalesce(last_known_status, '') AS status, coalesce(rarity, '') AS rarity, is_proxy
         FROM card WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .or_404()?;

    let link = product_link(&state, id).await?;
    Ok(EditCardPage {
        id,
        errors: Vec::new(),
        card,
        products: product_candidates(&state, &link).await?,
        product: match (link.cardmarket_product_manual, link.cardmarket_product_id) {
            (true, Some(product)) => product.to_string(),
            _ => String::new(),
        },
        guess: link
            .cardmarket_product_id
            .filter(|_| !link.cardmarket_product_manual),
    })
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Name");
    let status = match form.get("status") {
        None => None,
        Some(code) => {
            let status = CardStatus::from_code(code);
            if status.is_none() {
                form.errors.push("Invalid status.".into());
            }
            status
        }
    };

    let rarity = match form.get("rarity") {
        None => None,
        Some(label) => {
            let rarity = Rarity::from_label(label);
            if rarity.is_none() {
                form.errors.push("Unknown rarity.".into());
            }
            rarity
        }
    };

    let link = product_link(&state, id).await?;
    let products = product_candidates(&state, &link).await?;
    let product: Option<i64> = match form.get("cardmarket_product") {
        None => None,
        Some(value) => {
            let product = value
                .parse()
                .ok()
                .filter(|p| products.iter().any(|c| c.id == *p));
            if product.is_none() {
                form.errors.push("Unknown Cardmarket product.".into());
            }
            product
        }
    };

    let (Some(name), true) = (name, form.errors.is_empty()) else {
        return Ok(EditCardPage {
            id,
            card: CardValues {
                name: form.text("name"),
                en_name: form.text("en_name"),
                code: form.text("code"),
                status: form.text("status"),
                rarity: form.text("rarity"),
                is_proxy: form.checked("is_proxy"),
            },
            product: form.text("cardmarket_product"),
            errors: form.errors,
            products,
            guess: link
                .cardmarket_product_id
                .filter(|_| !link.cardmarket_product_manual),
        }
        .into_response());
    };

    let en_name = normalize_card_name(form.get("en_name").unwrap_or(&name));

    let mut tx = begin_write(&state.pool).await?;
    let old_en_name: Option<String> =
        sqlx::query_scalar::<_, Option<String>>("SELECT en_name FROM card WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .or_404()?;
    let renamed = old_en_name.as_deref() != Some(en_name.as_str());

    // a product chosen by hand is kept; back to automatic, the current guess
    // stays until the next refresh
    let (product_id, manual) = match product {
        Some(product) => (Some(product), true),
        None if link.cardmarket_product_manual => (None, false),
        None => (link.cardmarket_product_id, false),
    };
    sqlx::query(
        "UPDATE card SET name = ?, en_name = ?, code = ?, last_known_status = ?, rarity = ?,
            is_proxy = ?,
            card_data_id = CASE WHEN ? THEN NULL ELSE card_data_id END,
            cardmarket_product_id = CASE WHEN ? THEN NULL ELSE ? END,
            cardmarket_product_manual = ?
         WHERE id = ?",
    )
    .bind(&name)
    .bind(&en_name)
    .bind(form.get("code"))
    .bind(status.map(yugioh_domain::card::CardStatus::code))
    .bind(rarity.map(yugioh_domain::rarity::Rarity::label))
    .bind(form.checked("is_proxy"))
    .bind(renamed)
    .bind(renamed)
    .bind(product_id)
    .bind(manual && !renamed)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(card_id = id, relinked = renamed, "card edited");

    if renamed {
        spawn_link_cards(state, vec![id]);
    }
    Ok(Redirect::to(&format!("/cards/{id}")).into_response())
}
