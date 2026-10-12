//! Monthly budgets and apports (one-off contributions). Django only edited
//! them through its admin.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::NaiveDate;

use crate::db::{Money, Pool};
use crate::error::AppResult;
use crate::forms::{FormData, Pairs};
use crate::AppState;

#[derive(Debug, sqlx::FromRow)]
pub struct BudgetRow {
    id: i64,
    month: String,
    amount: Money,
}

impl BudgetRow {
    /// "2025-03-01" -> "March 2025"
    fn month_label(&self) -> String {
        self.month
            .parse::<NaiveDate>()
            .map(|d| d.format("%B %Y").to_string())
            .unwrap_or_else(|_| self.month.clone())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct ApportRow {
    id: i64,
    budget_id: i64,
    description: String,
    amount: Money,
    date: String,
}

#[derive(Template)]
#[template(path = "budget_manage.html")]
pub struct ManagePage {
    errors: Vec<String>,
    /// Most recent first.
    budgets: Vec<BudgetRow>,
    apports: Vec<ApportRow>,
}

impl ManagePage {
    fn apports_of(&self, budget_id: &i64) -> Vec<&ApportRow> {
        self.apports
            .iter()
            .filter(|a| a.budget_id == *budget_id)
            .collect()
    }
}

async fn load(pool: &Pool, errors: Vec<String>) -> AppResult<ManagePage> {
    Ok(ManagePage {
        errors,
        budgets: sqlx::query_as("SELECT id, month, amount FROM monthly_budget ORDER BY month DESC")
            .fetch_all(pool)
            .await?,
        apports: sqlx::query_as(
            "SELECT id, budget_id, description, amount, date FROM apport ORDER BY date, id",
        )
        .fetch_all(pool)
        .await?,
    })
}

pub async fn page(State(state): State<AppState>) -> AppResult<ManagePage> {
    load(&state.pool, Vec::new()).await
}

const MANAGE: &str = "/budget/manage";

/// Set the monthly budget from a month on (creates or updates that month).
pub async fn set_budget(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    // <input type="month"> sends "YYYY-MM"
    let month = form.required("month", "Month").and_then(|m| {
        let parsed = NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d").ok();
        if parsed.is_none() {
            form.errors
                .push(format!("Month: \"{m}\" is not a valid month."));
        }
        parsed
    });
    let amount = form.required_decimal("amount", "Amount");
    let (Some(month), Some(amount)) = (month, amount) else {
        return Ok(load(&state.pool, form.errors).await?.into_response());
    };

    sqlx::query(
        "INSERT INTO monthly_budget (month, amount) VALUES (?, ?)
         ON CONFLICT (month) DO UPDATE SET amount = excluded.amount",
    )
    .bind(month.to_string())
    .bind(Money(amount))
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to(MANAGE).into_response())
}

pub async fn add_apport(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let budget_id = form.id("budget_id", "Budget month");
    if budget_id.is_none() && form.errors.is_empty() {
        form.errors.push("Budget month is required.".into());
    }
    let amount = form.required_decimal("amount", "Amount");
    let date = form.required_date("date", "Date");
    let description = form.text("description");
    let (Some(budget_id), Some(amount), Some(date)) = (budget_id, amount, date) else {
        return Ok(load(&state.pool, form.errors).await?.into_response());
    };

    sqlx::query("INSERT INTO apport (budget_id, description, amount, date) VALUES (?, ?, ?, ?)")
        .bind(budget_id)
        .bind(description)
        .bind(Money(amount))
        .bind(date.to_string())
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(MANAGE).into_response())
}

pub async fn delete_apport(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Redirect> {
    sqlx::query("DELETE FROM apport WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(MANAGE))
}
