pub mod form;

use askama::Template;
use axum::extract::{Path, Query, State};

use crate::store::card_prices::fill_lot_estimates;
use crate::store::models::LotRow;
use crate::web::error::{AppResult, OrNotFound};
use crate::web::filters;
use crate::web::pagination::{Page, PageParam, PAGE_SIZE};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct SellerRow {
    id: i64,
    name: String,
    is_person: bool,
    source: Option<String>,
    lot_count: i64,
}

const SELLER_SELECT: &str = "SELECT seller.id, seller.name, seller.is_person,
        seller_source.name AS source,
        (SELECT count(*) FROM lot WHERE lot.seller_id = seller.id) AS lot_count
    FROM seller LEFT JOIN seller_source ON seller_source.id = seller.source_id";

#[derive(Template)]
#[template(path = "sellers/list.html")]
pub struct SellersList {
    page_title: &'static str,
    page: Page,
    total: i64,
    sellers: Vec<SellerRow>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(param): Query<PageParam>,
) -> AppResult<SellersList> {
    let pool = &state.pool;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM seller")
        .fetch_one(pool)
        .await?;
    let page = Page::new(param.page, total, String::new());
    let sellers = sqlx::query_as(&format!(
        "{SELLER_SELECT} ORDER BY seller.name, seller.id LIMIT ? OFFSET ?"
    ))
    .bind(PAGE_SIZE)
    .bind(page.offset())
    .fetch_all(pool)
    .await?;

    Ok(SellersList {
        page_title: "All Sellers",
        page,
        total,
        sellers,
    })
}

#[derive(Template)]
#[template(path = "sellers/detail.html")]
pub struct SellerDetail {
    seller: SellerRow,
    lots: Vec<LotRow>,
    total_cards: i64,
}

pub async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<SellerDetail> {
    let pool = &state.pool;
    let seller: SellerRow = sqlx::query_as(&format!("{SELLER_SELECT} WHERE seller.id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .or_404()?;
    let mut lots: Vec<LotRow> = sqlx::query_as(&format!(
        "{} WHERE lot.seller_id = ? ORDER BY lot.buy_date, lot.id",
        LotRow::SELECT
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;
    fill_lot_estimates(pool, &mut lots).await?;
    let total_cards = lots.iter().map(|l| l.card_count).sum();

    Ok(SellerDetail {
        seller,
        lots,
        total_cards,
    })
}
