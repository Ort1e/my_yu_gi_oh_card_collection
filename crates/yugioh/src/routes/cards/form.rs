//! Card edition (Django: admin only). Cards are never deleted: they are the
//! history of the collection.
//!
//! When the English name changes, the card is unlinked from its card data and
//! relinked in the background, like a new card.
//!
//! The Cardmarket product (its printing, for its market price) is guessed by
//! the price refresh (`store::prices::cardmarket`); choosing one here keeps it.
//!
//! The picture: automatic (ygoprodeck's, or its printing's from Yugipedia
//! when `find-pictures` found one without doubt), ygoprodeck's, or one of
//! Yugipedia's pictures of its printing (`store::yugipedia`), for this copy or
//! every copy of the same card, code and rarity. The gallery comes from the
//! cache (filled when a card is added, or by `find-pictures`); `?pictures=1`
//! asks Yugipedia. A new code, rarity or name
//! puts an automatic picture back to ygoprodeck's and looks for the new
//! printing's.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use yugioh_domain::card::{normalize_card_name, CardStatus};
use yugioh_domain::rarity::Rarity;

use super::images::offered;
use crate::store::card_data::spawn_link_cards;
use crate::store::db::begin_write;
use crate::store::prices::cardmarket::{candidates, Candidate};
use crate::store::yugipedia::{self, spawn_find_pictures};
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
    pictures: Pictures,
}

/// The picture choice: [`AUTOMATIC`], [`YGOPRODECK`] or a Yugipedia file.
const AUTOMATIC: &str = "";
const YGOPRODECK: &str = "ygoprodeck";

/// The card's picture and those it can choose from.
#[derive(Default)]
pub struct Pictures {
    /// Its card data, for ygoprodeck's picture.
    card_data_id: Option<i64>,
    /// Yugipedia's pictures of its printing.
    offered: Vec<String>,
    /// The choice: [`AUTOMATIC`], [`YGOPRODECK`] or a file.
    chosen: String,
    /// The automatic pass's picture, when it found one.
    automatic: Option<String>,
    /// Why Yugipedia offers nothing, when it doesn't.
    missing: &'static str,
    /// No gallery cached: a link to ask Yugipedia.
    can_ask: bool,
}

/// `?pictures=1`: ask Yugipedia for the card's gallery.
#[derive(Debug, Default, serde::Deserialize)]
pub struct EditQuery {
    pictures: Option<String>,
}

/// What the card's picture is now.
#[derive(sqlx::FromRow)]
struct PictureState {
    code: Option<String>,
    rarity: Option<String>,
    picture: Option<String>,
    picture_manual: bool,
}

async fn picture_state(state: &AppState, id: i64) -> AppResult<PictureState> {
    sqlx::query_as("SELECT code, rarity, picture, picture_manual FROM card WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .or_404()
}

/// The pictures the edit page offers: Yugipedia's gallery of its card data,
/// from the cache, else asked if `ask`.
async fn pictures(
    state: &AppState,
    card_data_id: Option<i64>,
    current: &PictureState,
    ask: bool,
) -> AppResult<Pictures> {
    let mut pictures = Pictures {
        card_data_id,
        chosen: match (current.picture_manual, &current.picture) {
            (false, _) => AUTOMATIC.to_string(),
            (true, None) => YGOPRODECK.to_string(),
            (true, Some(file)) => file.clone(),
        },
        automatic: current.picture.clone().filter(|_| !current.picture_manual),
        ..Pictures::default()
    };
    let Some(card_data_id) = card_data_id else {
        pictures.missing = "The card isn't linked to its card data yet.";
        return Ok(pictures);
    };

    let gallery = match yugipedia::cached(&state.pool, card_data_id, false).await? {
        Some(gallery) => gallery,
        None if !ask => {
            pictures.missing = "Yugipedia's pictures of this card haven't been looked up yet.";
            pictures.can_ask = true;
            return Ok(pictures);
        }
        None => match yugipedia::gallery(&state.pool, &state.http, card_data_id).await {
            Ok(gallery) => gallery,
            Err(e) => {
                tracing::warn!(card_data_id, "Yugipedia gallery: {e}");
                pictures.missing = "Yugipedia couldn't be reached: try again later.";
                pictures.can_ask = true;
                return Ok(pictures);
            }
        },
    };
    let Some(files) = gallery else {
        pictures.missing = "Yugipedia has no gallery for this card.";
        return Ok(pictures);
    };
    pictures.offered = offered(
        &files,
        current.code.as_deref().unwrap_or(""),
        current.rarity.as_deref(),
    );
    if pictures.offered.is_empty() {
        pictures.missing = "Yugipedia has no picture of this printing (code, language and rarity).";
    }
    Ok(pictures)
}

impl Pictures {
    /// What an offered picture is: `UR · 1st edition · EA`.
    fn label(&self, file: &str) -> String {
        let Some(p) = yugioh_domain::printing_picture::parse(file) else {
            return file.to_string();
        };
        let edition = match p.edition {
            Some("1E") => "1st edition",
            Some("UE") => "Unlimited",
            Some("LE") => "Limited",
            _ => "",
        };
        let mut parts = vec![p.language, p.rarity, edition];
        parts.extend(p.extras.iter().copied());
        common::web::tiles::dotted(parts)
    }

    /// The automatic picture's address (its printing's, else ygoprodeck's).
    fn automatic_src(&self) -> Option<String> {
        crate::store::models::picture_url(self.automatic.as_deref(), self.card_data_id)
    }

    fn automatic_label(&self) -> &'static str {
        if self.automatic.is_some() {
            "Automatic (now its printing's, from Yugipedia)"
        } else {
            "Automatic (now ygoprodeck's)"
        }
    }
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

pub async fn page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<EditQuery>,
) -> AppResult<EditCardPage> {
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
    let current = picture_state(&state, id).await?;
    Ok(EditCardPage {
        id,
        errors: Vec::new(),
        card,
        pictures: pictures(
            &state,
            link.card_data_id,
            &current,
            query.pictures.is_some(),
        )
        .await?,
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

    let current = picture_state(&state, id).await?;
    let mut offered = pictures(&state, link.card_data_id, &current, false).await?;
    let picture = form.text("picture");
    if picture != AUTOMATIC && picture != YGOPRODECK && !offered.offered.contains(&picture) {
        form.errors.push("Unknown picture.".into());
    }

    let (Some(name), true) = (name, form.errors.is_empty()) else {
        offered.chosen = picture;
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
            pictures: offered,
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
    // a new printing (or card) makes an automatic picture wrong
    let rarity_label = rarity.map(yugioh_domain::rarity::Rarity::label);
    let reprinted =
        current.code.as_deref() != form.get("code") || current.rarity.as_deref() != rarity_label;
    let (picture, picture_manual) = match picture.as_str() {
        _ if renamed => (None, false),
        AUTOMATIC if current.picture_manual || reprinted => (None, false),
        AUTOMATIC => (current.picture.clone(), false),
        YGOPRODECK => (None, true),
        file => (Some(file.to_string()), true),
    };
    let everywhere = form.checked("picture_everywhere") && !renamed;

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
    .bind(rarity_label)
    .bind(form.checked("is_proxy"))
    .bind(renamed)
    .bind(renamed)
    .bind(product_id)
    .bind(manual && !renamed)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE card SET picture = ?, picture_manual = ? WHERE id = ?")
        .bind(&picture)
        .bind(picture_manual)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let alike = if everywhere {
        sqlx::query(
            "UPDATE card SET picture = ?, picture_manual = ?
             WHERE id <> ? AND card_data_id IS ? AND code IS NOT NULL AND upper(code) = upper(?)
                 AND rarity IS ?",
        )
        .bind(&picture)
        .bind(picture_manual)
        .bind(id)
        .bind(link.card_data_id)
        .bind(form.get("code"))
        .bind(rarity_label)
        .execute(&mut *tx)
        .await?
        .rows_affected()
    } else {
        0
    };
    tx.commit().await?;
    tracing::info!(card_id = id, relinked = renamed, alike, "card edited");

    if renamed {
        spawn_link_cards(state, vec![id]);
    } else if picture.is_none() && !picture_manual {
        spawn_find_pictures(state, vec![id]);
    }
    Ok(Redirect::to(&format!("/cards/{id}")).into_response())
}
