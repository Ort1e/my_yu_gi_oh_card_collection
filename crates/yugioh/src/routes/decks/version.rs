//! `/deck-versions/:id`: a deck version's cards, read-only, as the card grid:
//! the Main, Extra and Side Decks, each by kind (monsters, Extra Deck
//! monsters, spells, traps: `yugioh_domain::stats::kind`), then by English
//! name ignoring case and accents (`yugioh_domain::card::sort_name`), so the
//! copies of a card sit together whatever their language. The deck builder
//! sorts the same way (`frontend/src/deck.ts`).
//! A missing card (the collection lacks it) has the red border; a tile links
//! to an owned copy's page (the slot's copy in a built deck); its value is
//! the card's Cardmarket trend.
//! Around the cards: the version's format or ban list (its own, else the
//! latest one), named, with a badge on the restricted cards' tiles; its
//! opening hands (the deck builder's Monte Carlo on its categories,
//! `yugioh_domain::monte_carlo`, going first and second); and a link to the
//! last tournament it was played in. The rest is said once elsewhere: the
//! format's problems and the wants list in the builder's tabs, the rounds on
//! the tournament's page.
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
use yugioh_domain::card::{sort_name, CardStatus};
use yugioh_domain::monte_carlo;
use yugioh_domain::stats::{kind, CardKind};

use super::api::{simulation_inputs, zone_tiles, ApiCard};
use super::PUBLIC_VERSION;
use crate::routes::tournaments::TournamentView;
use crate::store::db::Pool;
use crate::store::models::{other_name, DeckVersionRow};
use crate::web::error::{AppError, AppResult, OrNotFound};
use crate::web::filters;
use crate::AppState;
use common::web::tiles::{CardShape, CardTile};

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
    /// The format played: `(id, "Edison v2")`.
    format: Option<(i64, String)>,
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

/// The format a version plays, `(id, label)`.
async fn format(pool: &Pool, id: i64) -> AppResult<Option<(i64, String)>> {
    let format: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT deck_format.id, deck_format.name, deck_format.version FROM deck_format
         JOIN deck_version ON deck_version.format_id = deck_format.id
         WHERE deck_version.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(format.map(|(format_id, name, version)| (format_id, format!("{name} v{version}"))))
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
    let mut conn = pool.acquire().await?;
    let tiles = zone_tiles(&mut conn, id).await?;
    drop(conn);
    let mut zones = Vec::new();
    for (cards, name) in tiles
        .into_iter()
        .zip(["Main Deck", "Extra Deck", "Side Deck"])
    {
        // (kind, English name, name shown): the same card's copies
        // together, the French ones after the English ones
        let mut cards: Vec<((CardKind, String, String), ApiCard)> = cards
            .into_iter()
            .map(|card| {
                let kind = card.card_type.as_deref().map_or(CardKind::Other, kind);
                let english = sort_name(card.name_en.as_deref().unwrap_or(&card.name));
                ((kind, english, sort_name(&card.name)), card)
            })
            .collect();
        cards.sort_by(|(a, _), (b, _)| a.cmp(b));
        let tiles = cards
            .iter()
            .map(|(_, card)| {
                let copy = card.copy.as_ref();
                let status = copy
                    .and_then(|c| c.status.as_deref())
                    .and_then(CardStatus::from_code);
                CardTile {
                    href: card
                        .sample_copy
                        .map(|id| format!("/cards/{id}"))
                        .unwrap_or_default(),
                    shape: CardShape::YuGiOh,
                    image: card.image.clone(),
                    name: card.name.clone(),
                    other_name: other_name(&card.name, card.name_en.as_deref())
                        .unwrap_or_default()
                        .to_string(),
                    print: copy.and_then(|c| c.code.clone()).unwrap_or_default(),
                    details: copy.and_then(|c| c.rarity.clone()).unwrap_or_default(),
                    condition: status.map(|s| (s.code(), s.label())),
                    tags: Vec::new(),
                    value: card
                        .cardmarket_trend
                        .map(|t| t.to_string())
                        .unwrap_or_default(),
                    faded: false,
                    missing: card.missing,
                    ban: card.card_data_id.and_then(|d| statuses.get(&d).copied()),
                }
            })
            .collect();
        zones.push(ZoneCards { name, tiles });
    }

    // the last tournament played with a list taken from this version
    let tournament_id: Option<i64> = sqlx::query_scalar(
        "SELECT tournament_id FROM deck_snapshot
         WHERE version_id = ? AND tournament_id IS NOT NULL
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let played = match tournament_id {
        Some(tournament_id) => TournamentView::load(pool, tournament_id).await?,
        None => None,
    };

    Ok(VersionPage {
        version,
        zones,
        ban_list: match format {
            Some(format) => Some(BanList {
                date: String::new(),
                chosen: true,
                format: Some(format),
            }),
            None => ban.map(|(_, date, chosen)| BanList {
                date,
                chosen,
                format: None,
            }),
        },
        hands: hands(pool, id).await?,
        played,
    })
}
