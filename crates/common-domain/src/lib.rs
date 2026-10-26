//! Pure rules shared by the card collections (Yu-Gi-Oh!, Pokémon): a card's
//! condition and what it does to its value. No database, no HTTP.

use rust_decimal::Decimal;

/// A card's condition (`card.last_known_status`, `pokemon_copy.condition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CardStatus {
    #[serde(rename = "MT")]
    Mint,
    #[serde(rename = "NM")]
    NearMint,
    #[serde(rename = "EX")]
    Excellent,
    #[serde(rename = "G")]
    Good,
    #[serde(rename = "P")]
    Poor,
}

impl CardStatus {
    pub const ALL: [CardStatus; 5] = [
        CardStatus::Mint,
        CardStatus::NearMint,
        CardStatus::Excellent,
        CardStatus::Good,
        CardStatus::Poor,
    ];

    /// Value stored in the database.
    pub fn code(self) -> &'static str {
        match self {
            CardStatus::Mint => "MT",
            CardStatus::NearMint => "NM",
            CardStatus::Excellent => "EX",
            CardStatus::Good => "G",
            CardStatus::Poor => "P",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CardStatus::Mint => "Mint",
            CardStatus::NearMint => "Near Mint",
            CardStatus::Excellent => "Excellent",
            CardStatus::Good => "Good",
            CardStatus::Poor => "Poor",
        }
    }

    pub fn from_code(code: &str) -> Option<CardStatus> {
        CardStatus::ALL.into_iter().find(|s| s.code() == code)
    }
}

/// Condition factors (MT, NM and unknown: 1).
pub const EXCELLENT_FACTOR: Decimal = Decimal::from_parts(8, 0, 0, false, 1);
pub const GOOD_FACTOR: Decimal = Decimal::from_parts(6, 0, 0, false, 1);
pub const POOR_FACTOR: Decimal = Decimal::from_parts(4, 0, 0, false, 1);

/// What a copy's condition does to its market price.
pub fn condition_factor(status: Option<CardStatus>) -> Decimal {
    match status {
        Some(CardStatus::Excellent) => EXCELLENT_FACTOR,
        Some(CardStatus::Good) => GOOD_FACTOR,
        Some(CardStatus::Poor) => POOR_FACTOR,
        Some(CardStatus::Mint | CardStatus::NearMint) | None => Decimal::ONE,
    }
}
