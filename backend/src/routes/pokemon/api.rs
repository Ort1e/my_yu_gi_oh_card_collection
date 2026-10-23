//! The card finder's JSON (`static/js/pokemon_finder.js`), behind the login:
//! both ask TCGdex and write its cache.
//!
//! - `GET /pokemon/api/search?q=…&lang=fr`: the printed code (`FNE 067`)
//!   finds a card through its set (one per set sharing the code, unless
//!   the set size `/030` settles it; a number in no set offers the smaller
//!   sets' cards: Classic Collection reprints keep their old number); a
//!   code and a name (`30C Zekrom`) searches that set's names; anything
//!   else is a name (`&`, `et` and `and` alike). A Japanese search typed
//!   in Latin letters (`Horsea`) finds the Japanese cards of that Pokémon
//!   by its Pokédex number. 200 `{"catalogue", "cards": [{"id", "name",
//!   "local_id", "set_id", "set_name", "thumb", "secret"}], "set",
//!   "note"}`, 400 / 404 / 502 `{"error"}`.
//! - `GET /pokemon/api/card?id=…&lang=fr`: one card, saved in
//!   `pokemon_card`, with what the form needs to choose: rarity (and the
//!   guide's link), printed variants with today's price, full art
//!   suggestion. 404 `{"error"}` when TCGdex knows no such card.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use domain::pokemon::card_ref::{self, CardRef};
use domain::pokemon::{default_variant, is_full_art, parse_language, Catalogue, Variant};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{card_names, image_src, rarities};
use crate::store::pokemon::{cards, prices, sets, tcgdex};
use crate::web::error::{AppError, AppResult};
use crate::AppState;

fn answer(status: StatusCode, error: impl Into<String>) -> Response {
    (status, Json(json!({ "error": error.into() }))).into_response()
}

/// TCGdex down: a 502 the finder shows, not a 500.
fn unavailable(e: AppError) -> AppResult<Response> {
    match e {
        AppError::Internal(why) => {
            tracing::warn!("{why}");
            Ok(answer(
                StatusCode::BAD_GATEWAY,
                "TCGdex did not answer. Try again in a moment.",
            ))
        }
        e => Err(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    q: String,
    lang: Option<String>,
}

pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> AppResult<Response> {
    let language = params
        .lang
        .as_deref()
        .and_then(parse_language)
        .unwrap_or_else(|| "fr".into());
    let q = params.q.trim();
    if q.is_empty() {
        return Ok(answer(
            StatusCode::BAD_REQUEST,
            "Type a name or a code like FNE 067.",
        ));
    }
    let catalogue = Catalogue::of_language(&language);

    let found =
        match card_ref::parse(q) {
            Ok(CardRef {
                set_code: Some(code),
                number,
                total,
            }) => by_code(&state, &language, &code, &number, total.as_deref()).await,
            Ok(CardRef { set_code: None, .. }) => return Ok(answer(
                StatusCode::BAD_REQUEST,
                "Add the set code before the number: FNE 067 (it is printed next to the number).",
            )),
            Err(_) => match q.split_once(' ') {
                Some((code, name)) if card_ref::is_set_code(code) => {
                    in_set(&state, &language, code, name.trim(), q).await
                }
                _ => by_name(&state, &language, q).await,
            },
        };
    let (cards, set, note) = match found {
        Ok(Ok(found)) => found,
        Ok(Err(response)) => return Ok(response),
        Err(e) => return unavailable(e),
    };

    Ok(Json(json!({
        "catalogue": catalogue.code(),
        "cards": cards,
        "set": set,
        "note": note,
    }))
    .into_response())
}

/// The cards, the set (or null), a note for the finder (or null); or the
/// error answer.
type Found = Result<(Vec<Value>, Value, Option<String>), Response>;

fn card_json(set: &sets::SetInfo, card: &sets::SetCard) -> Value {
    json!({
        "id": card.id,
        "name": card.name,
        "local_id": card.local_id,
        "set_id": set.id,
        "set_name": set.name,
        "official": set.official,
        "thumb": card.image.as_ref().map(|i| format!("{i}/low.webp")),
        "secret": set.is_secret(&card.local_id),
    })
}

/// `30C Zekrom`: the cards of the sets with this code whose name has these
/// words; a code TCGdex doesn't know: a name search of the whole text.
async fn in_set(
    state: &AppState,
    language: &str,
    code: &str,
    name: &str,
    q: &str,
) -> AppResult<Found> {
    let found = sets::by_code(&state.pool, &state.http, language, code).await?;
    if found.is_empty() {
        return by_name(state, language, q).await;
    }
    let cards: Vec<Value> = sets::cards_named(&found, name)
        .into_iter()
        .map(|(set, card)| card_json(set, card))
        .collect();
    if cards.is_empty() {
        return Ok(Err(answer(
            StatusCode::NOT_FOUND,
            format!("No card named like \"{name}\" in the sets {code}."),
        )));
    }
    Ok(Ok((cards, Value::Null, None)))
}

/// The card(s) with this printed code: one, or one per set sharing the
/// code when the set size (`/128`) doesn't tell them apart.
async fn by_code(
    state: &AppState,
    language: &str,
    code: &str,
    number: &str,
    total: Option<&str>,
) -> AppResult<Found> {
    let found = sets::by_code(&state.pool, &state.http, language, code).await?;
    let Some(main) = found.first() else {
        return Ok(Err(answer(
            StatusCode::NOT_FOUND,
            format!("No set \"{code}\" in this language: check the code printed on the card."),
        )));
    };
    let cards = sets::cards(&found, number, total);
    let subsets = sets::subsets(&found);
    if cards.is_empty() && !subsets.is_empty() {
        // a reprint keeps the number of the card it reprints: offer them
        let results = subsets
            .iter()
            .flat_map(|set| set.cards.iter().map(move |card| card_json(set, card)))
            .collect();
        let printed = match total {
            Some(total) => format!("{number}/{total}"),
            None => number.to_string(),
        };
        let note = format!(
            "{printed} is not a {} number: a {} card keeps the number of the card it \
             reprints. Pick yours below.",
            main.name, subsets[0].name
        );
        return Ok(Ok((results, Value::Null, Some(note))));
    }
    if cards.is_empty() {
        let names: Vec<String> = found
            .iter()
            .map(|set| format!("{} ({} cards)", set.name, set.cards.len()))
            .collect();
        return Ok(Err(answer(
            StatusCode::NOT_FOUND,
            format!("No number {number} in {}.", names.join(", ")),
        )));
    }

    let results = cards
        .iter()
        .map(|(set, card)| card_json(set, card))
        .collect();
    let set =
        json!({"id": main.id, "name": main.name, "code": main.code, "official": main.official});
    Ok(Ok((results, set, None)))
}

async fn by_name(state: &AppState, language: &str, q: &str) -> AppResult<Found> {
    let mut briefs = if language == "ja" && q.is_ascii() {
        japanese_by_latin_name(state, q).await?
    } else {
        tcgdex::search(&state.http, language, q).await?
    };
    // "Pikachu & Zekrom" is "Pikachu et Zekrom" in French
    for other in other_spellings(q) {
        if !briefs.is_empty() {
            break;
        }
        briefs = tcgdex::search(&state.http, language, &other).await?;
    }
    if briefs.is_empty() {
        return Ok(Err(answer(
            StatusCode::NOT_FOUND,
            format!("No card named like \"{q}\" in this language."),
        )));
    }

    let cards = briefs
        .iter()
        .filter_map(|brief| {
            let id = brief["id"].as_str()?;
            let (set_id, _) = id.rsplit_once('-')?;
            Some(json!({
                "id": id,
                "name": brief["name"],
                "local_id": brief["localId"],
                "set_id": set_id,
                "thumb": brief["image"].as_str().map(|i| format!("{i}/low.webp")),
                "secret": false,
            }))
        })
        .collect();
    Ok(Ok((cards, Value::Null, None)))
}

/// `&`, `et` and `and` written the other ways (TCGdex's names use `&` in
/// English, `et` in French).
fn other_spellings(q: &str) -> Vec<String> {
    let words: Vec<&str> = q.split_whitespace().collect();
    if !words
        .iter()
        .any(|w| matches!(w.to_lowercase().as_str(), "&" | "et" | "and"))
    {
        return Vec::new();
    }
    ["&", "et", "and"]
        .iter()
        .map(|joint| {
            words
                .iter()
                .map(|w| match w.to_lowercase().as_str() {
                    "&" | "et" | "and" => *joint,
                    _ => *w,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|other| other != q)
        .collect()
}

/// The Japanese cards of the Pokémon named `q` in English or French: its
/// Pokédex number from a card of that name, then the Japanese cards with it.
async fn japanese_by_latin_name(state: &AppState, q: &str) -> AppResult<Vec<Value>> {
    for language in ["en", "fr"] {
        let named = tcgdex::search(&state.http, language, q).await?;
        let exact = named
            .iter()
            .find(|c| {
                c["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(q))
            })
            .or(named.first());
        let Some(id) = exact.and_then(|c| c["id"].as_str()) else {
            continue;
        };
        let Some(card) = tcgdex::card(&state.http, language, id).await? else {
            continue;
        };
        if let Some(dex_id) = card["dexId"].as_array().and_then(|d| d.first()?.as_i64()) {
            return tcgdex::species_cards(&state.http, "ja", dex_id).await;
        }
    }
    Ok(Vec::new())
}

#[derive(Debug, Deserialize)]
pub struct CardParams {
    #[serde(default)]
    id: String,
    lang: Option<String>,
    /// For a wish in any language: `intl` or `ja`.
    catalogue: Option<String>,
}

#[derive(sqlx::FromRow)]
struct CardRow {
    id: i64,
    catalogue: String,
    tcgdex_id: String,
    set_name: String,
    local_id: String,
    name_fr: Option<String>,
    name_en: Option<String>,
    name_ja: Option<String>,
    name_manual: Option<String>,
    rarity: Option<String>,
    category: String,
    types: Option<String>,
    variants: String,
    image: Option<String>,
    image_fr: Option<String>,
    image_from: Option<String>,
}

pub async fn card(
    State(state): State<AppState>,
    Query(params): Query<CardParams>,
) -> AppResult<Response> {
    let language = params.lang.as_deref().and_then(parse_language);
    let catalogue = match (&language, params.catalogue.as_deref()) {
        (Some(language), _) => Catalogue::of_language(language),
        (None, Some(code)) => Catalogue::from_code(code).unwrap_or(Catalogue::International),
        (None, None) => Catalogue::International,
    };
    let id = params.id.trim();
    if id.is_empty() {
        return Ok(answer(StatusCode::BAD_REQUEST, "No card id."));
    }

    let card_id = match cards::ensure(&state.pool, &state.http, catalogue, id).await {
        Ok(Some(card_id)) => card_id,
        Ok(None) => {
            return Ok(answer(
                StatusCode::NOT_FOUND,
                format!("TCGdex knows no card \"{id}\" in these cards."),
            ))
        }
        Err(e) => return unavailable(e),
    };
    let card: CardRow = sqlx::query_as(
        "SELECT id, catalogue, tcgdex_id, set_name, local_id, name_fr, name_en, name_ja,
                name_manual, rarity, category, types, variants, image, image_fr, image_from
         FROM pokemon_card WHERE id = ?",
    )
    .bind(card_id)
    .fetch_one(&state.pool)
    .await?;

    let shown = language.unwrap_or_else(|| if card.catalogue == "ja" { "ja" } else { "fr" }.into());
    let (name, other_name) = card_names(
        &shown,
        card.name_fr.as_deref(),
        card.name_en.as_deref(),
        card.name_ja.as_deref(),
        card.name_manual.as_deref(),
    );
    let latest = prices::latest(&state.pool).await?;
    let card_prices = latest.get(&card.id);
    let mut variants = Variant::parse_list(&card.variants);
    let printed_known = !variants.is_empty();
    if !printed_known {
        variants = Variant::ALL.to_vec();
    }
    let variants: Vec<Value> = variants
        .iter()
        .map(|v| {
            json!({
                "code": v.code(),
                "label": v.label(),
                "price": card_prices.and_then(|p| p.get(v)).map(|p| p.to_string()),
            })
        })
        .collect();

    let has_image = super::has_picture(&card.image, &card.image_fr, &card.image_from);
    Ok(Json(json!({
        "card_id": card.id,
        "id": card.tcgdex_id,
        "catalogue": card.catalogue,
        "name": name,
        "other_name": other_name,
        "set_name": card.set_name,
        "local_id": card.local_id,
        "rarity": card.rarity,
        "rarity_href": card.rarity.as_deref().map(rarities::href),
        "full_art": is_full_art(card.rarity.as_deref()),
        "category": card.category,
        "types": card.types,
        "variants": variants,
        "variants_known": printed_known,
        "default_variant": default_variant(&Variant::parse_list(&card.variants)).code(),
        "image": has_image.then(|| image_src(card.id, &shown)),
    }))
    .into_response())
}
