//! Adding one copy (a single, a gift, a graded card: boosters have
//! `booster.rs`) and editing a copy, its sale included (behind the login).
//!
//! The card is chosen with the card finder (`partials/pokemon_finder.html`,
//! `static/js/pokemon_finder.js`): it fills the `card` field with the
//! TCGdex id and shows the card's rarity and printed variants to choose
//! from. The copy's language gives the catalogue (`ja`: the Japanese
//! cards). Copies are never deleted: a mistake is fixed here, a copy that
//! leaves the collection is marked sold.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use domain::card::CardStatus;
use domain::pokemon::{parse_language, Catalogue, Source, Variant, LANGUAGES};
use serde::Deserialize;

use super::CopyRow;
use crate::store::db::{begin_write, today, Pool};
use crate::store::pokemon::cards::{self, CopyFields};
use crate::store::pokemon::openings;
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// A copy's fields as typed, to fill the form again.
#[derive(Debug, Default)]
pub struct CopyValues {
    pub card: String,
    pub language: String,
    pub variant: String,
    pub condition: String,
    pub full_art: bool,
    pub source: String,
    pub obtained_on: String,
    pub price: String,
    pub grader: String,
    pub grade: String,
    pub cert_number: String,
    pub estimate: String,
    pub sold_on: String,
    pub sold_price: String,
    pub note: String,
    /// Edit page only: the card's name typed by hand (a Japanese Trainer).
    pub name_manual: String,
    /// The booster opening's id, or empty.
    pub opening: String,
    /// The openings to choose from: `(id, label)`.
    pub openings: Vec<(i64, String)>,
}

impl CopyValues {
    fn from_form(form: &FormData) -> CopyValues {
        CopyValues {
            card: form.text("card"),
            language: form.text("language"),
            variant: form.text("variant"),
            condition: form.text("condition"),
            full_art: form.checked("full_art"),
            source: form.text("source"),
            obtained_on: form.text("obtained_on"),
            price: form.text("price"),
            grader: form.text("grader"),
            grade: form.text("grade"),
            cert_number: form.text("cert_number"),
            estimate: form.text("estimate"),
            sold_on: form.text("sold_on"),
            sold_price: form.text("sold_price"),
            note: form.text("note"),
            name_manual: form.text("name_manual"),
            opening: form.text("opening"),
            openings: Vec::new(),
        }
    }

    /// The openings to choose from, loaded.
    async fn with_openings(mut self, pool: &Pool) -> AppResult<CopyValues> {
        self.openings = openings::all(pool)
            .await?
            .into_iter()
            .map(|o| (o.id, o.label()))
            .collect();
        Ok(self)
    }

    pub fn opening_is(&self, id: &i64) -> bool {
        self.opening == id.to_string()
    }

    pub fn languages(&self) -> [(&'static str, &'static str); 7] {
        LANGUAGES
    }

    pub fn language_is(&self, code: &str) -> bool {
        self.language == code
    }

    /// A copy has one language ("Any" is for wishes).
    pub fn any_language(&self) -> bool {
        false
    }

    /// A language typed that the list doesn't offer.
    pub fn other_language(&self) -> &str {
        if LANGUAGES.iter().any(|(code, _)| *code == self.language) {
            ""
        } else {
            &self.language
        }
    }

    pub fn variants(&self) -> [Variant; 5] {
        Variant::ALL
    }

    pub fn variant_is(&self, variant: &Variant) -> bool {
        self.variant == variant.code()
    }

    pub fn conditions(&self) -> [CardStatus; 5] {
        CardStatus::ALL
    }

    pub fn condition_is(&self, status: &CardStatus) -> bool {
        self.condition == status.code()
    }

    pub fn sources(&self) -> [Source; 5] {
        Source::ALL
    }

    pub fn source_is(&self, source: &Source) -> bool {
        self.source == source.code()
    }

    pub fn has_grading(&self) -> bool {
        !(self.grader.is_empty() && self.grade.is_empty() && self.estimate.is_empty())
    }
}

/// Validate the copy fields but the card (its id comes after the TCGdex
/// lookup); errors are added to `form.errors`. Returns the TCGdex id and
/// the fields with `card_id` 0.
fn read_copy(form: &mut FormData) -> Option<(String, CopyFields)> {
    let card = form.required("card", "Card (find it above)");
    let language = match form.get("language").or(form.get("other_language")) {
        None => {
            form.errors.push("Language is required.".into());
            None
        }
        Some(text) => {
            let language = parse_language(text);
            if language.is_none() {
                form.errors.push(format!(
                    "Language: \"{text}\" is not a language code (fr, en, ja…)."
                ));
            }
            language
        }
    };
    let variant = match form.get("variant").map(Variant::from_code) {
        Some(Some(variant)) => Some(variant),
        Some(None) => {
            form.errors.push("Variant: invalid choice.".into());
            None
        }
        None => {
            form.errors.push("Variant is required.".into());
            None
        }
    };
    let condition = match form.get("condition") {
        None => None,
        Some(code) => {
            let status = CardStatus::from_code(code);
            if status.is_none() {
                form.errors.push("Condition: invalid choice.".into());
            }
            status
        }
    };
    let source = match form.get("source").map(Source::from_code) {
        Some(Some(source)) => Some(source),
        _ => {
            form.errors.push("Obtained from: invalid choice.".into());
            None
        }
    };
    let obtained_on = form.date("obtained_on", "Obtained on");
    let price = form.decimal("price", "Price paid");
    let grader = form.get("grader").map(str::to_string);
    let grade = form.get("grade").map(str::to_string);
    if grade.is_some() && grader.is_none() {
        form.errors
            .push("Grade: give the grading company too (PSA, CGC…).".into());
    }
    let estimate = form.decimal("estimate", "Estimate");
    let opening_id = form.id("opening", "Booster opening");
    let sold_on = form.date("sold_on", "Sold on");
    let sold_price = form.decimal("sold_price", "Sold for");
    if sold_price.is_some() && form.get("sold_on").is_none() {
        form.errors
            .push("Sold for: give the date it was sold.".into());
    }

    if !form.errors.is_empty() {
        return None;
    }

    Some((
        card?,
        CopyFields {
            card_id: 0,
            language: language?,
            variant: variant?,
            condition,
            full_art: form.checked("full_art"),
            source: source?,
            obtained_on,
            price,
            grader,
            grade,
            cert_number: form.get("cert_number").map(str::to_string),
            estimate,
            sold_on,
            sold_price,
            note: form.get("note").map(str::to_string),
            opening_id,
        },
    ))
}

/// An error when the opening chosen doesn't exist (any more).
async fn check_opening(pool: &Pool, copy: &CopyFields, errors: &mut Vec<String>) -> AppResult<()> {
    if let Some(id) = copy.opening_id {
        if openings::get(pool, id).await?.is_none() {
            errors.push("Booster opening: invalid choice.".into());
        }
    }
    Ok(())
}

/// The `pokemon_card` id of the card typed, asking TCGdex when needed; a
/// form error when it knows no such card (or doesn't answer).
pub async fn find_card(
    state: &AppState,
    catalogue: Catalogue,
    tcgdex_id: &str,
    errors: &mut Vec<String>,
) -> AppResult<Option<i64>> {
    match cards::ensure(&state.pool, &state.http, catalogue, tcgdex_id).await {
        Ok(Some(id)) => Ok(Some(id)),
        Ok(None) => {
            let hint = match catalogue {
                Catalogue::Japanese => "Japanese cards have their own ids (SV6a-010)",
                Catalogue::International => {
                    "a Japanese card needs the language Japanese; others use ids like sv06.5-067"
                }
            };
            errors.push(format!(
                "Card: TCGdex knows no card \"{tcgdex_id}\" ({hint})."
            ));
            Ok(None)
        }
        Err(AppError::Internal(why)) => {
            tracing::warn!("{why}");
            errors.push("Card: TCGdex did not answer. Try again in a moment.".into());
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

// ---------- add ----------

/// A copy just added, with the wishes it fulfils.
pub struct Added {
    pub copy: CopyRow,
    /// `(wish id, copy id)`.
    pub wishes: Vec<(i64, i64)>,
}

/// The copies just added and the wishes they fulfil (shown after a save,
/// with "Mark got" buttons).
pub async fn added(pool: &Pool, ids: &[i64]) -> AppResult<Vec<Added>> {
    let mut added = Vec::new();
    let mut conn = pool.acquire().await?;
    for id in ids {
        let copy: Option<CopyRow> = sqlx::query_as(&format!("{} WHERE pc.id = ?", CopyRow::SELECT))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
        let Some(copy) = copy else {
            continue;
        };
        let key = domain::pokemon::wish::CopyKey {
            card_id: copy.card_id,
            language: copy.language.clone(),
            variant: copy.variant_value().unwrap_or(Variant::Normal),
        };
        let wishes = cards::wishes_fulfilled(&mut conn, &[(copy.id, key)]).await?;
        added.push(Added { copy, wishes });
    }
    Ok(added)
}

#[derive(Template)]
#[template(path = "pokemon/add.html")]
pub struct AddCopyPage {
    errors: Vec<String>,
    values: CopyValues,
    added: Vec<Added>,
}

/// `?added=<id>` after a save; `?card=…&language=…&variant=…` from a wish;
/// `?opening=<id>` from an opening's page.
#[derive(Debug, Deserialize)]
pub struct AddParams {
    added: Option<i64>,
    card: Option<String>,
    language: Option<String>,
    variant: Option<String>,
    opening: Option<i64>,
}

/// `GET /pokemon/add`.
pub async fn add_page(
    State(state): State<AppState>,
    Query(params): Query<AddParams>,
) -> AppResult<AddCopyPage> {
    let mut values = CopyValues {
        card: params.card.unwrap_or_default(),
        language: params.language.unwrap_or_else(|| "fr".into()),
        variant: params.variant.unwrap_or_default(),
        source: Source::Single.code().into(),
        obtained_on: today().to_string(),
        // a card added is near mint unless said otherwise
        condition: CardStatus::NearMint.code().into(),
        ..CopyValues::default()
    };
    if let Some(opening) = match params.opening {
        Some(id) => openings::get(&state.pool, id).await?,
        None => None,
    } {
        values.opening = opening.id.to_string();
        values.language = opening.language;
        values.source = Source::Booster.code().into();
        values.obtained_on = opening.opened_on.unwrap_or_default();
    }
    let values = values.with_openings(&state.pool).await?;
    let added = match params.added {
        Some(id) => added(&state.pool, &[id]).await?,
        None => Vec::new(),
    };
    Ok(AddCopyPage {
        errors: Vec::new(),
        values,
        added,
    })
}

/// `POST /pokemon/add`: save, then the empty form again for the next card.
pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let copy = read_copy(&mut form);
    let mut copy = match copy {
        Some((tcgdex_id, mut copy)) => {
            let catalogue = Catalogue::of_language(&copy.language);
            match find_card(&state, catalogue, &tcgdex_id, &mut form.errors).await? {
                Some(card_id) => {
                    copy.card_id = card_id;
                    Some(copy)
                }
                None => None,
            }
        }
        None => None,
    };

    if let Some(copy) = &copy {
        check_opening(&state.pool, copy, &mut form.errors).await?;
    }
    let Some(copy) = copy.take().filter(|_| form.errors.is_empty()) else {
        let page = AddCopyPage {
            values: CopyValues::from_form(&form)
                .with_openings(&state.pool)
                .await?,
            errors: form.errors,
            added: Vec::new(),
        };
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let id = cards::insert_copy(&mut tx, &copy).await?;
    tx.commit().await?;
    tracing::info!(copy_id = id, "Pokémon copy added");

    let to = match copy.opening_id {
        Some(opening) => format!("/pokemon/openings/{opening}?added={id}"),
        None => format!("/pokemon/add?added={id}"),
    };
    Ok(Redirect::to(&to).into_response())
}

// ---------- edit ----------

#[derive(Template)]
#[template(path = "pokemon/edit.html")]
pub struct EditCopyPage {
    id: i64,
    errors: Vec<String>,
    values: CopyValues,
    /// The card is Japanese: its name can be typed by hand.
    japanese: bool,
}

fn stored_values(copy: &CopyRow) -> CopyValues {
    let text = |value: &Option<String>| value.clone().unwrap_or_default();
    let money = |value: &Option<crate::store::db::Money>| {
        value
            .map(|m| m.0.normalize().to_string())
            .unwrap_or_default()
    };
    CopyValues {
        card: copy.tcgdex_id.clone(),
        language: copy.language.clone(),
        variant: copy.variant.clone(),
        condition: text(&copy.condition),
        full_art: copy.full_art,
        source: copy.source.clone(),
        obtained_on: text(&copy.obtained_on),
        price: money(&copy.price),
        grader: text(&copy.grader),
        grade: text(&copy.grade),
        cert_number: text(&copy.cert_number),
        estimate: money(&copy.estimate),
        sold_on: text(&copy.sold_on),
        sold_price: money(&copy.sold_price),
        note: text(&copy.note),
        name_manual: text(&copy.name_manual),
        opening: copy.opening_id.map(|id| id.to_string()).unwrap_or_default(),
        openings: Vec::new(),
    }
}

async fn stored(pool: &Pool, id: i64) -> AppResult<CopyRow> {
    sqlx::query_as(&format!("{} WHERE pc.id = ?", CopyRow::SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()
}

/// `GET /pokemon/:id/edit`.
pub async fn edit_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<EditCopyPage> {
    let copy = stored(&state.pool, id).await?;
    Ok(EditCopyPage {
        id,
        errors: Vec::new(),
        values: stored_values(&copy).with_openings(&state.pool).await?,
        japanese: copy.catalogue == "ja",
    })
}

/// `POST /pokemon/:id/edit`.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let before = stored(&state.pool, id).await?;
    let mut form = FormData::new(pairs);
    let mut copy = None;
    if let Some((tcgdex_id, mut fields)) = read_copy(&mut form) {
        let catalogue = Catalogue::of_language(&fields.language);
        if let Some(card_id) = find_card(&state, catalogue, &tcgdex_id, &mut form.errors).await? {
            fields.card_id = card_id;
            copy = Some(fields);
        }
    }

    if let Some(copy) = &copy {
        check_opening(&state.pool, copy, &mut form.errors).await?;
    }
    let Some(copy) = copy.filter(|_| form.errors.is_empty()) else {
        let page = EditCopyPage {
            id,
            values: CopyValues::from_form(&form)
                .with_openings(&state.pool)
                .await?,
            errors: form.errors,
            japanese: before.catalogue == "ja",
        };
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    if !cards::update_copy(&mut tx, id, &copy).await? {
        return Err(AppError::NotFound);
    }
    sqlx::query("UPDATE pokemon_card SET name_manual = ? WHERE id = ?")
        .bind(form.get("name_manual"))
        .bind(copy.card_id)
        .execute(&mut *tx)
        .await?;
    cards::delete_orphans(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(copy_id = id, "Pokémon copy edited");

    Ok(Redirect::to(&format!("/pokemon/{id}")).into_response())
}
