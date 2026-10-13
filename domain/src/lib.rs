//! Business logic ported from the Django app (tag `django-final`, `my_ygo_cards/`).
//!
//! Everything here is pure (no database, no HTTP) so it can be shared by the
//! backend, the importer and maintenance commands, and unit-tested directly.

pub mod ban_list;
pub mod budget;
pub mod card;
pub mod cardmarket;
pub mod lot;
pub mod monte_carlo;
pub mod ydke;
