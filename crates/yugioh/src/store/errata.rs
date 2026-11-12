//! Errata of the cards of a deck version (`yugioh_domain::errata`): which
//! of its missing cards, and of a built version's copies, have another text
//! than the one in force when the version's format or ban list applied. Used by
//! `/decks/built` and the deck builder's `…/check`.
//!
//! The cutoff of a version is its format's, else its ban list's date (the
//! date it is played at); a version with neither has none: no warning. The
//! errata come from the cache ([`cached_errata`]); the missing ones are
//! asked in the background ([`spawn_errata`]) and counted as pending, so a
//! page never waits for Yugipedia.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use common::web::filters::french_date;
use serde_json::Value;
use sqlx::QueryBuilder;
use yugioh_domain::errata::{self, Errata, SetDates, TextVersion};

use super::db::{push_ids, Pool};
use super::lists;
use super::yugipedia::{cached_errata, spawn_errata};
use crate::web::error::{AppError, AppResult};
use crate::AppState;
use yugioh_domain::ownership::MissingLine;

/// Printings offered at most in a warning (the card data can list dozens).
const SHOWN_PRINTINGS: usize = 8;

/// What a version is played at: its format (`Edison v1`) or its ban list
/// (`ban list 01/03/2010`), and the date its cards' texts count at.
#[derive(Debug, Clone)]
pub struct Context {
    pub label: String,
    pub cutoff: NaiveDate,
}

#[derive(sqlx::FromRow)]
struct ContextRow {
    id: i64,
    name: Option<String>,
    version: Option<i64>,
    cutoff: Option<String>,
    ban_date: Option<String>,
}

/// The contexts of these versions (those with a format or a ban list).
pub async fn contexts(pool: &Pool, version_ids: &[i64]) -> AppResult<HashMap<i64, Context>> {
    if version_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut qb = QueryBuilder::new(
        "SELECT deck_version.id AS id, deck_format.name AS name, deck_format.version AS version, deck_format.cutoff AS cutoff,
                advanced_ban_list.date AS ban_date
         FROM deck_version
         LEFT JOIN deck_format ON deck_format.id = deck_version.format_id
         LEFT JOIN advanced_ban_list ON advanced_ban_list.id = deck_version.ban_list_id
         WHERE deck_version.id IN ",
    );
    push_ids(&mut qb, version_ids.iter().copied());
    let rows: Vec<ContextRow> = qb.build_query_as().fetch_all(pool).await?;

    let bad = |id: i64, date: &str| AppError::Internal(format!("version {id}: date {date:?}"));
    let mut found = HashMap::new();
    for ContextRow {
        id,
        name,
        version,
        cutoff,
        ban_date,
    } in rows
    {
        let context = match (name, version, cutoff, ban_date) {
            (Some(name), Some(version), Some(cutoff), _) => Context {
                label: format!("{name} v{version}"),
                cutoff: cutoff.parse().map_err(|_| bad(id, &cutoff))?,
            },
            (_, _, _, Some(date)) => Context {
                label: format!("ban list {}", french_date(&date)),
                cutoff: date.parse().map_err(|_| bad(id, &date))?,
            },
            _ => continue,
        };
        found.insert(id, context);
    }
    Ok(found)
}

/// A card of a version as the errata need it: a built version's copy, or a
/// missing one (to buy).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardRef {
    pub card_data_id: Option<i64>,
    pub name: String,
    /// The copy's code (`''` for a missing one).
    pub code: String,
    /// Groups the copies of a card (its card data's `en_name`, else the name).
    pub key: String,
    /// The English name as written on the card data (`en_name` is lowercase).
    pub english_name: String,
    pub missing: bool,
}

/// A version's cards for the errata: a built version's copies, then a card
/// per missing copy (`lists::missing`); a plain version's owned cards have
/// no copy to judge.
pub async fn version_cards(
    conn: &mut sqlx::SqliteConnection,
    version_id: i64,
) -> sqlx::Result<Vec<CardRef>> {
    let lines = lists::lines(conn, version_id).await?;
    let short = lists::missing(conn, version_id, &lines).await?;
    version_cards_missing(conn, version_id, short).await
}

/// [`version_cards`] with the missing copies given (the shopping list's,
/// `ownership::missing_for_all`).
pub async fn version_cards_missing(
    conn: &mut sqlx::SqliteConnection,
    version_id: i64,
    short: Vec<MissingLine>,
) -> sqlx::Result<Vec<CardRef>> {
    let mut cards: Vec<CardRef> = sqlx::query_as(
        "SELECT card.card_data_id, card.name, coalesce(card.code, '') AS code,
                coalesce(card_data.en_name, card.en_name, card.name) AS key,
                coalesce(json_extract(card_data.json_data, '$.name'), card.en_name, card.name)
                    AS english_name,
                0 AS missing
         FROM deck_version_copy JOIN card ON card.id = deck_version_copy.card_id
         LEFT JOIN card_data ON card_data.id = card.card_data_id
         WHERE deck_version_copy.deck_version_id = ?
         ORDER BY card.name",
    )
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;

    let ids: Vec<i64> = short.iter().filter_map(|m| m.card_data_id).collect();
    let mut names: HashMap<i64, (String, String)> = HashMap::new();
    if !ids.is_empty() {
        let mut qb = QueryBuilder::new(
            "SELECT id, en_name, coalesce(json_extract(json_data, '$.name'), en_name)
             FROM card_data WHERE id IN ",
        );
        push_ids(&mut qb, ids);
        let rows: Vec<(i64, String, String)> = qb.build_query_as().fetch_all(&mut *conn).await?;
        names.extend(
            rows.into_iter()
                .map(|(id, key, english)| (id, (key, english))),
        );
    }
    for m in short {
        let (key, english_name) = m
            .card_data_id
            .and_then(|id| names.get(&id).cloned())
            .unwrap_or_else(|| (m.name.to_lowercase(), m.name.clone()));
        let card = CardRef {
            card_data_id: m.card_data_id,
            name: m.name,
            code: String::new(),
            key,
            english_name,
            missing: true,
        };
        cards.extend(std::iter::repeat_n(card, m.count as usize));
    }
    Ok(cards)
}

/// A card whose text was errata'd after the context's cutoff: buying the
/// wrong printing gets the wrong text.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ErrataWarning {
    /// `Edison v1`.
    pub format: String,
    /// The set of the first change after the cutoff.
    pub changed_in: String,
    /// The printings with the text in force at the cutoff, most recent
    /// first, as text.
    pub right_sets: String,
    /// The text at the cutoff.
    pub old_text: String,
    /// Today's text.
    pub new_text: String,
}

/// A card missing from a version: its missing copies, plus a built
/// version's copies with the wrong text.
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct MissingCard {
    pub name: String,
    /// The name Cardmarket knows the card by.
    pub english_name: String,
    /// Missing and wrong-text copies together.
    pub count: usize,
    /// The codes of the owned copies with the wrong text.
    pub wrong_text: Vec<String>,
    pub warning: Option<ErrataWarning>,
}

/// The errata of some cards in their contexts.
pub struct Verdicts {
    /// `(context label, card data id)` → errata and warning.
    found: HashMap<(String, i64), (Errata, ErrataWarning)>,
    dates: SetDates,
    /// Cards whose errata are being looked up on Yugipedia.
    pub pending: usize,
}

/// The missing and the wrong-text copies of a version, by card key.
pub struct Missing {
    pub lines: BTreeMap<String, MissingCard>,
    /// Real copies with the wrong text.
    pub wrong_text: usize,
}

impl Verdicts {
    /// Look up the errata of these cards `(context, card)`: cached ones
    /// are judged, the others asked in the background.
    pub async fn load(state: &AppState, cards: &[(&Context, &CardRef)]) -> AppResult<Verdicts> {
        let pool = &state.pool;
        let none = || Verdicts {
            found: HashMap::new(),
            dates: SetDates::default(),
            pending: 0,
        };
        if cards.is_empty() {
            return Ok(none());
        }

        let sets: Vec<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT name, code, tcg_date FROM ygo_set")
                .fetch_all(pool)
                .await?;
        let dates = SetDates::new(
            sets.into_iter()
                .map(|(name, code, date)| (name, code, date.and_then(|d| d.parse().ok()))),
        );

        // the cards to look up: the missing ones, and the copies whose set
        // has a date
        let wanted: BTreeSet<i64> = cards
            .iter()
            .filter_map(|(_, c)| Some((c, c.card_data_id?)))
            .filter(|(c, _)| c.missing || dates.of_code(&c.code).is_some())
            .map(|(_, id)| id)
            .collect();
        let mut known: HashMap<i64, Vec<TextVersion>> = HashMap::new();
        let mut pending = Vec::new();
        for id in wanted {
            match cached_errata(pool, id).await? {
                Some(Some(versions)) => {
                    known.insert(id, versions);
                }
                Some(None) => {}
                None => pending.push(id),
            }
        }
        let count = pending.len();
        spawn_errata(state.clone(), pending);

        let ids: Vec<i64> = known.keys().copied().collect();
        let printings = printings(pool, &ids).await?;

        let mut found = HashMap::new();
        for (context, card) in cards {
            let Some(id) = card.card_data_id else {
                continue;
            };
            let key = (context.label.clone(), id);
            let (Some(versions), false) = (known.get(&id), found.contains_key(&key)) else {
                continue;
            };
            let verdict = judge(
                versions,
                &dates,
                printings.get(&id).map_or(&[], Vec::as_slice),
                context,
            );
            if let Some(verdict) = verdict {
                found.insert(key, verdict);
            }
        }
        Ok(Verdicts {
            found,
            dates,
            pending: count,
        })
    }

    fn of(&self, context: &Context, card: &CardRef) -> Option<&(Errata, ErrataWarning)> {
        self.found.get(&(context.label.clone(), card.card_data_id?))
    }

    /// The missing cards among a version's cards: the missing ones and the
    /// copies whose printing has another text than the one in force.
    pub fn missing<'a>(
        &self,
        context: Option<&Context>,
        cards: impl IntoIterator<Item = &'a CardRef>,
    ) -> Missing {
        let mut lines: BTreeMap<String, MissingCard> = BTreeMap::new();
        let mut wrong_text = 0;
        for card in cards {
            let judged = context.and_then(|c| self.of(c, card));
            let wrong = !card.missing
                && judged.is_some_and(|(errata, _)| {
                    errata::copy_is_right(&card.code, &self.dates, errata) == Some(false)
                });
            if !card.missing && !wrong {
                continue;
            }

            let line = lines
                .entry(card.key.clone())
                .or_insert_with(|| MissingCard {
                    name: card.name.clone(),
                    english_name: card.english_name.clone(),
                    count: 0,
                    wrong_text: Vec::new(),
                    warning: None,
                });
            line.count += 1;
            if wrong {
                wrong_text += 1;
                line.wrong_text.push(card.code.clone());
            }
            if line.warning.is_none() {
                line.warning = judged.map(|(_, warning)| warning.clone());
            }
        }
        Missing { lines, wrong_text }
    }
}

/// A card data's printings `(code, set name)`, from its `card_sets`.
async fn printings(pool: &Pool, ids: &[i64]) -> AppResult<HashMap<i64, Vec<(String, String)>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut qb = QueryBuilder::new("SELECT id, json_data FROM card_data WHERE id IN ");
    push_ids(&mut qb, ids.iter().copied());
    let rows: Vec<(i64, String)> = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(id, json)| {
            let card: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
            (id, errata::card_printings(&card))
        })
        .collect())
}

/// The warning for a card in a context: its errata at the cutoff and the
/// printings to buy.
fn judge(
    versions: &[TextVersion],
    dates: &SetDates,
    printings: &[(String, String)],
    context: &Context,
) -> Option<(Errata, ErrataWarning)> {
    let dated: Vec<_> = versions.iter().map(|v| dates.of_version(v)).collect();
    let errata = errata::at_cutoff(versions, &dated, context.cutoff)?;

    let right = errata::right_printings(printings, dates, &errata);
    let mut right_sets: Vec<String> = right
        .iter()
        .take(SHOWN_PRINTINGS)
        .map(|p| format!("{} ({})", p.set, p.code))
        .collect();
    if right.len() > SHOWN_PRINTINGS {
        right_sets.push(format!("{} more", right.len() - SHOWN_PRINTINGS));
    }
    let right_sets = if right_sets.is_empty() {
        "no printing of the card data has it: check on Yugipedia".to_string()
    } else {
        right_sets.join(", ")
    };

    let warning = ErrataWarning {
        format: context.label.clone(),
        changed_in: errata.changed.set.clone(),
        right_sets,
        old_text: errata.in_force.text.clone(),
        new_text: errata.latest.text.clone(),
    };
    Some((errata, warning))
}
