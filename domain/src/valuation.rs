//! What the collection is worth at market prices, compared with what it cost.
//!
//! Prices are ygoprodeck's `cardmarket_price` (EUR), **one per card name**:
//! it follows the cheapest printing, so a value is an estimate, usually too
//! low for old or high rarity copies. They are refreshed weekly into
//! `card_price` (one row per card data and refresh date), so the value can be
//! followed over time.
//!
//! - A copy's value is its card's price × a condition factor × a language
//!   factor ([`copy_value`]); totals are rounded to the cent at the end.
//! - A copy counts on a date when it was bought on or before it and not sold
//!   by then ([`Copy::is_owned_on`]).
//! - The gain/loss uses the **lot prices** (`lot.price`, what was paid, with
//!   the shipping), not the unite prices: many lots were bought as a whole,
//!   without a price per card. On a date it is the value of the copies owned
//!   then + what the sale lots brought − what the purchase lots cost, all
//!   lots up to that date ([`Totals`]).
//! - The copies that have a unite price are also compared one by one: value
//!   against that price ([`UnitTotals`]; no shipping share).
//!
//! New in the Rust app (Django had no valuation). Deliberate choice: every
//! language but English gets the French factor (the collection only has
//! English and French copies).

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::card::{CardStatus, Language};

/// Condition factors (MT, NM and unknown: 1).
pub const EXCELLENT_FACTOR: Decimal = Decimal::from_parts(8, 0, 0, false, 1);
pub const GOOD_FACTOR: Decimal = Decimal::from_parts(6, 0, 0, false, 1);
pub const POOR_FACTOR: Decimal = Decimal::from_parts(4, 0, 0, false, 1);
/// Language factor of non-English copies (English and unknown: 1).
pub const NON_ENGLISH_FACTOR: Decimal = Decimal::from_parts(9, 0, 0, false, 1);

pub fn condition_factor(status: Option<CardStatus>) -> Decimal {
    match status {
        Some(CardStatus::Excellent) => EXCELLENT_FACTOR,
        Some(CardStatus::Good) => GOOD_FACTOR,
        Some(CardStatus::Poor) => POOR_FACTOR,
        Some(CardStatus::Mint | CardStatus::NearMint) | None => Decimal::ONE,
    }
}

/// `language`: from the copy's code (`Language::of_code`), `None` for old
/// codes without one (English sets).
pub fn language_factor(language: Option<Language>) -> Decimal {
    match language {
        Some(Language::English) | None => Decimal::ONE,
        Some(_) => NON_ENGLISH_FACTOR,
    }
}

/// Market value of one copy, not rounded (round the totals).
pub fn copy_value(
    price: Decimal,
    status: Option<CardStatus>,
    language: Option<Language>,
) -> Decimal {
    price * condition_factor(status) * language_factor(language)
}

/// One owned (or once owned) copy: a non-proxy card with a purchase lot that
/// isn't cancelled.
#[derive(Debug, Clone, PartialEq)]
pub struct Copy {
    pub card_data_id: Option<i64>,
    pub status: Option<CardStatus>,
    pub language: Option<Language>,
    /// `buy_date` of its purchase lot.
    pub bought: NaiveDate,
    /// `buy_date` of its sale lot (not cancelled), if sold.
    pub sold: Option<NaiveDate>,
    /// Its `unite.price` in its purchase lot, `None` when the lot was bought
    /// as a whole.
    pub paid: Option<Decimal>,
}

impl Copy {
    /// Owned at the end of `date`: bought on or before it, not sold by then.
    pub fn is_owned_on(&self, date: NaiveDate) -> bool {
        self.bought <= date && self.sold.is_none_or(|sold| sold > date)
    }

    /// Its value with `prices` (by card data id), `None` without a price.
    pub fn value(&self, prices: &HashMap<i64, Decimal>) -> Option<Decimal> {
        let price = prices.get(&self.card_data_id?)?;
        Some(copy_value(*price, self.status, self.language))
    }
}

/// A lot that isn't cancelled: `price` paid (purchase) or received (sale).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lot {
    pub date: NaiveDate,
    pub is_sale: bool,
    pub price: Decimal,
}

/// The collection on a date: the copies owned then, valued, and the lots up
/// to then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    pub date: NaiveDate,
    /// Number of copies owned.
    pub copies: usize,
    /// Value of the copies with a price.
    pub value: Decimal,
    pub without_price: usize,
    /// What the purchase lots cost.
    pub spent: Decimal,
    /// What the sale lots brought.
    pub sold: Decimal,
}

impl Totals {
    /// `value + sold − spent`.
    pub fn gain(&self) -> Decimal {
        self.value + self.sold - self.spent
    }

    /// The gain in % of what was spent, `None` when nothing was.
    pub fn gain_percent(&self) -> Option<Decimal> {
        if self.spent.is_zero() {
            return None;
        }

        Some((self.gain() * Decimal::ONE_HUNDRED / self.spent).round_dp(1))
    }
}

/// The collection on `date`, with `prices` by card data id. Rounded to the
/// cent.
pub fn totals_on(
    copies: &[Copy],
    lots: &[Lot],
    date: NaiveDate,
    prices: &HashMap<i64, Decimal>,
) -> Totals {
    let mut totals = Totals {
        date,
        copies: 0,
        value: Decimal::ZERO,
        without_price: 0,
        spent: Decimal::ZERO,
        sold: Decimal::ZERO,
    };

    for copy in copies.iter().filter(|c| c.is_owned_on(date)) {
        totals.copies += 1;
        match copy.value(prices) {
            Some(value) => totals.value += value,
            None => totals.without_price += 1,
        }
    }
    totals.value = totals.value.round_dp(2);

    for lot in lots.iter().filter(|lot| lot.date <= date) {
        if lot.is_sale {
            totals.sold += lot.price;
        } else {
            totals.spent += lot.price;
        }
    }

    totals
}

/// The copies owned on a date that have both a price and a unite price:
/// their value against what was paid for them, card by card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTotals {
    pub copies: usize,
    pub value: Decimal,
    pub paid: Decimal,
}

impl UnitTotals {
    pub fn gain(&self) -> Decimal {
        self.value - self.paid
    }

    pub fn gain_percent(&self) -> Option<Decimal> {
        if self.paid.is_zero() {
            return None;
        }

        Some((self.gain() * Decimal::ONE_HUNDRED / self.paid).round_dp(1))
    }
}

/// [`UnitTotals`] on `date`, rounded to the cent.
pub fn unit_totals_on(
    copies: &[Copy],
    date: NaiveDate,
    prices: &HashMap<i64, Decimal>,
) -> UnitTotals {
    let mut totals = UnitTotals {
        copies: 0,
        value: Decimal::ZERO,
        paid: Decimal::ZERO,
    };

    for copy in copies.iter().filter(|c| c.is_owned_on(date)) {
        let (Some(value), Some(paid)) = (copy.value(prices), copy.paid) else {
            continue;
        };
        totals.copies += 1;
        totals.value += value;
        totals.paid += paid;
    }

    totals.value = totals.value.round_dp(2);
    totals
}

/// [`totals_on`] each refresh date, with that date's prices, oldest first.
pub fn history(
    copies: &[Copy],
    lots: &[Lot],
    prices_by_date: &BTreeMap<NaiveDate, HashMap<i64, Decimal>>,
) -> Vec<Totals> {
    prices_by_date
        .iter()
        .map(|(date, prices)| totals_on(copies, lots, *date, prices))
        .collect()
}

/// A card's price change between two refreshes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceMove {
    pub card_data_id: i64,
    pub before: Decimal,
    pub after: Decimal,
}

impl PriceMove {
    pub fn change(&self) -> Decimal {
        self.after - self.before
    }

    /// In % of `before` (never zero, see [`biggest_moves`]).
    pub fn percent(&self) -> Decimal {
        (self.change() * Decimal::ONE_HUNDRED / self.before).round_dp(1)
    }
}

/// The cards whose price moved most between `before` and `after`, in %,
/// biggest first, at most `limit`. Only the cards priced in both and whose
/// price moved by at least `floor` € (so €0.02 → €0.05 isn't a +150% move).
pub fn biggest_moves(
    before: &HashMap<i64, Decimal>,
    after: &HashMap<i64, Decimal>,
    floor: Decimal,
    limit: usize,
) -> Vec<PriceMove> {
    let mut moves: Vec<PriceMove> = after
        .iter()
        .filter_map(|(id, after)| {
            let before = *before.get(id)?;
            let price_move = PriceMove {
                card_data_id: *id,
                before,
                after: *after,
            };
            let counts = before > Decimal::ZERO && price_move.change().abs() >= floor;
            counts.then_some(price_move)
        })
        .collect();

    // ties by id, so the order doesn't depend on the HashMap's
    moves.sort_by(|a, b| {
        b.percent()
            .abs()
            .cmp(&a.percent().abs())
            .then(a.card_data_id.cmp(&b.card_data_id))
    });
    moves.truncate(limit);
    moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn date(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn copy(card_data_id: i64, bought: NaiveDate) -> Copy {
        Copy {
            card_data_id: Some(card_data_id),
            status: Some(CardStatus::NearMint),
            language: Some(Language::English),
            bought,
            sold: None,
            paid: None,
        }
    }

    #[test]
    fn factors() {
        assert_eq!(condition_factor(Some(CardStatus::Mint)), dec!(1));
        assert_eq!(condition_factor(None), dec!(1));
        assert_eq!(condition_factor(Some(CardStatus::Excellent)), dec!(0.8));
        assert_eq!(condition_factor(Some(CardStatus::Good)), dec!(0.6));
        assert_eq!(condition_factor(Some(CardStatus::Poor)), dec!(0.4));
        assert_eq!(language_factor(Some(Language::French)), dec!(0.9));
        assert_eq!(language_factor(Some(Language::German)), dec!(0.9));
        assert_eq!(language_factor(None), dec!(1));
        assert_eq!(
            copy_value(
                dec!(10),
                Some(CardStatus::Excellent),
                Some(Language::French)
            ),
            dec!(7.2)
        );
    }

    #[test]
    fn ownership_by_date() {
        let mut c = copy(1, date(5, 10));
        assert!(!c.is_owned_on(date(5, 9)));
        assert!(c.is_owned_on(date(5, 10)));
        c.sold = Some(date(6, 1));
        assert!(c.is_owned_on(date(5, 31)));
        assert!(!c.is_owned_on(date(6, 1)), "sold that day");
    }

    fn lot(date: NaiveDate, is_sale: bool, price: Decimal) -> Lot {
        Lot {
            date,
            is_sale,
            price,
        }
    }

    #[test]
    fn totals_of_a_date() {
        let prices = HashMap::from([(1, dec!(10.00)), (2, dec!(0.333))]);
        let mut sold = copy(1, date(1, 1));
        sold.sold = Some(date(3, 1));
        let copies = [
            copy(1, date(1, 1)),
            // three copies of 0.333: the total is rounded, not each copy
            copy(2, date(1, 1)),
            copy(2, date(1, 1)),
            copy(2, date(1, 1)),
            copy(3, date(1, 1)), // no price
            copy(1, date(5, 1)), // bought later
            sold,
        ];
        let lots = [
            lot(date(1, 1), false, dec!(8.00)),
            lot(date(3, 1), true, dec!(2.50)),
            lot(date(5, 1), false, dec!(9.00)), // later
        ];

        let totals = totals_on(&copies, &lots, date(4, 1), &prices);
        assert_eq!(totals.copies, 5);
        assert_eq!(totals.value, dec!(11.00)); // 10 + 3 × 0.333 = 10.999
        assert_eq!(totals.without_price, 1);
        assert_eq!((totals.spent, totals.sold), (dec!(8.00), dec!(2.50)));
        assert_eq!(totals.gain(), dec!(5.50));
        assert_eq!(totals.gain_percent(), Some(dec!(68.8)));
    }

    #[test]
    fn unit_totals_compare_priced_copies_only() {
        let prices = HashMap::from([(1, dec!(3.00)), (2, dec!(0.50))]);
        let paid = |card_data_id, paid| Copy {
            paid,
            ..copy(card_data_id, date(1, 1))
        };
        let copies = [
            paid(1, Some(dec!(1.00))),
            paid(2, Some(dec!(1.00))),
            paid(2, None),             // bought as a whole
            paid(3, Some(dec!(4.00))), // no market price
        ];

        let totals = unit_totals_on(&copies, date(2, 1), &prices);
        assert_eq!(totals.copies, 2);
        assert_eq!((totals.value, totals.paid), (dec!(3.50), dec!(2.00)));
        assert_eq!(totals.gain(), dec!(1.50));
        assert_eq!(totals.gain_percent(), Some(dec!(75.0)));
    }

    #[test]
    fn a_copy_without_card_data_has_no_value() {
        let mut c = copy(1, date(1, 1));
        c.card_data_id = None;
        assert_eq!(c.value(&HashMap::from([(1, dec!(5))])), None);
        let totals = totals_on(&[c], &[], date(1, 2), &HashMap::new());
        assert_eq!(totals.without_price, 1);
        assert_eq!(totals.gain_percent(), None);
    }

    #[test]
    fn history_follows_the_refreshes() {
        let copies = [copy(1, date(1, 1)), copy(1, date(2, 1))];
        let lots = [
            lot(date(1, 1), false, dec!(1)),
            lot(date(2, 1), false, dec!(1)),
        ];
        let prices = BTreeMap::from([
            (date(1, 7), HashMap::from([(1, dec!(2))])),
            (date(2, 7), HashMap::from([(1, dec!(3))])),
        ]);
        let points: Vec<_> = history(&copies, &lots, &prices)
            .into_iter()
            .map(|t| (t.date, t.copies, t.value, t.spent))
            .collect();
        assert_eq!(
            points,
            vec![
                (date(1, 7), 1, dec!(2), dec!(1)),
                (date(2, 7), 2, dec!(6), dec!(2))
            ]
        );
    }

    #[test]
    fn biggest_moves_skip_small_changes() {
        let before = HashMap::from([
            (1, dec!(0.02)),
            (2, dec!(10)),
            (3, dec!(4)),
            (4, dec!(1)),
            (5, dec!(0)),
        ]);
        let after = HashMap::from([
            (1, dec!(0.05)), // +150%, but only €0.03
            (2, dec!(8)),    // −20%
            (3, dec!(6)),    // +50%
            (5, dec!(3)),    // no price before
            (6, dec!(9)),    // new
        ]);
        let moves = biggest_moves(&before, &after, dec!(0.50), 10);
        let ids: Vec<_> = moves
            .iter()
            .map(|m| (m.card_data_id, m.percent()))
            .collect();
        assert_eq!(ids, vec![(3, dec!(50.0)), (2, dec!(-20.0))]);
        assert_eq!(biggest_moves(&before, &after, dec!(0.50), 1).len(), 1);
    }
}
