//! The wishlists (one per person, `store::wishlists`): `/pokemon/wishlist`
//! lists them, `/pokemon/wishlists/:id` is one list: cards wanted, with
//! today's price against the most the owner wants to pay
//! (`pokemon_domain::wish`). Public to read. Adding, editing, deleting and
//! ticking off a wish need the permission of its list
//! (`pokemon_wishlist:<id>`, checked in each handler: the routes are
//! `login_only`); creating, renaming and deleting a list need an admin.
//! A wish is got (`got_on`), with the copy that fulfils it when there is one
//! (`got_copy_id`): the add pages offer it for the lists filled from the
//! collection. Above a list's wishes, the boosters to open for them
//! (`pokemon_domain::booster`, `store::boosters`), also in the public copy.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use pokemon_domain::booster::{suggest, Suggestion};
use pokemon_domain::wish::{Verdict, Wish};
use pokemon_domain::{language_label, parse_language, Catalogue, Variant, LANGUAGES};
use rust_decimal::Decimal;

use super::form::find_card;
use super::{card_names, image_src, money, rarities};
use crate::store::boosters;
use crate::store::cards::{self, WishFields};
use crate::store::prices;
use crate::store::wishlists::{self, Summary, Wishlist};
use common::db::{begin_write, Money, Pool};
use common::web::auth::CurrentUser;
use common::web::error::{bad_request, AppError, AppResult, OrNotFound};
use common::web::forms::{FormData, Pairs};
use common::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct WishRow {
    id: i64,
    card_id: i64,
    language: Option<String>,
    variant: Option<String>,
    max_price: Option<Money>,
    priority: i64,
    note: Option<String>,
    got_on: Option<String>,
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
            w.priority, w.note, w.got_on, w.got_copy_id, c.catalogue, c.tcgdex_id,
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
        common::web::pagination::querystring([
            ("card", Some(self.tcgdex_id.as_str())),
            ("language", Some(language.as_str())),
            ("variant", self.variant.as_deref()),
        ])
    }
}

/// A suggested booster, as the page shows it.
pub struct BoosterRow {
    name: String,
    price: String,
    wished: usize,
    /// The TCGdex sets of its wished cards (`Étincelles Déferlantes`).
    sets: String,
}

impl BoosterRow {
    fn new(suggestion: &Suggestion, wanted: &[WishRow], expansions: &[Option<i64>]) -> BoosterRow {
        let mut sets: Vec<&str> = Vec::new();
        for (wish, expansion) in wanted.iter().zip(expansions) {
            if *expansion == Some(suggestion.booster.expansion)
                && !sets.contains(&wish.set_name.as_str())
            {
                sets.push(&wish.set_name);
            }
        }
        BoosterRow {
            name: suggestion.booster.name.clone(),
            price: money(suggestion.booster.price),
            wished: suggestion.wished,
            sets: sets.join(", "),
        }
    }
}

#[derive(Template)]
#[template(path = "pokemon/wishlist.html")]
pub struct WishlistPage {
    list: Wishlist,
    /// The current user may change this list's wishes.
    can_edit: bool,
    wanted: Vec<WishRow>,
    got: Vec<WishRow>,
    most_wished: Vec<BoosterRow>,
    cheapest: Vec<BoosterRow>,
}

impl WishlistPage {
    fn under_max(&self) -> usize {
        self.wanted
            .iter()
            .filter(|w| w.verdict == Some(Verdict::UnderMax))
            .count()
    }
}

/// `/pokemon/wishlists/:id`.
pub async fn list(
    State(state): State<AppState>,
    user: Option<CurrentUser>,
    Path(list_id): Path<i64>,
) -> AppResult<WishlistPage> {
    let pool = &state.pool;
    let list = wishlists::find(pool, list_id).await?.or_404()?;
    let can_edit = user.is_some_and(|u| u.allows(&wishlists::permission(list.id)));
    let mut wishes: Vec<WishRow> = sqlx::query_as(&format!(
        "{} WHERE w.wishlist_id = ? ORDER BY w.priority, w.added_on, w.id",
        WishRow::SELECT
    ))
    .bind(list_id)
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
    let (got, wanted): (Vec<WishRow>, Vec<WishRow>) =
        wishes.into_iter().partition(|w| w.got_on.is_some());

    let expansion_of: HashMap<i64, Option<i64>> = sqlx::query_as(&format!(
        "SELECT w.id, {} FROM pokemon_wish w JOIN pokemon_card c ON c.id = w.card_id
         WHERE w.got_on IS NULL AND w.wishlist_id = ?",
        boosters::EXPANSION
    ))
    .bind(list_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    let expansions: Vec<Option<i64>> = wanted
        .iter()
        .map(|w| expansion_of.get(&w.id).copied().flatten())
        .collect();
    let known = boosters::all(pool).await?;
    let suggestions = suggest(expansions.iter().copied(), &known);
    let rows = |list: &[Suggestion]| -> Vec<BoosterRow> {
        list.iter()
            .map(|s| BoosterRow::new(s, &wanted, &expansions))
            .collect()
    };
    let most_wished = rows(&suggestions.most_wished);
    let cheapest = rows(&suggestions.cheapest);
    Ok(WishlistPage {
        list,
        can_edit,
        wanted,
        got,
        most_wished,
        cheapest,
    })
}

#[derive(Template)]
#[template(path = "pokemon/wishlists.html")]
pub struct WishlistsPage {
    lists: Vec<Summary>,
    is_admin: bool,
    errors: Vec<String>,
}

async fn index_page(
    state: &AppState,
    user: Option<&CurrentUser>,
    errors: Vec<String>,
) -> AppResult<WishlistsPage> {
    Ok(WishlistsPage {
        lists: wishlists::summaries(&state.pool).await?,
        is_admin: user.is_some_and(CurrentUser::is_admin),
        errors,
    })
}

/// `/pokemon/wishlist`: the wishlists, with their counts.
pub async fn index(
    State(state): State<AppState>,
    user: Option<CurrentUser>,
) -> AppResult<WishlistsPage> {
    index_page(&state, user.as_ref(), Vec::new()).await
}

/// The list name and flag of a form, checked (`except`: the list's own id).
async fn list_form(
    state: &AppState,
    form: &FormData,
    except: Option<i64>,
) -> AppResult<Result<(String, bool), String>> {
    let name = form.text("name");
    if name.is_empty() || name.chars().count() > 60 {
        return Ok(Err("A list needs a name of 60 characters at most.".into()));
    }
    if wishlists::name_taken(&state.pool, &name, except).await? {
        return Ok(Err(format!("There already is a list called {name}.")));
    }
    Ok(Ok((name, form.checked("fills_from_collection"))))
}

/// `POST /pokemon/wishlists/add` (admin).
pub async fn create_list(
    State(state): State<AppState>,
    user: CurrentUser,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    user.require_admin()?;
    let form = FormData::new(pairs);
    let (name, fills) = match list_form(&state, &form, None).await? {
        Ok(read) => read,
        Err(error) => return refused(&state, &user, error).await,
    };
    let id = wishlists::create(&state.pool, &name, fills).await?;
    state.auth.reload(&state.pool).await?;
    tracing::info!(list_id = id, name, "Pokémon wishlist created");
    Ok(Redirect::to("/pokemon/wishlist").into_response())
}

async fn refused(state: &AppState, user: &CurrentUser, error: String) -> AppResult<Response> {
    let page = index_page(state, Some(user), vec![error]).await?;
    Ok((StatusCode::BAD_REQUEST, page).into_response())
}

/// `POST /pokemon/wishlists/:id/edit` (admin): rename, the flag.
pub async fn update_list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    user.require_admin()?;
    let form = FormData::new(pairs);
    let (name, fills) = match list_form(&state, &form, Some(id)).await? {
        Ok(read) => read,
        Err(error) => return refused(&state, &user, error).await,
    };
    if !wishlists::update(&state.pool, id, &name, fills).await? {
        return Err(AppError::NotFound);
    }
    state.auth.reload(&state.pool).await?;
    Ok(Redirect::to("/pokemon/wishlist").into_response())
}

/// `POST /pokemon/wishlists/:id/delete` (admin): its wishes, its permission,
/// and the cards nothing else uses.
pub async fn delete_list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    user.require_admin()?;
    if !wishlists::delete(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    state.auth.reload(&state.pool).await?;
    tracing::info!(list_id = id, "Pokémon wishlist deleted");
    Ok(Redirect::to("/pokemon/wishlist").into_response())
}

/// The list a wish is on, when `user` may change it.
async fn editable_list(state: &AppState, user: &CurrentUser, wish_id: i64) -> AppResult<i64> {
    let list_id = wishlists::of_wish(&state.pool, wish_id).await?.or_404()?;
    user.require(&wishlists::permission(list_id))?;
    Ok(list_id)
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
    list: Wishlist,
    errors: Vec<String>,
    values: WishValues,
}

impl WishFormPage {
    fn action(&self) -> String {
        match self.id {
            Some(id) => format!("/pokemon/wishes/{id}/edit"),
            None => format!("/pokemon/wishlists/{}/add", self.list.id),
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

/// `GET /pokemon/wishlists/:id/add`.
pub async fn add_page(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(list_id): Path<i64>,
) -> AppResult<WishFormPage> {
    user.require(&wishlists::permission(list_id))?;
    let list = wishlists::find(&state.pool, list_id).await?.or_404()?;
    Ok(WishFormPage {
        id: None,
        list,
        errors: Vec::new(),
        values: WishValues {
            priority: "2".into(),
            ..WishValues::default()
        },
    })
}

/// `POST /pokemon/wishlists/:id/add`.
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(list_id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    user.require(&wishlists::permission(list_id))?;
    let list = wishlists::find(&state.pool, list_id).await?.or_404()?;
    let mut form = FormData::new(pairs);
    let Some(wish) = read_and_find(&state, &mut form)
        .await?
        .filter(|_| form.errors.is_empty())
    else {
        let page = WishFormPage {
            id: None,
            list,
            values: WishValues::from_form(&form),
            errors: form.errors,
        };
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;
    let id = cards::insert_wish(&mut tx, list_id, &wish).await?;
    tx.commit().await?;
    tracing::info!(wish_id = id, list_id, "Pokémon wish added");
    Ok(Redirect::to(&format!("/pokemon/wishlists/{list_id}")).into_response())
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

/// `GET /pokemon/wishes/:id/edit`.
pub async fn edit_page(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<WishFormPage> {
    let list_id = editable_list(&state, &user, id).await?;
    Ok(WishFormPage {
        id: Some(id),
        list: wishlists::find(&state.pool, list_id).await?.or_404()?,
        errors: Vec::new(),
        values: stored_values(&state.pool, id).await?,
    })
}

/// `POST /pokemon/wishes/:id/edit`.
pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let list_id = editable_list(&state, &user, id).await?;
    let list = wishlists::find(&state.pool, list_id).await?.or_404()?;
    let mut form = FormData::new(pairs);
    let Some(wish) = read_and_find(&state, &mut form)
        .await?
        .filter(|_| form.errors.is_empty())
    else {
        let page = WishFormPage {
            id: Some(id),
            list,
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
    Ok(Redirect::to(&format!("/pokemon/wishlists/{list_id}")).into_response())
}

/// `POST /pokemon/wishes/:id/delete` (its card goes too when nothing else
/// uses it).
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let list_id = editable_list(&state, &user, id).await?;
    let mut tx = begin_write(&state.pool).await?;
    if !cards::delete_wish(&mut tx, id).await? {
        return Err(AppError::NotFound);
    }
    cards::delete_orphans(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(wish_id = id, "Pokémon wish deleted");
    Ok(Redirect::to(&format!("/pokemon/wishlists/{list_id}")).into_response())
}

/// `POST /pokemon/wishes/:id/got`: `copy_id` = the copy that fulfils it, or
/// `got` = ticked off without one; neither: wanted again. `back`: where to
/// go after.
pub async fn got(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let list_id = editable_list(&state, &user, id).await?;
    let mut form = FormData::new(pairs);
    let copy_id = form.id("copy_id", "Copy");
    if !form.errors.is_empty() {
        return bad_request(form.errors.join(" "));
    }

    let mut tx = begin_write(&state.pool).await?;
    let found = if let Some(copy_id) = copy_id {
        common::db::exists(&mut tx, "pokemon_copy", copy_id).await?;
        cards::set_wish_got(&mut tx, id, Some(copy_id)).await?
    } else if form.checked("got") {
        cards::set_wish_got(&mut tx, id, None).await?
    } else {
        cards::set_wish_wanted(&mut tx, id).await?
    };
    if !found {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;

    let default = format!("/pokemon/wishlists/{list_id}");
    let back = form
        .get("back")
        .filter(|b| b.starts_with("/pokemon"))
        .unwrap_or(&default);
    Ok(Redirect::to(back).into_response())
}
