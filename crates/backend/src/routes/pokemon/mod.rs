//! The Pokémon card collection: a third collection with its own tables
//! (migration 0012), nothing shared with the Yu-Gi-Oh! cards but the app.
//! Every route starts with `/pokemon`. Reads are public like the cards,
//! writes and the TCGdex lookups behind the login.
//!
//! - `mod.rs`: the collection (a grid of pictures or a table) and a copy's page;
//! - `form.rs`: add one copy (singles, gifts, graded copies), edit, sell;
//! - `booster.rs`: add a booster opening's cards at once (or more cards to one);
//! - `openings.rs`: the booster openings, one with its cards;
//! - `wishlist.rs`: the wishes, their price against the maximum;
//! - `api.rs`: the card finder's JSON (`static/js/pokemon_finder.js`);
//! - `rarities.rs`: the rarity and variant guide; `images.rs`: card pictures.
//!
//! Copies are never deleted (sold instead); wishes are. Card data and
//! prices: `store::pokemon`; the rules: `domain::pokemon`.

pub mod api;
pub mod booster;
pub mod form;
pub mod images;
pub mod openings;
pub mod rarities;
pub mod wishlist;

use std::collections::{BTreeSet, HashMap};

use askama::Template;
use axum::extract::{Path, Query, State};
use domain::card::CardStatus;
use domain::pokemon::price::copy_value;
use domain::pokemon::{language_label, Source, Variant};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};

use crate::routes::cards::like_pattern;
use crate::store::db::{Money, Pool};
use crate::store::pokemon::prices::{self, VariantPrices};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::filters;
use crate::web::pagination::{self, Page, PageParam};
use crate::AppState;

/// Copies per page of the picture grid.
const GRID_PAGE_SIZE: i64 = 24;

/// The names a card is shown with: the one of the copy's language first,
/// then another one (French under English, the species' French name under
/// a Japanese name). `(main, other)`, `other` empty when none.
pub fn card_names(
    language: &str,
    fr: Option<&str>,
    en: Option<&str>,
    ja: Option<&str>,
    manual: Option<&str>,
) -> (String, String) {
    let own = match language {
        "fr" => fr,
        "ja" => ja,
        "en" => en,
        _ => None,
    };
    let main = own
        .or(manual)
        .or(fr)
        .or(en)
        .or(ja)
        .unwrap_or("?")
        .to_string();
    let others = match language {
        "ja" => [manual, fr, en],
        "fr" => [en, manual, None],
        _ => [en, fr, manual],
    };
    let other = others
        .into_iter()
        .flatten()
        .find(|other| *other != main)
        .unwrap_or_default()
        .to_string();
    (main, other)
}

/// Whether a card has a picture to show: its own, or a borrowed one, or
/// none tried yet (the picture route borrows one: `image_from` NULL).
pub fn has_picture(
    image: &Option<String>,
    image_fr: &Option<String>,
    image_from: &Option<String>,
) -> bool {
    image.is_some() || image_fr.is_some() || image_from.is_none()
}

/// The picture of a card in a language (`/pokemon/card-image/…`).
pub fn image_src(card_id: i64, language: &str) -> String {
    format!("/pokemon/card-image/{card_id}/{language}")
}

/// One copy with its card, as the pages show it.
#[derive(Debug, sqlx::FromRow)]
pub struct CopyRow {
    pub id: i64,
    pub card_id: i64,
    pub language: String,
    pub variant: String,
    pub condition: Option<String>,
    pub full_art: bool,
    pub source: String,
    pub obtained_on: Option<String>,
    pub price: Option<Money>,
    pub grader: Option<String>,
    pub grade: Option<String>,
    pub cert_number: Option<String>,
    pub estimate: Option<Money>,
    pub sold_on: Option<String>,
    pub sold_price: Option<Money>,
    pub note: Option<String>,
    pub added_on: String,
    pub opening_id: Option<i64>,
    pub catalogue: String,
    pub tcgdex_id: String,
    pub set_name: String,
    pub local_id: String,
    pub name_fr: Option<String>,
    pub name_en: Option<String>,
    pub name_ja: Option<String>,
    pub name_manual: Option<String>,
    pub rarity: Option<String>,
    pub types: Option<String>,
    pub image: Option<String>,
    pub image_fr: Option<String>,
    /// The print whose picture is shown, when not the card's own.
    pub image_from: Option<String>,
    /// Today's market value (`None`: no price, or sold).
    #[sqlx(skip)]
    pub value: Option<Decimal>,
}

impl CopyRow {
    /// `FROM pokemon_copy pc JOIN pokemon_card c`: add `WHERE`.
    pub const SELECT: &'static str = "SELECT pc.id, pc.card_id, pc.language, pc.variant,
            pc.condition, pc.full_art, pc.source, pc.obtained_on, pc.price, pc.grader,
            pc.grade, pc.cert_number, pc.estimate, pc.sold_on, pc.sold_price, pc.note,
            pc.added_on, pc.opening_id, c.catalogue, c.tcgdex_id, c.set_name, c.local_id,
            c.name_fr, c.name_en, c.name_ja, c.name_manual, c.rarity, c.types, c.image,
            c.image_fr, c.image_from
         FROM pokemon_copy pc JOIN pokemon_card c ON c.id = pc.card_id";

    fn names(&self) -> (String, String) {
        card_names(
            &self.language,
            self.name_fr.as_deref(),
            self.name_en.as_deref(),
            self.name_ja.as_deref(),
            self.name_manual.as_deref(),
        )
    }

    pub fn name(&self) -> String {
        self.names().0
    }

    pub fn other_name(&self) -> String {
        self.names().1
    }

    pub fn variant_value(&self) -> Option<Variant> {
        Variant::from_code(&self.variant)
    }

    pub fn variant_label(&self) -> &str {
        match self.variant_value() {
            Some(variant) => variant.label(),
            None => &self.variant,
        }
    }

    pub fn status(&self) -> Option<CardStatus> {
        self.condition.as_deref().and_then(CardStatus::from_code)
    }

    pub fn condition_code(&self) -> &str {
        self.condition.as_deref().unwrap_or("")
    }

    pub fn condition_label(&self) -> &str {
        self.status().map_or("", CardStatus::label)
    }

    pub fn language_label(&self) -> &str {
        language_label(&self.language)
    }

    pub fn source_label(&self) -> &str {
        match Source::from_code(&self.source) {
            Some(source) => source.label(),
            None => &self.source,
        }
    }

    pub fn has_image(&self) -> bool {
        has_picture(&self.image, &self.image_fr, &self.image_from)
    }

    /// Where its picture comes from when not TCGdex (`pokemontcg:swsh10tg-TG29`),
    /// or empty.
    pub fn picture_from(&self) -> &str {
        self.image_from.as_deref().unwrap_or("")
    }

    pub fn image_src(&self) -> String {
        image_src(self.card_id, &self.language)
    }

    pub fn is_sold(&self) -> bool {
        self.sold_on.is_some()
    }

    pub fn is_graded(&self) -> bool {
        self.grader.is_some()
    }

    /// `PSA 10`.
    pub fn grading(&self) -> String {
        match (&self.grader, &self.grade) {
            (Some(grader), Some(grade)) => format!("{grader} {grade}"),
            (Some(grader), None) => grader.clone(),
            _ => String::new(),
        }
    }

    pub fn types_text(&self) -> String {
        self.types.as_deref().unwrap_or("").replace(',', ", ")
    }

    pub fn rarity_text(&self) -> &str {
        self.rarity.as_deref().unwrap_or("")
    }

    /// The rarity guide's link for this card's rarity.
    pub fn rarity_href(&self) -> String {
        rarities::href(self.rarity_text())
    }

    /// `12.50`, or empty.
    pub fn value_text(&self) -> String {
        money(self.value)
    }

    pub fn price_text(&self) -> String {
        money(self.price.map(|m| m.0))
    }

    pub fn sold_price_text(&self) -> String {
        money(self.sold_price.map(|m| m.0))
    }

    pub fn estimate_text(&self) -> String {
        money(self.estimate.map(|m| m.0))
    }

    fn set_value(&mut self, prices: &HashMap<i64, VariantPrices>) {
        if self.is_sold() {
            self.value = None;
            return;
        }
        let empty = VariantPrices::new();
        let prices = prices.get(&self.card_id).unwrap_or(&empty);
        self.value = self.variant_value().and_then(|variant| {
            copy_value(variant, self.status(), self.estimate.map(|m| m.0), prices)
        });
    }
}

fn money(amount: Option<Decimal>) -> String {
    amount
        .map(|a| filters::money(&a).unwrap_or_default())
        .unwrap_or_default()
}

/// The list's filters and sort; every field as typed.
#[derive(Debug, Default, Deserialize)]
pub struct PokemonFilter {
    pub q: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub set: Option<String>,
    pub rarity: Option<String>,
    pub full_art: Option<String>,
    pub language: Option<String>,
    pub graded: Option<String>,
    pub source: Option<String>,
    /// `owned` (default), `sold`, `all`.
    pub sold: Option<String>,
    /// `set` (default), `price`, `added`.
    pub sort: Option<String>,
    /// `grid` (default), `table`.
    pub view: Option<String>,
}

fn given(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

impl PokemonFilter {
    fn push_where(&self, qb: &mut QueryBuilder<'_, Sqlite>) {
        qb.push(" WHERE 1");
        if let Some(q) = given(&self.q) {
            let pattern = like_pattern(q);
            qb.push(" AND (");
            for (i, column) in ["c.name_fr", "c.name_en", "c.name_ja", "c.name_manual"]
                .iter()
                .enumerate()
            {
                if i > 0 {
                    qb.push(" OR ");
                }
                qb.push(*column)
                    .push(" LIKE ")
                    .push_bind(pattern.clone())
                    .push(" ESCAPE '\\'");
            }
            qb.push(")");
        }
        if let Some(kind) = given(&self.kind) {
            qb.push(" AND (',' || c.types || ',') LIKE ")
                .push_bind(format!("%,{kind},%"));
        }
        if let Some(set) = given(&self.set) {
            qb.push(" AND c.set_id = ").push_bind(set.to_string());
        }
        if let Some(rarity) = given(&self.rarity) {
            qb.push(" AND c.rarity = ").push_bind(rarity.to_string());
        }
        if given(&self.full_art).is_some() {
            qb.push(" AND pc.full_art = 1");
        }
        if let Some(language) = given(&self.language) {
            qb.push(" AND pc.language = ")
                .push_bind(language.to_string());
        }
        if given(&self.graded).is_some() {
            qb.push(" AND pc.grader IS NOT NULL");
        }
        if let Some(source) = given(&self.source) {
            qb.push(" AND pc.source = ").push_bind(source.to_string());
        }
        match given(&self.sold) {
            Some("sold") => qb.push(" AND pc.sold_on IS NOT NULL"),
            Some("all") => qb.push(""),
            _ => qb.push(" AND pc.sold_on IS NULL"),
        };
    }

    fn sort(&self, copies: &mut [CopyRow]) {
        match given(&self.sort) {
            Some("price") => copies.sort_by(|a, b| b.value.cmp(&a.value).then(a.id.cmp(&b.id))),
            Some("added") => {
                copies.sort_by(|a, b| b.obtained_on.cmp(&a.obtained_on).then(b.id.cmp(&a.id)))
            }
            _ => {} // the query's order: set, then number
        }
    }

    fn querystring(&self) -> String {
        pagination::querystring([
            ("q", self.q.as_deref()),
            ("type", self.kind.as_deref()),
            ("set", self.set.as_deref()),
            ("rarity", self.rarity.as_deref()),
            ("full_art", self.full_art.as_deref()),
            ("language", self.language.as_deref()),
            ("graded", self.graded.as_deref()),
            ("source", self.source.as_deref()),
            ("sold", self.sold.as_deref()),
            ("sort", self.sort.as_deref()),
            ("view", self.view.as_deref()),
        ])
    }

    fn is_table(&self) -> bool {
        given(&self.view) == Some("table")
    }

    fn is(value: &Option<String>, option: &str) -> bool {
        given(value) == Some(option)
    }
}

/// The choices of the filter form, from the copies owned.
#[derive(Debug, Default)]
pub struct FilterOptions {
    types: Vec<String>,
    /// `(set id, name)`.
    sets: Vec<(String, String)>,
    rarities: Vec<String>,
    languages: Vec<(String, String)>,
}

async fn filter_options(pool: &Pool) -> sqlx::Result<FilterOptions> {
    let rows: Vec<(Option<String>, String, String, Option<String>)> = sqlx::query_as(
        "SELECT DISTINCT c.types, c.set_id, c.set_name, c.rarity FROM pokemon_card c
         WHERE c.id IN (SELECT card_id FROM pokemon_copy) ORDER BY c.set_id",
    )
    .fetch_all(pool)
    .await?;
    let languages: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT language FROM pokemon_copy ORDER BY language")
            .fetch_all(pool)
            .await?;

    let mut types = BTreeSet::new();
    let mut sets = Vec::new();
    let mut rarities = BTreeSet::new();
    for (row_types, set_id, set_name, rarity) in rows {
        types.extend(
            row_types
                .unwrap_or_default()
                .split(',')
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        );
        if !sets.iter().any(|(id, _)| *id == set_id) {
            sets.push((set_id, set_name));
        }
        rarities.extend(rarity);
    }
    Ok(FilterOptions {
        types: types.into_iter().collect(),
        sets,
        rarities: rarities.into_iter().collect(),
        languages: languages
            .into_iter()
            .map(|l| (language_label(&l).to_string(), l))
            .collect(),
    })
}

#[derive(Template)]
#[template(path = "pokemon/list.html")]
pub struct PokemonList {
    page: Page,
    copies: Vec<CopyRow>,
    /// Every copy ever added (sold ones too).
    total: i64,
    /// The value of the copies matching the filters.
    value: Decimal,
    without_price: usize,
    filter: PokemonFilter,
    options: FilterOptions,
}

impl PokemonList {
    fn value_text(&self) -> String {
        money(Some(self.value))
    }

    fn is_table(&self) -> bool {
        self.filter.is_table()
    }

    fn sort_is(&self, option: &str) -> bool {
        given(&self.filter.sort).unwrap_or("set") == option
    }

    fn sold_is(&self, option: &str) -> bool {
        given(&self.filter.sold).unwrap_or("owned") == option
    }

    fn type_is(&self, option: &str) -> bool {
        PokemonFilter::is(&self.filter.kind, option)
    }

    fn set_is(&self, option: &str) -> bool {
        PokemonFilter::is(&self.filter.set, option)
    }

    fn rarity_is(&self, option: &str) -> bool {
        PokemonFilter::is(&self.filter.rarity, option)
    }

    fn language_is(&self, option: &str) -> bool {
        PokemonFilter::is(&self.filter.language, option)
    }

    fn source_is(&self, option: &str) -> bool {
        PokemonFilter::is(&self.filter.source, option)
    }

    fn q_value(&self) -> &str {
        given(&self.filter.q).unwrap_or("")
    }

    fn full_art_checked(&self) -> bool {
        given(&self.filter.full_art).is_some()
    }

    fn graded_checked(&self) -> bool {
        given(&self.filter.graded).is_some()
    }

    fn sources(&self) -> [Source; 5] {
        Source::ALL
    }

    /// The same filters, in the other view.
    fn other_view_href(&self) -> String {
        let mut filter = PokemonFilter {
            view: Some(if self.is_table() { "grid" } else { "table" }.to_string()),
            ..PokemonFilter::default()
        };
        let f = &self.filter;
        filter.q.clone_from(&f.q);
        filter.kind.clone_from(&f.kind);
        filter.set.clone_from(&f.set);
        filter.rarity.clone_from(&f.rarity);
        filter.full_art.clone_from(&f.full_art);
        filter.language.clone_from(&f.language);
        filter.graded.clone_from(&f.graded);
        filter.source.clone_from(&f.source);
        filter.sold.clone_from(&f.sold);
        filter.sort.clone_from(&f.sort);
        format!("/pokemon?{}", filter.querystring())
    }

    fn view(&self) -> &'static str {
        if self.is_table() {
            "table"
        } else {
            "grid"
        }
    }
}

/// `/pokemon`.
pub async fn list(
    State(state): State<AppState>,
    Query(page): Query<PageParam>,
    Query(filter): Query<PokemonFilter>,
) -> AppResult<PokemonList> {
    let pool = &state.pool;

    let mut select = QueryBuilder::new(CopyRow::SELECT);
    filter.push_where(&mut select);
    select.push(
        " ORDER BY c.catalogue, c.set_id,
             CAST(ltrim(c.local_id, 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz') AS INTEGER),
             c.local_id, pc.id",
    );
    let mut copies: Vec<CopyRow> = select.build_query_as().fetch_all(pool).await?;

    let prices = prices::latest(pool).await?;
    for copy in &mut copies {
        copy.set_value(&prices);
    }
    let value = copies
        .iter()
        .filter_map(|c| c.value)
        .sum::<Decimal>()
        .round_dp(2);
    let without_price = copies
        .iter()
        .filter(|c| !c.is_sold() && c.value.is_none())
        .count();
    filter.sort(&mut copies);

    let size = if filter.is_table() {
        100
    } else {
        GRID_PAGE_SIZE
    };
    let page = Page::sized(page.page, copies.len() as i64, filter.querystring(), size);
    let copies = copies
        .into_iter()
        .skip(page.offset() as usize)
        .take(size as usize)
        .collect();

    Ok(PokemonList {
        page,
        copies,
        total: sqlx::query_scalar("SELECT count(*) FROM pokemon_copy")
            .fetch_one(pool)
            .await?,
        value,
        without_price,
        options: filter_options(pool).await?,
        filter,
    })
}

/// One price of a copy's history.
pub struct PricePoint {
    date: String,
    price: String,
}

#[derive(Template)]
#[template(path = "pokemon/detail.html")]
pub struct CopyDetail {
    copy: CopyRow,
    history: Vec<PricePoint>,
    /// The other copies of the same card: `(id, language label, variant label, sold)`.
    others: Vec<(i64, String, String, bool)>,
    /// The wishes this copy fulfilled: `(id)`.
    wishes_got: Vec<i64>,
    /// The latest market price of its variant (before the condition factor).
    market: Option<Decimal>,
}

impl CopyDetail {
    fn market_text(&self) -> String {
        money(self.market)
    }
}

/// `/pokemon/:id`.
pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<CopyDetail> {
    let pool = &state.pool;
    let mut copy: CopyRow = sqlx::query_as(&format!("{} WHERE pc.id = ?", CopyRow::SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    let prices = prices::latest(pool).await?;
    copy.set_value(&prices);

    let variant = copy.variant_value();
    let history = prices::history(pool, copy.card_id)
        .await?
        .into_iter()
        .filter_map(|(date, prices)| {
            let price = prices.get(&variant?)?;
            Some(PricePoint {
                date,
                price: money(Some(*price)),
            })
        })
        .collect();
    let market = variant.and_then(|v| prices.get(&copy.card_id)?.get(&v).copied());

    let others: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, language, variant, sold_on FROM pokemon_copy
         WHERE card_id = ? AND id != ? ORDER BY id",
    )
    .bind(copy.card_id)
    .bind(id)
    .fetch_all(pool)
    .await?;
    let wishes_got = sqlx::query_scalar("SELECT id FROM pokemon_wish WHERE got_copy_id = ?")
        .bind(id)
        .fetch_all(pool)
        .await?;

    Ok(CopyDetail {
        copy,
        history,
        others: others
            .into_iter()
            .map(|(id, language, variant, sold_on)| {
                let variant = Variant::from_code(&variant).map_or(variant, |v| v.label().into());
                (
                    id,
                    language_label(&language).to_string(),
                    variant,
                    sold_on.is_some(),
                )
            })
            .collect(),
        wishes_got,
        market,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_copy_language() {
        let names = |language| card_names(language, Some("Hypotrempe"), Some("Horsea"), None, None);
        assert_eq!(names("fr"), ("Hypotrempe".into(), "Horsea".into()));
        assert_eq!(names("en"), ("Horsea".into(), "Hypotrempe".into()));
        assert_eq!(names("de"), ("Hypotrempe".into(), "Horsea".into()));

        let japanese = card_names(
            "ja",
            Some("Hypotrempe"),
            Some("Horsea"),
            Some("タッツー"),
            None,
        );
        assert_eq!(japanese, ("タッツー".into(), "Hypotrempe".into()));
        let trainer = card_names("ja", None, None, Some("ネモ"), Some("Nemona"));
        assert_eq!(trainer, ("ネモ".into(), "Nemona".into()));
        assert_eq!(
            card_names("fr", None, None, None, None),
            ("?".into(), String::new())
        );
    }
}
