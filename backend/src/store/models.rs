//! Rows shared by several pages, with the display helpers the templates use.

use domain::card::CardStatus;

use crate::store::db::Money;
use crate::store::valuation::{estimate, MARKET_PRICE};

/// A card as shown in lists.
#[derive(Debug, sqlx::FromRow)]
pub struct CardRow {
    pub id: i64,
    pub name: String,
    pub code: Option<String>,
    pub last_known_status: Option<String>,
    pub rarity: Option<String>,
    pub is_proxy: bool,
    pub card_data_id: Option<i64>,
    /// The English name as ygoprodeck writes it ("Rainbow Bridge Bifrost").
    pub name_en: Option<String>,
    /// Its latest market price (`valuation::MARKET_PRICE`).
    pub market_price: Option<Money>,
}

impl CardRow {
    /// The columns to select from `card` (a `format!` of `MARKET_PRICE`).
    pub fn columns() -> String {
        format!(
            "card.id, card.name, card.code, card.last_known_status, card.rarity, card.is_proxy, \
             card.card_data_id, \
             (SELECT json_extract(card_data.json_data, '$.name') FROM card_data \
              WHERE card_data.id = card.card_data_id) AS name_en, \
             {MARKET_PRICE} AS market_price"
        )
    }

    /// Its estimated value: market price × condition and language factors.
    pub fn estimate(&self) -> Option<Money> {
        estimate(
            self.market_price,
            self.last_known_status.as_deref(),
            self.code.as_deref(),
        )
    }

    /// The English name, when it isn't the name already shown.
    pub fn english_name(&self) -> Option<&str> {
        other_name(&self.name, self.name_en.as_deref())
    }

    pub fn status(&self) -> Option<CardStatus> {
        self.last_known_status
            .as_deref()
            .and_then(CardStatus::from_code)
    }
}

/// `english` when it differs from `name` (ignoring case): a French card shows
/// its English name too, an English card doesn't repeat it.
pub fn other_name<'a>(name: &str, english: Option<&'a str>) -> Option<&'a str> {
    english.filter(|en| !en.eq_ignore_ascii_case(name.trim()))
}

/// A lot as shown in lists.
#[derive(Debug, sqlx::FromRow)]
pub struct LotRow {
    pub id: i64,
    pub lot_type: String,
    pub price: Money,
    pub buy_date: String,
    pub received_date: Option<String>,
    pub seller_id: Option<i64>,
    pub seller: Option<String>,
    pub shipment_file: Option<String>,
    pub card_count: i64,
    /// Estimated value of its cards still owned (purchase lots), filled by
    /// `valuation::fill_lot_estimates`.
    #[sqlx(skip)]
    pub estimate: Option<Money>,
}

impl LotRow {
    pub const SELECT: &'static str = "SELECT lot.id, lot.lot_type, lot.price, lot.buy_date,
            lot.received_date, lot.seller_id, seller.name AS seller, lot.shipment_file,
            (SELECT count(*) FROM unite WHERE unite.lot_id = lot.id) AS card_count
        FROM lot LEFT JOIN seller ON seller.id = lot.seller_id";

    pub fn seller_name(&self) -> &str {
        self.seller.as_deref().unwrap_or("No Seller")
    }

    pub fn title(&self) -> String {
        format!("Lot {} - {}", self.buy_date, self.seller_name())
    }
}

/// A deck version as shown in lists.
#[derive(Debug, sqlx::FromRow)]
pub struct DeckVersionRow {
    pub id: i64,
    pub version_name: String,
    pub deck_name: Option<String>,
}

impl DeckVersionRow {
    pub const SELECT: &'static str = "SELECT deck_version.id, deck_version.version_name,
            deck.name AS deck_name
        FROM deck_version LEFT JOIN deck ON deck.id = deck_version.deck_id";

    /// "Deck - version", as Django's `DeckVersion.name`.
    pub fn full_name(&self) -> String {
        version_full_name(self.deck_name.as_deref(), &self.version_name)
    }
}

/// "Deck - version" (Django's `DeckVersion.name`), "No Deck - …" for a
/// version without a deck.
pub fn version_full_name(deck_name: Option<&str>, version_name: &str) -> String {
    format!("{} - {version_name}", deck_name.unwrap_or("No Deck"))
}

#[cfg(test)]
mod tests {
    use super::other_name;

    #[test]
    fn english_name_only_when_it_differs() {
        assert_eq!(
            other_name("Dragon à Trois Cornes", Some("Tri-Horned Dragon")),
            Some("Tri-Horned Dragon")
        );
        assert_eq!(other_name("Polymerization", Some("Polymerization")), None);
        assert_eq!(other_name("polymerization ", Some("Polymerization")), None);
        assert_eq!(other_name("Kuriboh", None), None);
    }
}
