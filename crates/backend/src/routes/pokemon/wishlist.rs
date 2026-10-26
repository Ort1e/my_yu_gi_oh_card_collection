//! The wishlist: cards wanted, with today's price against the most the
//! owner wants to pay (`domain::pokemon::wish`). Public to read; adding,
//! editing, deleting and "Mark got" behind the login. A wish is got with a
//! copy (`got_copy_id`): the add pages offer it when a new copy fulfils it.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use domain::pokemon::wish::{Verdict, Wish};
use domain::pokemon::{language_label, parse_language, Catalogue, Variant, LANGUAGES};
use rust_decimal::Decimal;

use super::form::find_card;
use super::{card_names, image_src, money, rarities};
use crate::store::db::{begin_write, Money, Pool};
use crate::store::pokemon::cards::{self, WishFields};
use crate::store::pokemon::prices;
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct WishRow {
    id: i64,
    card_id: i64,
    language: Option<String>,
    variant: Option<String>,
    max_price: Option<Money>,
    priority: i64,
    note: Option<String>,
    got_copy_id: Option<i64>,
    catalogue: String,
    tcgdex_id: String,
    set_name: String,
    local_id: String,
    name_fr: Option<String>,
    name_en: Option<String>,
    name_ja: Option<String>,
    name_manual: Option<String>,
    rarity: Option<String>,
    image: Option<String>,
    image_fr: Option<String>,
    image_from: Option<String>,
    #[sqlx(skip)]
    price: Option<Decimal>,
    #[sqlx(skip)]
    verdict: Option<Verdict>,
}

impl WishRow {
    const SELECT: &'static str = "SELECT w.id, w.card_id, w.language, w.variant, w.max_price,
            w.priority, w.note, w.got_copy_id, c.catalogue, c.tcgdex_id,
            c.set_name, c.local_id, c.name_fr, c.name_en, c.name_ja, c.name_manual, c.rarity,
            c.image, c.image_fr, c.image_from
         FROM pokemon_wish w JOIN pokemon_card c ON c.id = w.card_id";

    /// The language its name and picture are shown in.
    fn shown_language(&self) -> &str {
        match (&self.language, self.catalogue.as_str()) {
            (Some(language), _) => language,
            (None, "ja") => "ja",
            (None, _) => "fr",
        }
    }

    fn names(&self) -> (String, String) {
        card_names(
            self.shown_language(),
            self.name_fr.as_deref(),
            self.name_en.as_deref(),
            self.name_ja.as_deref(),
            self.name_manual.as_deref(),
        )
    }

    fn name(&self) -> String {
        self.names().0
    }

    fn other_name(&self) -> String {
        self.names().1
    }

    fn has_image(&self) -> bool {
        super::has_picture(&self.image, &self.image_fr, &self.image_from)
    }

    fn image_src(&self) -> String {
        image_src(self.card_id, self.shown_language())
    }

    fn language_text(&self) -> &str {
        self.language.as_deref().map_or("Any", language_label)
    }

    fn variant_text(&self) -> &str {
        match self.variant.as_deref().and_then(Variant::from_code) {
            Some(variant) => variant.label(),
            None => "Any",
        }
    }

    fn max_text(&self) -> String {
        money(self.max_price.map(|m| m.0))
    }

    fn price_text(&self) -> String {
        money(self.price)
    }

    fn verdict_class(&self) -> &'static str {
        self.verdict.map_or("", Verdict::css_class)
    }

    fn verdict_label(&self) -> &'static str {
        self.verdict.map_or("", Verdict::label)
    }

    fn rarity_text(&self) -> &str {
        self.rarity.as_deref().unwrap_or("")
    }

    fn rarity_href(&self) -> String {
        rarities::href(self.rarity_text())
    }

    fn rule(&self) -> Wish {
        Wish {
            card_id: self.card_id,
            language: self.language.clone(),
            variant: self.variant.as_deref().and_then(Variant::from_code),
            max_price: self.max_price.map(|m| m.0),
        }
    }

    /// `/pokemon/add?…` pre-filled with this card.
    fn got_it_href(&self) -> String {
        let language = self.shown_language().to_string();
        crate::web::pagination::querystring([
            ("card", Some(self.tcgdex_id.as_str())),
            ("language", Some(language.as_str())),
            ("variant", self.variant.as_deref()),
        ])
    }
}

#[derive(Template)]
#[template(path = "pokemon/wishlist.html")]
pub struct WishlistPage {
    wanted: Vec<WishRow>,
    got: Vec<WishRow>,
}

impl WishlistPage {
    fn under_max(&self) -> usize {
        self.wanted
            .iter()
            .filter(|w| w.verdict == Some(Verdict::UnderMax))
            .count()
    }
}

/// `/pokemon/wishlist`.
pub async fn list(State(state): State<AppState>) -> AppResult<WishlistPage> {
    let pool = &state.pool;
    let mut wishes: Vec<WishRow> = sqlx::query_as(&format!(
        "{} ORDER BY w.priority, w.added_on, w.id",
        WishRow::SELECT
    ))
    .fetch_all(pool)
    .await?;
    let prices = prices::latest(pool).await?;
    for wish in &mut wishes {
        let empty = prices::VariantPrices::new();
        let card_prices = prices.get(&wish.card_id).unwrap_or(&empty);
        let rule = wish.rule();
        wish.price = rule.price(card_prices);
        wish.verdict = Some(rule.verdict(wish.price));
    }
    let (got, wanted) = wishes.into_iter().partition(|w| w.got_copy_id.is_some());
    Ok(WishlistPage { wanted, got })
}

// ---------- add, edit ----------

#[derive(Debug, Default)]
pub struct WishValues {
    card: String,
    language: String,
    variant: String,
    max_price: String,
    priority: String,
    note: String,
}

impl WishValues {
    fn from_form(form: &FormData) -> WishValues {
        WishValues {
            card: form.text("card"),
            language: form.text("language"),
            variant: form.text("variant"),
            max_price: form.text("max_price"),
            priority: form.text("priority"),
            note: form.text("note"),
        }
    }

    fn languages(&self) -> [(&'static str, &'static str); 7] {
        LANGUAGES
    }

    fn language_is(&self, code: &str) -> bool {
        self.language == code
    }

    /// "Any language" (of the international cards: Japanese cards are
    /// Japanese only).
    fn any_language(&self) -> bool {
        true
    }

    fn other_language(&self) -> &str {
        ""
    }

    fn variants(&self) -> [Variant; 5] {
        Variant::ALL
    }

    fn variant_is(&self, variant: &Variant) -> bool {
        self.variant == variant.code()
    }
}

#[derive(Template)]
#[template(path = "pokemon/wish_form.html")]
pub struct WishFormPage {
    /// `None`: a new wish.
    id: Option<i64>,
    errors: Vec<String>,
    values: WishValues,
}

impl WishFormPage {
    fn action(&self) -> String {
        match self.id {
            Some(id) => format!("/pokemon/wishlist/{id}/edit"),
            None => "/pokemon/wishlist/add".into(),
        }
    }
}

/// Validate; the TCGdex id and the fields with `card_id` 0.
fn read_wish(form: &mut FormData) -> Option<(String, Catalogue, WishFields)> {
    let card = form.required("card", "Card (find it above)");
    let language = match form.get("language") {
        None => None,
        Some(text) => {
            let language = parse_language(text);
            if language.is_none() {
                form.errors.push("Language: invalid choice.".into());
            }
            language
        }
    };
    let catalogue = match &language {
        Some(language) => Catalogue::of_language(language),
        None => form
            .get("catalogue")
            .and_then(Catalogue::from_code)
            .unwrap_or(Catalogue::International),
    };
    let variant = match form.get("variant") {
        None => None,
        Some(code) => {
            let variant = Variant::from_code(code);
            if variant.is_none() {
                form.errors.push("Variant: invalid choice.".into());
            }
            variant
        }
    };
    let max_price = form.decimal("max_price", "Most I would pay");
    let priority = form.number("priority", "Priority", 1..=3).unwrap_or(2);

    if !form.errors.is_empty() {
        return None;
    }
    Some((
        card?,
        catalogue,
        WishFields {
            card_id: 0,
            language,
            variant,
            max_price,
            priority,
            note: form.get("note").map(str::to_string),
        },
    ))
}

async fn read_and_find(state: &AppState, form: &mut FormData) -> AppResult<Option<WishFields>> {
    let Some((tcgdex_id, catalogue, mut wish)) = read_wish(form) else {
        return Ok(None);
    };
    match find_card(state, catalogue, &tcgdex_id, &mut form.errors).await? {
        Some(card_id) => {
            wish.card_id = card_id;
            Ok(Some(wish))
        }
        None => Ok(None),
    }
}

/// `GET /pokemon/wishlist/add`.
pub async fn add_page() -> WishFormPage {
    WishFormPage {
        id: None,
        errors: Vec::new(),
        values: WishValues {
            priority: "2".into(),
            ..WishValues::default()
        },
    }
}

/// `POST /pokemon/wishlist/add`.
pub async fn create(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let Some(wish) = read_and_find(&state, &mut form)
        .await?
        .filter(|_| form.errors.is_empty())
    else {
        let page = WishFormPage {
            id: None,
            values: WishValues::from_form(&form),
            errors: form.errors,
        };
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let id = cards::insert_wish(&mut tx, &wish).await?;
    tx.commit().await?;
    tracing::info!(wish_id = id, "Pokémon wish added");
    Ok(Redirect::to("/pokemon/wishlist").into_response())
}

async fn stored_values(pool: &Pool, id: i64) -> AppResult<WishValues> {
    let wish: WishRow = sqlx::query_as(&format!("{} WHERE w.id = ?", WishRow::SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    Ok(WishValues {
        card: wish.tcgdex_id,
        // a Japanese card's wish in any language: Japanese (the only one)
        language: match (wish.language, wish.catalogue.as_str()) {
            (Some(language), _) => language,
            (None, "ja") => "ja".into(),
            (None, _) => String::new(),
        },
        variant: wish.variant.unwrap_or_default(),
        max_price: wish
            .max_price
            .map(|m| m.0.normalize().to_string())
            .unwrap_or_default(),
        priority: wish.priority.to_string(),
        note: wish.note.unwrap_or_default(),
    })
}

/// `GET /pokemon/wishlist/:id/edit`.
pub async fn edit_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<WishFormPage> {
    Ok(WishFormPage {
        id: Some(id),
        errors: Vec::new(),
        values: stored_values(&state.pool, id).await?,
    })
}

/// `POST /pokemon/wishlist/:id/edit`.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let Some(wish) = read_and_find(&state, &mut form)
        .await?
        .filter(|_| form.errors.is_empty())
    else {
        let page = WishFormPage {
            id: Some(id),
            values: WishValues::from_form(&form),
            errors: form.errors,
        };
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    if !cards::update_wish(&mut tx, id, &wish).await? {
        return Err(AppError::NotFound);
    }
    cards::delete_orphans(&mut tx).await?;
    tx.commit().await?;
    Ok(Redirect::to("/pokemon/wishlist").into_response())
}

/// `POST /pokemon/wishlist/:id/delete` (its card goes too when nothing else
/// uses it).
pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Response> {
    let mut tx = begin_write(&state.pool).await?;
    if !cards::delete_wish(&mut tx, id).await? {
        return Err(AppError::NotFound);
    }
    cards::delete_orphans(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(wish_id = id, "Pokémon wish deleted");
    Ok(Redirect::to("/pokemon/wishlist").into_response())
}

/// `POST /pokemon/wishlist/:id/got`: `copy_id` = the copy that fulfils it
/// (empty: wanted again). `back`: where to go after.
pub async fn got(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let copy_id = form.id("copy_id", "Copy");
    let mut tx = begin_write(&state.pool).await?;
    if let Some(copy_id) = copy_id {
        crate::store::db::exists(&mut tx, "pokemon_copy", copy_id).await?;
    }
    if !cards::set_wish_got(&mut tx, id, copy_id).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;

    let back = form
        .get("back")
        .filter(|b| b.starts_with("/pokemon"))
        .unwrap_or("/pokemon/wishlist");
    Ok(Redirect::to(back).into_response())
}
