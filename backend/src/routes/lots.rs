use askama::Template;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};

use crate::db::{Money, Pool};
use crate::forms::CardRowValues;
use crate::error::{AppResult, OrNotFound};
use crate::models::LotRow;
use crate::pagination::{self, Page, PAGE_SIZE};
use crate::routes::cards::like_pattern;
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
        if let Some(name) = self.seller_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
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
#[template(path = "lots_list.html")]
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
    let lots = select.build_query_as().fetch_all(pool).await?;

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
    price: Option<Money>,
}

#[derive(Template)]
#[template(path = "lot_detail.html")]
pub struct LotDetail {
    lot: LotRow,
    unites: Vec<UniteRow>,
    total_price: Money,
    /// "Add card" form: errors and values of the last submission.
    errors: Vec<String>,
    row: CardRowValues,
}

impl LotDetail {
    fn statuses(&self) -> [domain::card::CardStatus; 5] {
        domain::card::CardStatus::ALL
    }
}

pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<LotDetail> {
    load_detail(&state.pool, id, Vec::new(), CardRowValues::default()).await
}

pub async fn load_detail(
    pool: &Pool,
    id: i64,
    errors: Vec<String>,
    row: CardRowValues,
) -> AppResult<LotDetail> {
    let lot: LotRow = sqlx::query_as(&format!("{} WHERE lot.id = ?", LotRow::SELECT))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    let unites: Vec<UniteRow> = sqlx::query_as(
        "SELECT unite.card_id, card.name AS card_name, unite.price
         FROM unite JOIN card ON card.id = unite.card_id
         WHERE unite.lot_id = ? ORDER BY unite.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let total_price = Money(unites.iter().filter_map(|u| u.price).map(|p| p.0).sum());

    Ok(LotDetail {
        lot,
        unites,
        total_price,
        errors,
        row,
    })
}
