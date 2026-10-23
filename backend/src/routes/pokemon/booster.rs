//! A booster opening: every card pulled, added at once (behind the login).
//!
//! 1. The form: the set (its printed code, `FNE`), the language, the date
//!    and the numbers (`67 45r 12x2`, `domain::pokemon::pulls`).
//! 2. "Find the cards" (`step=preview`): the set's cards from TCGdex
//!    (`store::pokemon::sets`), each number matched; an unknown number
//!    stops here with the list. When sets share the code (`30C`: 30th
//!    Celebration and its Classic Collection), a number found in both
//!    offers both cards (`1/30` picks by the printed set size). Each card is shown with its picture,
//!    rarity (and the guide's link) and its printed variants to choose
//!    from, full art pre-ticked from the rarity.
//! 3. "Save" (`step=save`): the rows as checked, one copy per count.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use domain::card::CardStatus;
use domain::pokemon::pulls::{self, Pull};
use domain::pokemon::{
    default_variant, is_full_art, parse_language, Catalogue, Source, Variant, LANGUAGES,
};
use serde::Deserialize;

use super::form::{added, find_card, Added};
use super::{card_names, image_src, rarities};
use crate::store::db::{begin_write, today};
use crate::store::pokemon::cards::{self, CopyFields};
use crate::store::pokemon::sets;
use crate::web::error::{AppError, AppResult};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

/// The opening's fields as typed.
#[derive(Debug, Default)]
pub struct OpeningValues {
    set: String,
    language: String,
    obtained_on: String,
    pulls: String,
    source: String,
}

impl OpeningValues {
    fn from_form(form: &FormData) -> OpeningValues {
        OpeningValues {
            set: form.text("set"),
            language: form.text("language"),
            obtained_on: form.text("obtained_on"),
            pulls: form.text("pulls"),
            source: form.text("source"),
        }
    }

    fn languages(&self) -> [(&'static str, &'static str); 7] {
        LANGUAGES
    }

    fn language_is(&self, code: &str) -> bool {
        self.language == code
    }

    fn sources(&self) -> [Source; 5] {
        Source::ALL
    }

    fn source_is(&self, source: &Source) -> bool {
        self.source == source.code()
    }
}

/// One card found, to check before saving.
pub struct PreviewRow {
    tcgdex_id: String,
    card_id: i64,
    name: String,
    other_name: String,
    local_id: String,
    rarity: String,
    /// Above the set size (`067/064`): a chase card.
    secret: bool,
    variants: Vec<Variant>,
    variant: Variant,
    full_art: bool,
    count: u32,
    has_image: bool,
    language: String,
    /// The set of this card (shown when several sets share the code).
    set_name: String,
    /// The same number in the other sets with this code: `(TCGdex id, label)`.
    alternatives: Vec<(String, String)>,
}

impl PreviewRow {
    fn image_src(&self) -> String {
        image_src(self.card_id, &self.language)
    }

    fn rarity_href(&self) -> String {
        rarities::href(&self.rarity)
    }

    fn variant_is(&self, variant: &Variant) -> bool {
        self.variant == *variant
    }

    /// Several printed variants: the owner has to look (the guide helps).
    fn has_choice(&self) -> bool {
        self.variants.len() > 1
    }

    /// `30ᵉ Anniversaire No. 001 · Noeunoeuf`.
    fn label(&self) -> String {
        format!("{} No. {} · {}", self.set_name, self.local_id, self.name)
    }
}

#[derive(Template)]
#[template(path = "pokemon/booster.html")]
pub struct BoosterPage {
    errors: Vec<String>,
    values: OpeningValues,
    /// Step 2: the cards found, and the set's name.
    preview: Vec<PreviewRow>,
    set_name: String,
    /// After a save.
    added: Vec<Added>,
}

impl BoosterPage {
    fn copies(&self) -> u32 {
        self.preview.iter().map(|r| r.count).sum()
    }

    fn conditions(&self) -> [CardStatus; 5] {
        CardStatus::ALL
    }
}

#[derive(Debug, Deserialize)]
pub struct BoosterParams {
    /// The copies just added: `12,13,14`.
    added: Option<String>,
}

/// `GET /pokemon/add/booster`.
pub async fn page(
    State(state): State<AppState>,
    Query(params): Query<BoosterParams>,
) -> AppResult<BoosterPage> {
    let ids: Vec<i64> = params
        .added
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|id| id.parse().ok())
        .collect();
    Ok(BoosterPage {
        errors: Vec::new(),
        values: OpeningValues {
            language: "fr".into(),
            obtained_on: today().to_string(),
            source: Source::Booster.code().into(),
            ..OpeningValues::default()
        },
        preview: Vec::new(),
        set_name: String::new(),
        added: added(&state.pool, &ids).await?,
    })
}

/// `POST /pokemon/add/booster`: `step=preview` finds the cards, `step=save`
/// saves the rows.
pub async fn submit(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    if form.get("step") == Some("save") {
        return save(&state, form).await;
    }

    let mut page = BoosterPage {
        errors: Vec::new(),
        values: OpeningValues::from_form(&form),
        preview: Vec::new(),
        set_name: String::new(),
        added: Vec::new(),
    };
    let code = form.required("set", "Set (its code, printed on the cards: FNE)");
    let language = read_language(&mut form);
    form.date("obtained_on", "Opened on");
    let pulls = match pulls::parse(&form.text("pulls")) {
        Ok(pulls) => Some(pulls),
        Err(why) => {
            form.errors.push(format!("Cards: {why}."));
            None
        }
    };
    let (Some(code), Some(language), Some(pulls), true) =
        (code, language, pulls, form.errors.is_empty())
    else {
        page.errors = form.errors;
        return Ok(page.into_response());
    };

    match preview(&state, &code, &language, &pulls, &mut form.errors).await? {
        Some((set_name, rows)) if form.errors.is_empty() => {
            page.set_name = set_name;
            page.preview = rows;
        }
        _ => page.errors = form.errors,
    }
    Ok(page.into_response())
}

fn read_language(form: &mut FormData) -> Option<String> {
    let language = form.get("language").and_then(parse_language);
    if language.is_none() {
        form.errors.push("Language: invalid choice.".into());
    }
    language
}

/// The set's cards for these pulls; errors for unknown numbers.
async fn preview(
    state: &AppState,
    code: &str,
    language: &str,
    pulls: &[Pull],
    errors: &mut Vec<String>,
) -> AppResult<Option<(String, Vec<PreviewRow>)>> {
    let found = match sets::by_code(&state.pool, &state.http, language, code).await {
        Ok(found) if !found.is_empty() => found,
        Ok(_) => {
            errors.push(format!(
                "Set: TCGdex knows no set \"{code}\" in {}. Type the code printed at the \
                 bottom of the cards (FNE, SFA, 30C; Japanese: SV6a).",
                domain::pokemon::language_label(language)
            ));
            return Ok(None);
        }
        Err(AppError::Internal(why)) => {
            tracing::warn!("{why}");
            errors.push("Set: TCGdex did not answer. Try again in a moment.".into());
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    let unknown: Vec<&str> = pulls
        .iter()
        .filter(|p| sets::cards(&found, &p.number, p.total.as_deref()).is_empty())
        .map(|p| p.number.as_str())
        .collect();
    if !unknown.is_empty() {
        let names: Vec<String> = found
            .iter()
            .map(|set| format!("{} ({} cards)", set.name, set.cards.len()))
            .collect();
        errors.push(format!(
            "Cards: no number {} in {}.",
            unknown.join(", "),
            names.join(", ")
        ));
        return Ok(None);
    }

    let catalogue = Catalogue::of_language(language);
    let mut ids: HashMap<String, i64> = HashMap::new();
    let mut rows = Vec::new();
    for pull in pulls {
        let candidates = sets::cards(&found, &pull.number, pull.total.as_deref());
        let (set, card) = candidates[0];
        let card_id = match ids.get(&card.id) {
            Some(id) => *id,
            None => match find_card(state, catalogue, &card.id, errors).await? {
                Some(id) => {
                    ids.insert(card.id.clone(), id);
                    id
                }
                None => return Ok(None),
            },
        };
        let mut row = preview_row(
            state,
            card_id,
            language,
            pull,
            set.is_secret(&card.local_id),
        )
        .await?;
        row.set_name.clone_from(&set.name);
        row.alternatives = candidates[1..]
            .iter()
            .map(|(set, card)| {
                let label = format!("{} No. {} · {}", set.name, card.local_id, card.name);
                (card.id.clone(), label)
            })
            .collect();
        rows.push(row);
    }
    let names: Vec<&str> = found.iter().map(|set| set.name.as_str()).collect();
    Ok(Some((names.join(" / "), rows)))
}

#[derive(sqlx::FromRow)]
struct CardFacts {
    tcgdex_id: String,
    local_id: String,
    name_fr: Option<String>,
    name_en: Option<String>,
    name_ja: Option<String>,
    name_manual: Option<String>,
    rarity: Option<String>,
    variants: String,
    image: Option<String>,
    image_fr: Option<String>,
}

async fn preview_row(
    state: &AppState,
    card_id: i64,
    language: &str,
    pull: &Pull,
    secret: bool,
) -> AppResult<PreviewRow> {
    let card: CardFacts = sqlx::query_as(
        "SELECT tcgdex_id, local_id, name_fr, name_en, name_ja, name_manual, rarity, variants,
                image, image_fr
         FROM pokemon_card WHERE id = ?",
    )
    .bind(card_id)
    .fetch_one(&state.pool)
    .await?;
    let (name, other_name) = card_names(
        language,
        card.name_fr.as_deref(),
        card.name_en.as_deref(),
        card.name_ja.as_deref(),
        card.name_manual.as_deref(),
    );
    let variants = Variant::parse_list(&card.variants);
    Ok(PreviewRow {
        variant: pull.variant.unwrap_or_else(|| default_variant(&variants)),
        full_art: is_full_art(card.rarity.as_deref()),
        tcgdex_id: card.tcgdex_id,
        card_id,
        name,
        other_name,
        local_id: card.local_id,
        rarity: card.rarity.unwrap_or_default(),
        secret,
        variants: if variants.is_empty() {
            Variant::ALL.to_vec()
        } else {
            variants
        },
        count: pull.count,
        has_image: card.image.is_some() || card.image_fr.is_some(),
        set_name: String::new(),
        alternatives: Vec::new(),
        language: language.to_string(),
    })
}

/// Step 3: one copy per count of each row.
async fn save(state: &AppState, mut form: FormData) -> AppResult<Response> {
    let language = read_language(&mut form);
    let obtained_on = form.date("obtained_on", "Opened on");
    let source = form.get("source").and_then(Source::from_code);
    if source.is_none() {
        form.errors.push("Obtained from: invalid choice.".into());
    }

    let mut copies = Vec::new();
    for i in form.indices("rows-") {
        let field = |name: &str| form.get(&format!("rows-{i}-{name}")).map(str::to_string);
        let n = i + 1;
        let Some(tcgdex_id) = field("card") else {
            continue;
        };
        let count = field("count").and_then(|c| c.parse::<u32>().ok());
        let variant = field("variant").as_deref().and_then(Variant::from_code);
        let condition = field("condition");
        let condition = match condition.as_deref() {
            None => Some(None),
            Some(code) => CardStatus::from_code(code).map(Some),
        };
        match (count.filter(|c| (0..=99).contains(c)), variant, condition) {
            (Some(count), Some(variant), Some(condition)) => copies.push((
                tcgdex_id,
                count,
                variant,
                condition,
                form.checked(&format!("rows-{i}-full_art")),
            )),
            _ => form
                .errors
                .push(format!("Card {n}: invalid count, variant or condition.")),
        }
    }
    if copies.iter().all(|(_, count, ..)| *count == 0) {
        form.errors.push("No card to save.".into());
    }
    let (Some(language), Some(source), true) = (language, source, form.errors.is_empty()) else {
        let page = BoosterPage {
            values: OpeningValues::from_form(&form),
            errors: form.errors,
            preview: Vec::new(),
            set_name: String::new(),
            added: Vec::new(),
        };
        return Ok(page.into_response());
    };

    // the cards were looked up at step 2: these are cache hits
    let catalogue = Catalogue::of_language(&language);
    let mut rows = Vec::new();
    for (tcgdex_id, count, variant, condition, full_art) in copies {
        let Some(card_id) = find_card(state, catalogue, &tcgdex_id, &mut form.errors).await? else {
            continue;
        };
        rows.push((card_id, count, variant, condition, full_art));
    }
    if !form.errors.is_empty() {
        let page = BoosterPage {
            values: OpeningValues::from_form(&form),
            errors: form.errors,
            preview: Vec::new(),
            set_name: String::new(),
            added: Vec::new(),
        };
        return Ok(page.into_response());
    }

    let mut tx = begin_write(&state.pool).await?;
    let mut ids = Vec::new();
    for (card_id, count, variant, condition, full_art) in rows {
        let copy = CopyFields {
            card_id,
            language: language.clone(),
            variant,
            condition,
            full_art,
            source,
            obtained_on,
            price: None,
            grader: None,
            grade: None,
            cert_number: None,
            estimate: None,
            sold_on: None,
            sold_price: None,
            note: None,
        };
        for _ in 0..count {
            ids.push(cards::insert_copy(&mut tx, &copy).await?);
        }
    }
    tx.commit().await?;
    tracing::info!(copies = ids.len(), "Pokémon booster opening added");

    let ids: Vec<String> = ids.iter().map(i64::to_string).collect();
    Ok(Redirect::to(&format!("/pokemon/add/booster?added={}", ids.join(","))).into_response())
}
