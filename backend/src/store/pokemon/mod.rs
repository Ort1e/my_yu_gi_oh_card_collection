//! The Pokémon collection's data (routes `/pokemon`, migration 0012):
//! - `tcgdex.rs`: the TCGdex client (every call goes through it);
//! - `cards.rs`: TCGdex's cards, remembered in `pokemon_card` (owned or
//!   wished cards only), and the copy and wish writes;
//! - `sets.rs`: sets by printed code (`FNE`), remembered in `pokemon_set`;
//! - `prices.rs`: the weekly Cardmarket prices in `pokemon_price`;
//! - `openings.rs`: booster openings (`pokemon_opening`);
//! - `ptcgio.rs`: the Pokémon TCG API, for the pictures TCGdex lacks.
//!
//! Network calls happen before any write transaction (CLAUDE.md): the
//! lookups write their cache through the pool.

pub mod cards;
pub mod openings;
pub mod prices;
pub mod ptcgio;
pub mod sets;
pub mod tcgdex;

/// Whether a `YYYY-MM-DD` date is less than `days` old.
fn is_fresh(date: &str, days: i64) -> bool {
    date.parse::<chrono::NaiveDate>()
        .is_ok_and(|d| (crate::store::db::today() - d).num_days() < days)
}
