pub mod form;

use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};

use crate::routes::cards::like_pattern;
use crate::store::db::{Money, Pool};
use crate::store::models::{other_name, LotRow};
use crate::store::valuation::{estimate, fill_lot_estimates, MARKET_PRICE};
use crate::web::error::{AppResult, OrNotFound};
use crate::web::forms::CardRowValues;
use crate::web::pagination::{self, Page, PAGE_SIZE};
use crate::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct LotFilter {
    seller_name: Option<String>,
    /// "true": sales only, "false": purchases only.
    sold: Option<String>,
}

impl LotFilter {
    fn push_where(&self, qb: &mut QueryBuilder<'_, Sqlite>) {
        qb.push(" WHERE 1");
        if let Some(name) = self
            .seller_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            qb.push(" AND seller.name LIKE ")
                .push_bind(like_pattern(name))
                .push(" ESCAPE '\\'");
        }
        match self.sold.as_deref() {
            Some("true") => {
                qb.push(" AND lot.lot_type = 'sale'");
            }
            Some("false") => {
                qb.push(" AND lot.lot_type = 'purchase'");
            }
            _ => {}
        }
    }
}

#[derive(Template)]
#[template(path = "lots/list.html")]
pub struct LotsList {
    page_title: &'static str,
    page: Page,
    total: i64,
    lots: Vec<LotRow>,
    filter: LotFilter,
}

impl LotsList {
    fn sold_is(&self, option: &str) -> bool {
        self.filter.sold.as_deref().unwrap_or("") == option
    }
}

pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<pagination::PageParam>,
    Query(filter): Query<LotFilter>,
) -> AppResult<LotsList> {
    let pool = &state.pool;

    let mut count =
        QueryBuilder::new("SELECT count(*) FROM lot LEFT JOIN seller ON seller.id = lot.seller_id");
    filter.push_where(&mut count);
    let count: i64 = count.build_query_scalar().fetch_one(pool).await?;
    let querystring = pagination::querystring([
        ("seller_name", filter.seller_name.as_deref()),
        ("sold", filter.sold.as_deref()),
    ]);
    let page = Page::new(param.page, count, querystring);

    let mut select = QueryBuilder::new(LotRow::SELECT);
    filter.push_where(&mut select);
    select
        .push(" ORDER BY lot.buy_date, lot.id LIMIT ")
        .push_bind(PAGE_SIZE)
        .push(" OFFSET ")
        .push_bind(page.offset());
    let mut lots: Vec<LotRow> = select.build_query_as().fetch_all(pool).await?;
    fill_lot_estimates(pool, &mut lots).await?;

    Ok(LotsList {
        page_title: "All Lots",
        page,
        total: sqlx::query_scalar("SELECT count(*) FROM lot")
            .fetch_one(pool)
            .await?,
        lots,
        filter,
    })
}

#[derive(Debug, sqlx::FromRow)]
pub struct UniteRow {
    card_id: i64,
    card_name: String,
    name_en: Option<String>,
    code: Option<String>,
    rarity: Option<String>,
    price: Option<Money>,
    status: Option<String>,
    market_price: Option<Money>,
}

impl UniteRow {
    fn english_name(&self) -> Option<&str> {
        other_name(&self.card_name, self.name_en.as_deref())
    }

    /// Its estimated value (market price × condition and language).
    fn estimate(&self) -> Option<Money> {
        estimate(
            self.market_price,
            self.status.as_deref(),
            self.code.as_deref(),
        )
    }

    /// For the set code suggestions of the quick add (`data-code`).
    fn code_text(&self) -> &str {
        self.code.as_deref().unwrap_or("")
    }
}

#[derive(Template)]
#[template(path = "lots/detail.html")]
pub struct LotDetail {
    lot: LotRow,
    unites: Vec<UniteRow>,
    total_price: Money,
    /// "Add cards" form: errors and card rows of the last submission (at
    /// least one row).
    errors: Vec<String>,
    rows: Vec<CardRowValues>,
    /// The advent calendar whose lot this is, if any.
    calendar_year: Option<i64>,
}

impl LotDetail {
    fn blank_row(&self) -> CardRowValues {
        CardRowValues::default()
    }

    fn statuses(&self) -> [domain::card::CardStatus; 5] {
        domain::card::CardStatus::ALL
    }

    fn rarities(&self) -> [domain::rarity::Rarity; 34] {
        domain::rarity::Rarity::ALL
    }
}

pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<LotDetail> {
    load_detail(&state.pool, id, Vec::new(), Vec::new()).await
}

pub async fn load_detail(
    pool: &Pool,
    id: i64,
    errors: Vec<String>,
    mut rows: Vec<CardRowValues>,
) -> AppResult<LotDetail> {
    if rows.is_empty() {
        rows.push(CardRowValues::default());
    }
    let mut lot: LotRow = sqlx::query_as(&format!("{} WHERE lot.id = ?", LotRow::SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    fill_lot_estimates(pool, std::slice::from_mut(&mut lot)).await?;
    let unites: Vec<UniteRow> = sqlx::query_as(&format!(
        "SELECT unite.card_id, card.name AS card_name,
                json_extract(card_data.json_data, '$.name') AS name_en,
                card.code, card.rarity, unite.price, card.last_known_status AS status,
                {MARKET_PRICE} AS market_price
         FROM unite JOIN card ON card.id = unite.card_id
         LEFT JOIN card_data ON card_data.id = card.card_data_id
         WHERE unite.lot_id = ? ORDER BY unite.id"
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;
    let total_price = Money(unites.iter().filter_map(|u| u.price).map(|p| p.0).sum());

    let calendar_year = sqlx::query_scalar("SELECT year FROM advent_calendar WHERE lot_id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(LotDetail {
        lot,
        unites,
        total_price,
        errors,
        rows,
        calendar_year,
    })
}
