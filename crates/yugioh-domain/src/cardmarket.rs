//! Cardmarket shipment parsing (Django: `shipment_reader/cardmarket.py`).
//!
//! Works on the text of the shipment PDF's first page. That text contains
//! personal data (names, addresses): parse it in memory and keep only the
//! returned fields — never store, log or embed the text itself.
//!
//! Differences from Django: the account name is a parameter instead of a
//! constant, a missing total is an error instead of a crash, Cardmarket
//! conditions are mapped to [`CardStatus`], the rarity is read (from the
//! rarity code of the line, else from the `(V.x - …)` suffix of the name,
//! which is stripped), and the six TCG languages are read (Django: English
//! and French only).

use std::fmt;
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use rust_decimal::Decimal;

use crate::card::{CardStatus, Language};
use crate::lot::LotType;
use crate::rarity::{split_cardmarket_name, Rarity};

#[derive(Debug, Clone, PartialEq)]
pub struct Shipment {
    /// Purchase when someone else is the seller, sale when we are.
    pub lot_type: LotType,
    /// The other party (seller of a purchase, buyer of a sale), if found.
    pub counterparty: Option<String>,
    pub paid: Option<NaiveDate>,
    pub arrived: Option<NaiveDate>,
    pub total: Decimal,
    /// One entry per physical card (quantities are expanded).
    pub cards: Vec<ShipmentCard>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShipmentCard {
    /// Without Cardmarket's `(V.x - <rarity>)` suffix.
    pub name: String,
    /// e.g. `RA03-EN012`
    pub code: String,
    pub rarity: Option<Rarity>,
    /// The rarity code of the line as written (`UR`…), to report unknown ones.
    pub rarity_code: Option<String>,
    /// Raw Cardmarket condition (MT, NM, EX, GD, LP, PL, PO).
    pub condition: String,
    pub price: Decimal,
}

impl ShipmentCard {
    pub fn status(&self) -> Option<CardStatus> {
        match self.condition.as_str() {
            "GD" => Some(CardStatus::Good),
            "PO" => Some(CardStatus::Poor),
            other => CardStatus::from_code(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShipmentError {
    UnreadablePdf,
    MissingTotal,
}

impl fmt::Display for ShipmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShipmentError::UnreadablePdf => f.write_str("could not read the shipment PDF"),
            ShipmentError::MissingTotal => f.write_str("no total price found in the shipment"),
        }
    }
}

impl std::error::Error for ShipmentError {}

static SELLER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Seller\s*-\s*(.+)").unwrap());
static BUYER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Buyer\s*-\s*(.+)").unwrap());
static PAID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Paid:\s*(\d{2}\.\d{2}\.\d{4})").unwrap());
static ARRIVED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Arrived\s+(\d{2}\.\d{2}\.\d{4})").unwrap());
static TOTAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Total\s+([\d,]+)\s+EUR").unwrap());
// Matched against the whole singles section joined on one line: depending on
// the PDF library a card is one line (pypdf) or one line per cell (lopdf).
// `lang` is any spelling of a [`Language`].
static CARD: LazyLock<Regex> = LazyLock::new(|| {
    let languages: Vec<&str> = Language::ALL
        .iter()
        .flat_map(|l| l.spellings().iter().copied())
        .collect();
    Regex::new(&format!(
        r"(?P<qty>\d+)\s+(?P<name>.+?)\s+X?(?P<code>[0-9a-zA-Z]{{3}})\s+(?P<lang>{})\s+(?P<status>[A-Z]{{2}})\s+(?P<set>[A-Z0-9]+)\s+(?:(?P<rarity>[0-9A-Za-z]+)\s+)?.*?(?P<price>[\d,]+)\s+EUR",
        languages.join("|")
    ))
    .unwrap()
});

const SINGLES_HEADER: &str = "Yugioh Singles:";

/// Parse a shipment PDF (only its first page is read, like Django).
pub fn parse_shipment_pdf(pdf: &[u8], account: &str) -> Result<Shipment, ShipmentError> {
    let doc = lopdf::Document::load_mem(pdf).map_err(|_| ShipmentError::UnreadablePdf)?;
    let text = doc
        .extract_text(&[1])
        .map_err(|_| ShipmentError::UnreadablePdf)?;
    parse_shipment(&text, account)
}

/// Parse the text of a shipment. `account` is our own Cardmarket username,
/// used to tell purchases from sales.
pub fn parse_shipment(text: &str, account: &str) -> Result<Shipment, ShipmentError> {
    let capture = |re: &Regex| {
        re.captures(text)
            .map(|c| c[1].trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let seller = capture(&SELLER);
    let buyer = capture(&BUYER);

    let (lot_type, counterparty) = if seller.as_deref() == Some(account) {
        (LotType::Sale, buyer)
    } else {
        (LotType::Purchase, seller)
    };

    let date =
        |re: &Regex| capture(re).and_then(|d| NaiveDate::parse_from_str(&d, "%d.%m.%Y").ok());

    let total = capture(&TOTAL)
        .and_then(|t| parse_eur(&t))
        .ok_or(ShipmentError::MissingTotal)?;

    Ok(Shipment {
        lot_type,
        counterparty,
        paid: date(&PAID),
        arrived: date(&ARRIVED),
        total,
        cards: parse_cards(text),
    })
}

fn parse_cards(text: &str) -> Vec<ShipmentCard> {
    let Some((_, singles)) = text.split_once(SINGLES_HEADER) else {
        return Vec::new();
    };
    let singles = singles.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cards = Vec::new();
    for c in CARD.captures_iter(&singles) {
        let (Ok(qty), Some(price), Some(language)) = (
            c["qty"].parse::<usize>(),
            parse_eur(&c["price"]),
            Language::from_code(&c["lang"]),
        ) else {
            continue;
        };
        let (name, suffix_rarity) = split_cardmarket_name(c["name"].trim());
        let rarity_code = c.name("rarity").map(|m| m.as_str().to_string());
        let card = ShipmentCard {
            name: name.to_string(),
            // the card code spelling of the language: "ES" → "SP"
            code: format!("{}-{}{}", &c["set"], language.code(), &c["code"]),
            rarity: rarity_code
                .as_deref()
                .and_then(Rarity::from_cardmarket_code)
                .or(suffix_rarity),
            rarity_code,
            condition: c["status"].to_string(),
            price,
        };
        cards.extend(std::iter::repeat_n(card, qty));
    }
    cards
}

/// "1,50" -> 1.50
fn parse_eur(s: &str) -> Option<Decimal> {
    s.replace(',', ".").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // Synthetic text shaped like a shipment page. Real shipment PDFs contain
    // personal data: never paste their text here.
    const PURCHASE: &str = "\
Shipment #1
Seller - Example Shop
Buyer - me_account
Paid: 12.06.2025
Arrived 19.06.2025
Total 18,17 EUR
Yugioh Singles:
2 Ash Blossom & Joyous Spring 012 EN NM RA03 SER 1,50 EUR
1 Dark Magician X001 FR GD LOB UR First Edition 3,00 EUR
not a card line
";

    #[test]
    fn parses_purchase() {
        let s = parse_shipment(PURCHASE, "me_account").unwrap();
        assert_eq!(s.lot_type, LotType::Purchase);
        assert_eq!(s.counterparty.as_deref(), Some("Example Shop"));
        assert_eq!(s.paid, NaiveDate::from_ymd_opt(2025, 6, 12));
        assert_eq!(s.arrived, NaiveDate::from_ymd_opt(2025, 6, 19));
        assert_eq!(s.total, dec!(18.17));

        assert_eq!(s.cards.len(), 3);
        assert_eq!(s.cards[0], s.cards[1]);
        assert_eq!(s.cards[0].name, "Ash Blossom & Joyous Spring");
        assert_eq!(s.cards[0].code, "RA03-EN012");
        assert_eq!(s.cards[0].price, dec!(1.50));
        assert_eq!(s.cards[0].status(), Some(CardStatus::NearMint));
        assert_eq!(s.cards[0].rarity, Some(Rarity::SecretRare));
        assert_eq!(s.cards[2].code, "LOB-FR001");
        assert_eq!(s.cards[2].rarity, Some(Rarity::UltraRare));
        assert_eq!(s.cards[2].status(), Some(CardStatus::Good));
    }

    #[test]
    fn parses_one_cell_per_line_layout() {
        let text = "Total 3,00 EUR\nYugioh Singles:\n1\nDark Magician (V.1 - Ultra Rare)\n001\nEN\nNM\nLOB\nUR\nFirst Edition\n1,00 EUR\n2\nKuriboh\n123\nFR\nEX\nMRD1\nC\nUnlimited\n1,00 EUR\n";
        let s = parse_shipment(text, "me").unwrap();
        assert_eq!(s.cards.len(), 3);
        assert_eq!(s.cards[0].name, "Dark Magician");
        assert_eq!(s.cards[0].code, "LOB-EN001");
        assert_eq!(s.cards[0].rarity, Some(Rarity::UltraRare));
        assert_eq!(s.cards[2].code, "MRD1-FR123");
        assert_eq!(s.cards[2].rarity, Some(Rarity::Common));
        assert_eq!(s.cards[2].price, dec!(1.00));
    }

    #[test]
    fn rarity_from_the_name_suffix_when_the_code_is_unknown() {
        let text = "Total 1,00 EUR\nYugioh Singles:\n1 Raigeki (V.4 - Ultra Rare) 012 EN NM RA03 XYZ 1,00 EUR\n";
        let s = parse_shipment(text, "me").unwrap();
        assert_eq!(s.cards[0].name, "Raigeki");
        assert_eq!(s.cards[0].rarity_code.as_deref(), Some("XYZ"));
        assert_eq!(s.cards[0].rarity, Some(Rarity::UltraRare));
    }

    #[test]
    fn every_language_is_read() {
        let text = "Total 3,00 EUR\nYugioh Singles:\n\
            1 Schwarzer Magier 001 DE NM LOB UR 1,00 EUR\n\
            1 Mago Oscuro 001 ES EX LOB UR 1,00 EUR\n\
            1 Mago Nero 001 IT GD LOB UR 1,00 EUR\n";
        let s = parse_shipment(text, "me").unwrap();
        let codes: Vec<&str> = s.cards.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, ["LOB-DE001", "LOB-SP001", "LOB-IT001"]);
    }

    #[test]
    fn unknown_language_lines_are_skipped() {
        let text = "Total 1,00 EUR\nYugioh Singles:\n1 Something 001 XX NM LOB UR 1,00 EUR\n";
        assert!(parse_shipment(text, "me").unwrap().cards.is_empty());
    }

    #[test]
    fn line_without_a_rarity_code() {
        let text = "Total 1,00 EUR\nYugioh Singles:\n1 Raigeki 012 EN NM RA03 1,00 EUR\n";
        let s = parse_shipment(text, "me").unwrap();
        assert_eq!(s.cards.len(), 1);
        assert_eq!(s.cards[0].rarity_code, None);
        assert_eq!(s.cards[0].price, rust_decimal_macros::dec!(1.00));
    }

    #[test]
    fn parses_sale() {
        let text = "Seller - me_account\nBuyer - Someone\nPaid: 01.02.2025\nTotal 4,00 EUR\n";
        let s = parse_shipment(text, "me_account").unwrap();
        assert_eq!(s.lot_type, LotType::Sale);
        assert_eq!(s.counterparty.as_deref(), Some("Someone"));
        assert_eq!(s.arrived, None);
        assert!(s.cards.is_empty());
    }

    #[test]
    fn missing_total_is_an_error() {
        assert_eq!(
            parse_shipment("Seller - x\n", "me"),
            Err(ShipmentError::MissingTotal)
        );
    }
}
