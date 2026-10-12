use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};

use crate::db::Money;
use crate::error::{AppResult, OrNotFound};
use crate::filters;
use crate::models::{CardRow, DeckVersionRow, LotRow};
use crate::pagination::{self, Page, PAGE_SIZE};
use crate::AppState;

/// Card search filters (Django: `CardFilterSerializer` / `filter_cards_queryset`).
/// `proxy` and `sold` take "true" / "false"; anything else means "all".
#[derive(Debug, Default, Deserialize)]
pub struct CardFilter {
    pub name: Option<String>,
    pub code: Option<String>,
    pub status: Option<String>,
    pub card_type: Option<String>,
    pub proxy: Option<String>,
    pub sold: Option<String>,
}

impl CardFilter {
    fn flag(value: &Option<String>) -> Option<bool> {
        match value.as_deref() {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => None,
        }
    }

    fn text(value: &Option<String>) -> Option<&str> {
        value.as_deref().map(str::trim).filter(|v| !v.is_empty())
    }

    /// Append `WHERE ...` (always present, possibly `WHERE 1`) to a query on `card`.
    pub fn push_where(&self, qb: &mut QueryBuilder<'_, Sqlite>) {
        qb.push(" WHERE 1");
        if let Some(status) = Self::text(&self.status) {
            qb.push(" AND card.last_known_status = ")
                .push_bind(status.to_uppercase());
        }
        if let Some(proxy) = Self::flag(&self.proxy) {
            qb.push(" AND card.is_proxy = ").push_bind(proxy);
        }
        if let Some(name) = Self::text(&self.name) {
            let pattern = like_pattern(name);
            qb.push(" AND (card.name LIKE ")
                .push_bind(pattern.clone())
                .push(" ESCAPE '\\' OR card.en_name LIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\')");
        }
        if let Some(code) = Self::text(&self.code) {
            qb.push(" AND card.code LIKE ")
                .push_bind(like_pattern(code))
                .push(" ESCAPE '\\'");
        }
        if let Some(card_type) = Self::text(&self.card_type) {
            qb.push(
                " AND card.card_data_id IN \
                 (SELECT id FROM card_data WHERE card_type = ",
            )
            .push_bind(card_type.to_string())
            .push(" COLLATE NOCASE)");
        }
        if let Some(sold) = Self::flag(&self.sold) {
            qb.push(if sold { " AND " } else { " AND NOT " })
                .push(SOLD_EXISTS);
        }
    }

    fn querystring(&self) -> String {
        pagination::querystring([
            ("name", self.name.as_deref()),
            ("code", self.code.as_deref()),
            ("status", self.status.as_deref()),
            ("card_type", self.card_type.as_deref()),
            ("proxy", self.proxy.as_deref()),
            ("sold", self.sold.as_deref()),
        ])
    }
}

/// A card is sold when it is in a non-cancelled sale lot.
pub const SOLD_EXISTS: &str = "EXISTS (SELECT 1 FROM unite JOIN lot ON lot.id = unite.lot_id \
     WHERE unite.card_id = card.id AND lot.lot_type = 'sale' AND lot.is_cancelled = 0)";

/// `%text%` for LIKE, with `\` escaping the wildcards.
pub fn like_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

pub async fn card_types(pool: &crate::db::Pool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT DISTINCT card_type FROM card_data WHERE card_type IS NOT NULL ORDER BY card_type",
    )
    .fetch_all(pool)
    .await
}

#[derive(Template)]
#[template(path = "cards_list.html")]
pub struct CardsList {
    page_title: &'static str,
    page: Page,
    total: i64,
    cards: Vec<CardRow>,
    filter: CardFilter,
    card_types: Vec<String>,
}

impl CardsList {
    fn selected(&self, value: &Option<String>, option: &str) -> bool {
        value.as_deref().unwrap_or("") == option
    }
}

pub async fn list(
    State(state): State<AppState>,
    Query(page): Query<pagination::PageParam>,
    Query(filter): Query<CardFilter>,
) -> AppResult<CardsList> {
    let pool = &state.pool;

    let mut count = QueryBuilder::new("SELECT count(*) FROM card");
    filter.push_where(&mut count);
    let count: i64 = count.build_query_scalar().fetch_one(pool).await?;
    let page = Page::new(page.page, count, filter.querystring());

    let mut select = QueryBuilder::new(format!("SELECT {} FROM card", CardRow::COLUMNS));
    filter.push_where(&mut select);
    select
        .push(" ORDER BY card.en_name, card.id LIMIT ")
        .push_bind(PAGE_SIZE)
        .push(" OFFSET ")
        .push_bind(page.offset());
    let cards = select.build_query_as().fetch_all(pool).await?;

    Ok(CardsList {
        page_title: "All Cards",
        page,
        total: sqlx::query_scalar("SELECT count(*) FROM card")
            .fetch_one(pool)
            .await?,
        cards,
        filter,
        card_types: card_types(pool).await?,
    })
}

#[derive(Template)]
#[template(path = "card_detail.html")]
pub struct CardDetail {
    card: CardRow,
    /// The lot the card came from (its first unite) and its price there.
    lot: Option<LotRow>,
    price: Option<Money>,
    versions: Vec<DeckVersionRow>,
}

pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<CardDetail> {
    let pool = &state.pool;
    let card: CardRow = sqlx::query_as(&format!(
        "SELECT {} FROM card WHERE card.id = ?",
        CardRow::COLUMNS
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .or_404()?;

    let unite: Option<(i64, Option<Money>)> =
        sqlx::query_as("SELECT lot_id, price FROM unite WHERE card_id = ? ORDER BY id LIMIT 1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let (lot, price) = match unite {
        Some((lot_id, price)) => {
            let lot = sqlx::query_as(&format!("{} WHERE lot.id = ?", LotRow::SELECT))
                .bind(lot_id)
                .fetch_optional(pool)
                .await?;
            (lot, price)
        }
        None => (None, None),
    };

    let versions = sqlx::query_as(&format!(
        "{} WHERE deck_version.id IN (
            SELECT deck_version_id FROM deck_version_main_deck WHERE card_id = ?1
            UNION SELECT deck_version_id FROM deck_version_extra_deck WHERE card_id = ?1
            UNION SELECT deck_version_id FROM deck_version_side_deck WHERE card_id = ?1)
         ORDER BY deck_version.id",
        DeckVersionRow::SELECT
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;

    Ok(CardDetail {
        card,
        lot,
        price,
        versions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("50%_off"), "%50\\%\\_off%");
    }
}
