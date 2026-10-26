//! Market prices, refreshed weekly (`refresh-prices`, `commands::prices`):
//! - [`ygoprodeck`]: one price per card name (`card_price`);
//! - [`cardmarket`]: Cardmarket's price guide, a price per printing, and the
//!   links of the cards to their printing (`cardmarket_*`, `card.cardmarket_product_id`);
//! - [`history`]: both read back by date, for the valuation.

pub mod cardmarket;
pub mod history;
pub mod ygoprodeck;
