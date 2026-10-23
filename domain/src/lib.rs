//! Business logic ported from the Django app (tag `django-final`, `my_ygo_cards/`).
//!
//! Everything here is pure (no database, no HTTP) so it can be shared by the
//! backend, the importer and maintenance commands, and unit-tested directly.

pub mod ban_list;
pub mod book;
pub mod budget;
pub mod built_deck;
pub mod card;
pub mod card_set;
pub mod cardmarket;
pub mod cardmarket_catalog;
pub mod isbn;
pub mod lot;
pub mod monte_carlo;
pub mod pokemon;
pub mod rarity;
pub mod tournament;
pub mod valuation;
pub mod ydke;
