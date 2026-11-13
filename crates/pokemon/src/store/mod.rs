//! The Pokémon collection's data (routes `/pokemon`, migration 0012):
//! - `tcgdex.rs`: the TCGdex client (every call goes through it);
//! - `cards.rs`: TCGdex's cards, remembered in `pokemon_card` (owned or
//!   wished cards only), and the copy and wish writes;
//! - `sets.rs`: sets by printed code (`FNE`), remembered in `pokemon_set`;
//! - `prices.rs`: the weekly Cardmarket prices in `pokemon_price`;
//! - `boosters.rs`: each card's Cardmarket expansion and its booster's price
//!   (the wishlist's suggestions), weekly too;
//! - `openings.rs`: booster openings (`pokemon_opening`);
//! - `limitless.rs`: Limitless TCG's official scans, first fallback for pictures;
//! - `ptcgio.rs`: the Pokémon TCG API, for the pictures TCGdex lacks.
//!
//! Network calls happen before any write transaction (CLAUDE.md): the
//! lookups write their cache through the pool.

pub mod boosters;
pub mod cards;
pub mod limitless;
pub mod openings;
pub mod prices;
pub mod ptcgio;
pub mod sets;
pub mod tcgdex;
