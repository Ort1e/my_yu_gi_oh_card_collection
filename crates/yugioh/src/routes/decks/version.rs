//! `/deck-versions/:id`: a deck version's cards, read-only, as the card grid:
//! the Main, Extra and Side Decks, monsters then spells then traps, by name.
//! The deck builder (`/deck-builder/:id`) is a button away in the app; the
//! public copy shows only the versions on the public site (`PUBLIC_VERSION`),
//! without the tournament or the buttons. `POST /deck-versions/:id/public`
//! shows or hides a version there.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum::Form;
use yugioh_domain::stats::{kind, CardKind};

use super::PUBLIC_VERSION;
use crate::store::cards::Zone;
use crate::store::db::begin_write;
use crate::store::models::{CardRow, DeckVersionRow};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::forms::{FormData, Pairs};
use crate::AppState;
use common::web::tiles::CardTile;

/// One zone of the version: "Main Deck" and its cards.
pub struct ZoneCards {
    name: &'static str,
    tiles: Vec<CardTile>,
}

impl ZoneCards {
    fn tiles(&self) -> &[CardTile] {
        &self.tiles
    }
}

/// The tournament the version was played in (the app only).
#[derive(sqlx::FromRow)]
pub struct Played {
    id: i64,
    name: String,
    date: String,
}

#[derive(Template)]
#[template(path = "decks/version.html")]
pub struct VersionPage {
    version: DeckVersionRow,
    zones: Vec<ZoneCards>,
    played: Option<Played>,
}

impl VersionPage {
    fn deck_href(&self) -> String {
        self.version
            .deck_id
            .map_or("/decks".to_string(), |id| format!("/decks/{id}"))
    }

    fn no_card(&self) -> bool {
        self.zones.iter().all(|zone| zone.tiles.is_empty())
    }
}

pub async fn page(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<VersionPage> {
    let pool = &state.pool;
    let public = common::export::public_copy();
    let version: DeckVersionRow = sqlx::query_as(&format!(
        "{} WHERE deck_version.id = ?{}",
        DeckVersionRow::SELECT,
        if public {
            format!(" AND {PUBLIC_VERSION}")
        } else {
            String::new()
        }
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .or_404()?;

    let mut zones = Vec::new();
    for (zone, name) in Zone::ALL
        .into_iter()
        .zip(["Main Deck", "Extra Deck", "Side Deck"])
    {
        let rows: Vec<CardRow> = sqlx::query_as(&format!(
            "SELECT {} FROM card JOIN {} zone ON zone.card_id = card.id
             WHERE zone.deck_version_id = ?",
            CardRow::columns(),
            zone.table()
        ))
        .bind(id)
        .fetch_all(pool)
        .await?;
        let types: HashMap<i64, Option<String>> = sqlx::query_as(&format!(
            "SELECT card.id, card_data.card_type
             FROM card JOIN {} zone ON zone.card_id = card.id
             JOIN card_data ON card_data.id = card.card_data_id
             WHERE zone.deck_version_id = ?",
            zone.table()
        ))
        .bind(id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        let kind_of = |card: &CardRow| match types.get(&card.id) {
            Some(Some(card_type)) => kind(card_type),
            _ => CardKind::Other,
        };
        let mut cards: Vec<(CardKind, CardRow)> =
            rows.into_iter().map(|row| (kind_of(&row), row)).collect();
        cards.sort_by(|(a, x), (b, y)| a.cmp(b).then_with(|| x.name.cmp(&y.name)));
        zones.push(ZoneCards {
            name,
            tiles: cards.iter().map(|(_, card)| card.tile()).collect(),
        });
    }

    let played = if public {
        None
    } else {
        sqlx::query_as(
            "SELECT tournament.id, tournament.name, tournament.date FROM tournament
             JOIN deck_version ON deck_version.tournament_id = tournament.id
             WHERE deck_version.id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
    };

    Ok(VersionPage {
        version,
        zones,
        played,
    })
}

/// Show the version on the public site (`is_public=1`) or hide it (`0`); one
/// played in a tournament is shown anyway.
pub async fn set_public(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Redirect> {
    let is_public = FormData::new(pairs).get("is_public") == Some("1");
    let mut tx = begin_write(&state.pool).await?;
    let updated = sqlx::query("UPDATE deck_version SET is_public = ? WHERE id = ?")
        .bind(is_public)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    tracing::info!(version_id = id, is_public, "deck version shown or hidden");

    Ok(Redirect::to(&format!("/deck-versions/{id}")))
}
