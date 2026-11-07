//! `/deck-versions/:id`: a deck version's cards, read-only, as the card grid:
//! the Main, Extra and Side Decks, each by kind (monsters, Extra Deck
//! monsters, spells, traps: `yugioh_domain::stats::kind`), then by English
//! name ignoring case and accents (`yugioh_domain::card::sort_name`), so the
//! copies of a card sit together whatever their language. The deck builder
//! sorts the same way (`frontend/src/deck.ts`).
//! Around the cards: the version's format (its problems there, its list)
//! or ban list (its own, else the latest one: the restricted cards and a
//! badge on their tiles), its opening hands (the
//! deck builder's Monte Carlo on its categories, `yugioh_domain::monte_carlo`,
//! going first and second) and the tournament it was played in, round by
//! round.
//! The page is only the read-only view: all the editing is in the deck builder
//! (`/deck-builder/:id`, a button away in the app), which also shows or hides
//! the version on the public site (`is_public`, the `PATCH` of its API). The
//! public copy shows only the versions on the public site (`PUBLIC_VERSION`),
//! without the button.

use std::collections::{BTreeMap, HashMap};

use askama::Template;
use axum::extract::{Path, State};
use rand::rngs::StdRng;
use rand::SeedableRng;
use yugioh_domain::card::sort_name;
use yugioh_domain::monte_carlo;
use yugioh_domain::stats::{kind, CardKind};

use super::api::simulation_inputs;
use super::PUBLIC_VERSION;
use crate::routes::tournaments::TournamentView;
use crate::store::cards::Zone;
use crate::store::db::Pool;
use crate::store::formats;
use crate::store::models::{CardRow, DeckVersionRow};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::filters;
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

/// The list the page follows: the version's format, else its ban list,
/// else the latest one.
pub struct BanList {
    date: String,
    /// Chosen in the deck builder (not the latest one by default).
    chosen: bool,
    /// The version's restricted cards: status, English name, copies.
    cards: Vec<(&'static str, String, usize)>,
    /// The format played: `(id, "Edison v2", cutoff)`, its problems.
    format: Option<(i64, String, String)>,
    problems: Vec<String>,
}

/// The opening hands of one category: the share of hands with at least one
/// of its cards (`77%`) going first (5 cards) and second (6 cards), and how
/// many of them a hand has on average going first (`1.2`).
pub struct HandLine {
    name: String,
    cards: u32,
    first: String,
    second: String,
    average: String,
}

/// Hands drawn per hand size (the page says 10,000): enough for a percent; the same draws each time
/// (seeded by the version), so the public copy changes only with the deck.
const HANDS: u32 = 10_000;

#[derive(Template)]
#[template(path = "decks/version.html")]
pub struct VersionPage {
    version: DeckVersionRow,
    zones: Vec<ZoneCards>,
    ban_list: Option<BanList>,
    hands: Vec<HandLine>,
    played: Option<TournamentView>,
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

/// A stored `ban_list_entry.status` as a `'static` label.
fn ban_label(status: &str) -> Option<&'static str> {
    ["Banned", "Limited", "Semi-Limited"]
        .into_iter()
        .find(|label| *label == status)
}

/// The format a version plays, `(id, label, cutoff, its problems)`.
async fn format(pool: &Pool, id: i64) -> AppResult<Option<(i64, String, String, Vec<String>)>> {
    let mut conn = pool.acquire().await?;
    Ok(formats::version_problems(&mut conn, id)
        .await?
        .map(|(format, problems)| {
            let problems = problems.iter().map(|p| p.message()).collect();
            (format.id, format.label(), format.cutoff, problems)
        }))
}

/// The ban list the page follows, `(id, date, chosen)`: the version's, else
/// the latest one.
async fn ban_list(pool: &Pool, id: i64) -> AppResult<Option<(i64, String, bool)>> {
    let chosen: Option<(i64, String)> = sqlx::query_as(
        "SELECT advanced_ban_list.id, advanced_ban_list.date FROM advanced_ban_list
         JOIN deck_version ON deck_version.ban_list_id = advanced_ban_list.id
         WHERE deck_version.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some((list_id, date)) = chosen {
        return Ok(Some((list_id, date, true)));
    }

    let latest: Option<(i64, String)> =
        sqlx::query_as("SELECT id, date FROM advanced_ban_list ORDER BY date DESC LIMIT 1")
            .fetch_optional(pool)
            .await?;
    Ok(latest.map(|(list_id, date)| (list_id, date, false)))
}

/// The opening hands of the version's categories, empty without any.
async fn hands(pool: &Pool, id: i64) -> AppResult<Vec<HandLine>> {
    let mut conn = pool.acquire().await?;
    let (main, categories, assignments) = simulation_inputs(&mut conn, id).await?;
    drop(conn);
    if categories.is_empty() || main.is_empty() {
        return Ok(Vec::new());
    }

    let (first, second) = tokio::task::spawn_blocking(move || {
        let mut rng = StdRng::seed_from_u64(id as u64);
        let mut draw = |hand_size| {
            monte_carlo::simulate(&main, &categories, &assignments, hand_size, HANDS, &mut rng)
        };
        (draw(5), draw(6))
    })
    .await
    .map_err(|e| AppError::Internal(format!("simulation: {e}")))?;

    let at_least_one = |per_hand: &BTreeMap<u32, u32>, total: u32| {
        let none = per_hand.get(&0).copied().unwrap_or(0);
        format!("{:.0}%", f64::from(total - none) * 100.0 / f64::from(total))
    };
    Ok(first
        .categories
        .iter()
        .zip(&second.categories)
        .map(|(first_hand, second_hand)| HandLine {
            name: first_hand.name.clone(),
            cards: first_hand.cards_in_deck,
            first: at_least_one(&first_hand.per_hand, first.total_simulations),
            second: at_least_one(&second_hand.per_hand, second.total_simulations),
            average: format!(
                "{:.1}",
                f64::from(first_hand.occurrences) / f64::from(first.total_simulations)
            ),
        })
        .collect())
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

    let format = format(pool, id).await?;
    let ban = match &format {
        Some(_) => None,
        None => ban_list(pool, id).await?,
    };
    let entries: Vec<(i64, String)> = match (&format, &ban) {
        (Some((format_id, ..)), _) => {
            sqlx::query_as("SELECT card_data_id, status FROM deck_format_entry WHERE format_id = ?")
                .bind(format_id)
                .fetch_all(pool)
                .await?
        }
        (None, Some((list_id, ..))) => {
            sqlx::query_as("SELECT card_data_id, status FROM ban_list_entry WHERE ban_list_id = ?")
                .bind(list_id)
                .fetch_all(pool)
                .await?
        }
        (None, None) => Vec::new(),
    };
    let statuses: HashMap<i64, &'static str> = entries
        .into_iter()
        .filter_map(|(data_id, status)| Some((data_id, ban_label(&status)?)))
        .collect();
    // status, English name → copies in the version
    let mut restricted: BTreeMap<(&'static str, String), usize> = BTreeMap::new();

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
        // (kind, English name, name shown, id): the same card's copies
        // together, the French ones after the English ones
        let mut cards: Vec<((CardKind, String, String, i64), CardRow)> = rows
            .into_iter()
            .map(|row| {
                let english = sort_name(row.name_en.as_deref().unwrap_or(&row.name));
                ((kind_of(&row), english, sort_name(&row.name), row.id), row)
            })
            .collect();
        cards.sort_by(|(a, _), (b, _)| a.cmp(b));
        let tiles = cards
            .iter()
            .map(|(_, card)| {
                let mut tile = card.tile();
                tile.ban = card.card_data_id.and_then(|d| statuses.get(&d).copied());
                if let Some(status) = tile.ban {
                    let english = card.name_en.clone().unwrap_or_else(|| card.name.clone());
                    *restricted.entry((status, english)).or_default() += 1;
                }
                tile
            })
            .collect();
        zones.push(ZoneCards { name, tiles });
    }

    let tournament_id: Option<i64> =
        sqlx::query_scalar("SELECT tournament_id FROM deck_version WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;
    let played = match tournament_id {
        Some(tournament_id) => TournamentView::load(pool, tournament_id).await?,
        None => None,
    };

    Ok(VersionPage {
        version,
        zones,
        ban_list: {
            let cards = restricted
                .into_iter()
                .map(|((status, name), copies)| (status, name, copies))
                .collect();
            match format {
                Some((format_id, label, cutoff, problems)) => Some(BanList {
                    date: String::new(),
                    chosen: true,
                    cards,
                    format: Some((format_id, label, cutoff)),
                    problems,
                }),
                None => ban.map(|(_, date, chosen)| BanList {
                    date,
                    chosen,
                    cards,
                    format: None,
                    problems: Vec::new(),
                }),
            }
        },
        hands: hands(pool, id).await?,
        played,
    })
}
