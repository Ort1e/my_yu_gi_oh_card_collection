//! `/decks/built`: the built deck versions (`store::lists`: they hold copies
//! of the collection), where they live (deck box, sleeves), how many of their
//! cards are missing (`/decks/shopping` says which), and every deck's value.

use askama::Template;
use axum::extract::State;

use crate::routes::collection::{deck_values, DeckLine, ValueData};
use crate::store::lists;
use crate::store::models::DeckVersionRow;
use crate::web::error::AppResult;
use crate::web::filters;
use crate::AppState;

pub struct BuiltVersion {
    version: DeckVersionRow,
    cards: usize,
    /// Copies it holds.
    held: usize,
    /// Cards the collection lacks (`lists::missing`).
    missing: usize,
}

#[derive(Template)]
#[template(path = "decks/built.html")]
pub struct BuiltPage {
    versions: Vec<BuiltVersion>,
    /// Each deck's latest version: its value and what its missing cards
    /// would cost.
    decks: Vec<DeckLine>,
}

pub async fn page(State(state): State<AppState>) -> AppResult<BuiltPage> {
    let pool = &state.pool;
    let rows: Vec<DeckVersionRow> = sqlx::query_as(&format!(
        "{} WHERE deck_version.is_built = 1 ORDER BY deck.name, deck_version.version_name",
        DeckVersionRow::SELECT
    ))
    .fetch_all(pool)
    .await?;

    let mut conn = pool.acquire().await?;
    let mut versions = Vec::new();
    for version in rows {
        let lines = lists::lines(&mut conn, version.id).await?;
        let missing = lists::missing(&mut conn, version.id, &lines)
            .await?
            .iter()
            .map(|m| m.count as usize)
            .sum();
        let held = lists::playing_copies(&mut conn, version.id, true)
            .await?
            .len();
        versions.push(BuiltVersion {
            cards: lines.iter().map(|l| l.count as usize).sum(),
            held,
            missing,
            version,
        });
    }
    drop(conn);

    let values = ValueData::load(pool).await?;
    Ok(BuiltPage {
        versions,
        decks: deck_values(pool, &values.latest).await?,
    })
}
