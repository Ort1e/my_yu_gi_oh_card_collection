//! Monthly budget recap (Django: `views/budget.py` — `RecapEntry`,
//! `BudgetTotals`), used by the budget page and the dashboard.
//!
//! Callers load the rows; this module only does the arithmetic.
//! Deliberate difference from Django: cancelled sales are not added to the
//! monthly budget (Django counted them there but not in the selling revenue).

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;

use crate::lot::LotType;

/// The fields of a lot the recap needs.
#[derive(Debug, Clone)]
pub struct LotSummary {
    pub lot_type: LotType,
    pub is_cancelled: bool,
    pub buy_date: NaiveDate,
    pub price: Decimal,
    /// See [`crate::lot::shipping_cost`].
    pub shipping_cost: Decimal,
}

/// A `monthly_budget` row with the sum of its `apport` rows.
#[derive(Debug, Clone)]
pub struct BudgetSummary {
    pub id: i64,
    /// First day of the month.
    pub month: NaiveDate,
    pub amount: Decimal,
    pub apports_total: Decimal,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RecapEntry {
    pub month: NaiveDate,
    pub spent: Decimal,
    pub budget: Decimal,
    pub debt: Decimal,
    pub shipping_cost: Decimal,
    pub shipping_percent: Decimal,
    pub selling_revenue: Decimal,
    pub selling_shipping_cost: Decimal,
    pub selling_real_revenue: Decimal,
}

impl RecapEntry {
    fn new(
        month: NaiveDate,
        spent: Decimal,
        budget: Decimal,
        shipping_cost: Decimal,
        selling_revenue: Decimal,
        selling_shipping_cost: Decimal,
    ) -> RecapEntry {
        RecapEntry {
            month,
            spent,
            budget,
            debt: spent - budget,
            shipping_cost,
            shipping_percent: percent(shipping_cost, spent),
            selling_revenue,
            selling_shipping_cost,
            selling_real_revenue: selling_revenue - selling_shipping_cost,
        }
    }

    /// e.g. "October 2026".
    pub fn month_str(&self) -> String {
        self.month.format("%B %Y").to_string()
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BudgetTotals {
    pub total_budget: Decimal,
    pub total_spent: Decimal,
    pub total_debt: Decimal,
    pub total_shipping_cost: Decimal,
    pub total_shipping_percent: Decimal,
    pub total_selling_revenue: Decimal,
    pub total_selling_shipping_cost: Decimal,
    pub total_real_selling_revenue: Decimal,
}

impl BudgetTotals {
    pub fn compute(entries: &[RecapEntry]) -> BudgetTotals {
        let sum = |f: fn(&RecapEntry) -> Decimal| entries.iter().map(f).sum::<Decimal>();
        let total_spent = sum(|e| e.spent);
        let total_shipping_cost = sum(|e| e.shipping_cost);
        let total_selling_revenue = sum(|e| e.selling_revenue);
        let total_selling_shipping_cost = sum(|e| e.selling_shipping_cost);
        BudgetTotals {
            total_budget: sum(|e| e.budget),
            total_spent,
            total_debt: sum(|e| e.debt),
            total_shipping_cost,
            total_shipping_percent: percent(total_shipping_cost, total_spent),
            total_selling_revenue,
            total_selling_shipping_cost,
            total_real_selling_revenue: total_selling_revenue - total_selling_shipping_cost,
        }
    }
}

fn percent(part: Decimal, whole: Decimal) -> Decimal {
    if whole > Decimal::ZERO {
        part / whole * Decimal::ONE_HUNDRED
    } else {
        Decimal::ZERO
    }
}

pub fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("day 1 always exists")
}

pub fn next_month(d: NaiveDate) -> NaiveDate {
    let d = first_of_month(d);
    if d.month() == 12 {
        NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
    }
}

/// First day of each month from `start` to `end`, inclusive.
pub fn month_range(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let end = first_of_month(end);
    let mut months = Vec::new();
    let mut current = first_of_month(start);
    while current <= end {
        months.push(current);
        current = next_month(current);
    }
    months
}

/// `RecapEntry.from_db`: one entry per month, **most recent first**, from the
/// first budget month (or, without budgets, the first purchase) up to the
/// current month — or the next month when `include_next_month` (Django's
/// `prev=True`, used by the dashboard and the budget table).
pub fn recap(
    lots: &[LotSummary],
    budgets: &[BudgetSummary],
    today: NaiveDate,
    include_next_month: bool,
) -> Vec<RecapEntry> {
    let mut budgets: Vec<&BudgetSummary> = budgets.iter().collect();
    budgets.sort_by_key(|b| b.month);

    let active = |t: LotType| {
        lots.iter()
            .filter(move |l| !l.is_cancelled && l.lot_type == t)
    };

    let first_month = match budgets.first() {
        Some(b) => first_of_month(b.month),
        None => active(LotType::Purchase)
            .map(|l| first_of_month(l.buy_date))
            .min()
            .unwrap_or_else(|| first_of_month(today)),
    };
    let last_month = if include_next_month {
        next_month(today)
    } else {
        first_of_month(today)
    };
    let months = month_range(first_month, last_month);

    let spending = monthly_totals(active(LotType::Purchase));
    let selling = monthly_totals(active(LotType::Sale));
    let budget_map = budget_map(&selling, &budgets, &months);

    months
        .iter()
        .rev()
        .map(|&month| {
            let (spent, shipping) = spending.get(&month).copied().unwrap_or_default();
            let (sold, sold_shipping) = selling.get(&month).copied().unwrap_or_default();
            let budget = budget_map.get(&month).copied().unwrap_or_default();
            RecapEntry::new(month, spent, budget, shipping, sold, sold_shipping)
        })
        .collect()
}

/// month -> (Σ price, Σ shipping cost)
fn monthly_totals<'a>(
    lots: impl Iterator<Item = &'a LotSummary>,
) -> HashMap<NaiveDate, (Decimal, Decimal)> {
    let mut totals: HashMap<NaiveDate, (Decimal, Decimal)> = HashMap::new();
    for lot in lots {
        let entry = totals.entry(first_of_month(lot.buy_date)).or_default();
        entry.0 += lot.price;
        entry.1 += lot.shipping_cost;
    }
    totals
}

/// Budget available each month: the latest monthly amount (carried forward),
/// plus the apports of a budget row for that exact month, plus that month's
/// sale revenue.
fn budget_map(
    selling: &HashMap<NaiveDate, (Decimal, Decimal)>,
    sorted_budgets: &[&BudgetSummary],
    months: &[NaiveDate],
) -> HashMap<NaiveDate, Decimal> {
    let mut map = HashMap::new();
    let mut last_amount = Decimal::ZERO;
    let mut idx = 0;
    for &month in months {
        while idx < sorted_budgets.len() && sorted_budgets[idx].month <= month {
            last_amount = sorted_budgets[idx].amount;
            idx += 1;
        }
        let apports: Decimal = sorted_budgets
            .iter()
            .filter(|b| b.month == month)
            .map(|b| b.apports_total)
            .sum();
        let sold = selling.get(&month).map(|s| s.0).unwrap_or_default();
        map.insert(month, last_amount + apports + sold);
    }
    map
}

/// Dashboard / budget page "available debt": 3 × the amount of the last
/// `monthly_budget` row by id (Django's `MonthlyBudget.objects.last()`).
pub fn available_debt(budgets: &[BudgetSummary]) -> Decimal {
    budgets
        .iter()
        .max_by_key(|b| b.id)
        .map(|b| b.amount * Decimal::from(3))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn lot(t: LotType, cancelled: bool, d: NaiveDate, price: Decimal, ship: Decimal) -> LotSummary {
        LotSummary {
            lot_type: t,
            is_cancelled: cancelled,
            buy_date: d,
            price,
            shipping_cost: ship,
        }
    }

    fn budget(id: i64, month: NaiveDate, amount: Decimal, apports: Decimal) -> BudgetSummary {
        BudgetSummary {
            id,
            month,
            amount,
            apports_total: apports,
        }
    }

    #[test]
    fn month_helpers() {
        assert_eq!(next_month(date(2025, 12, 15)), date(2026, 1, 1));
        assert_eq!(
            month_range(date(2025, 11, 20), date(2026, 1, 3)),
            vec![date(2025, 11, 1), date(2025, 12, 1), date(2026, 1, 1)]
        );
    }

    #[test]
    fn recap_carries_budget_forward_and_adds_apports_and_sales() {
        let budgets = [
            budget(1, date(2025, 1, 1), dec!(25.00), dec!(0)),
            budget(2, date(2025, 3, 1), dec!(30.00), dec!(10.00)),
        ];
        let lots = [
            lot(
                LotType::Purchase,
                false,
                date(2025, 1, 10),
                dec!(20.00),
                dec!(2.00),
            ),
            lot(
                LotType::Purchase,
                false,
                date(2025, 1, 20),
                dec!(10.00),
                dec!(1.00),
            ),
            lot(
                LotType::Purchase,
                true,
                date(2025, 2, 5),
                dec!(99.00),
                dec!(9.00),
            ),
            lot(
                LotType::Sale,
                false,
                date(2025, 2, 6),
                dec!(8.00),
                dec!(1.50),
            ),
            lot(
                LotType::Sale,
                true,
                date(2025, 3, 6),
                dec!(4.00),
                dec!(0.50),
            ),
        ];

        let entries = recap(&lots, &budgets, date(2025, 3, 15), false);
        let months: Vec<_> = entries.iter().map(|e| e.month).collect();
        assert_eq!(
            months,
            vec![date(2025, 3, 1), date(2025, 2, 1), date(2025, 1, 1)]
        );

        let jan = &entries[2];
        assert_eq!(jan.spent, dec!(30.00));
        assert_eq!(jan.shipping_cost, dec!(3.00));
        assert_eq!(jan.budget, dec!(25.00));
        assert_eq!(jan.debt, dec!(5.00));
        assert_eq!(jan.shipping_percent, dec!(10));
        assert_eq!(jan.month_str(), "January 2025");

        let feb = &entries[1];
        assert_eq!(feb.spent, dec!(0)); // cancelled purchase ignored
        assert_eq!(feb.budget, dec!(33.00)); // carried 25 + sale 8
        assert_eq!(feb.selling_revenue, dec!(8.00));
        assert_eq!(feb.selling_real_revenue, dec!(6.50));
        assert_eq!(feb.shipping_percent, dec!(0));

        let mar = &entries[0];
        // 30 + apports 10, the cancelled sale is ignored
        assert_eq!(mar.budget, dec!(40.00));
        assert_eq!(mar.selling_revenue, dec!(0));

        let totals = BudgetTotals::compute(&entries);
        assert_eq!(totals.total_budget, dec!(98.00));
        assert_eq!(totals.total_spent, dec!(30.00));
        assert_eq!(totals.total_debt, dec!(-68.00));
        assert_eq!(totals.total_shipping_percent, dec!(10));
        assert_eq!(totals.total_real_selling_revenue, dec!(6.50));
    }

    #[test]
    fn recap_with_next_month() {
        let budgets = [budget(1, date(2025, 3, 1), dec!(30.00), dec!(0))];
        let entries = recap(&[], &budgets, date(2025, 3, 15), true);
        assert_eq!(entries[0].month, date(2025, 4, 1));
        assert_eq!(entries[0].budget, dec!(30.00));
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn recap_without_budgets_starts_at_first_purchase() {
        let lots = [lot(
            LotType::Purchase,
            false,
            date(2025, 1, 10),
            dec!(5),
            dec!(1),
        )];
        let entries = recap(&lots, &[], date(2025, 2, 1), false);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].debt, dec!(5));
        assert!(recap(&[], &[], date(2025, 2, 1), false).len() == 1);
    }

    #[test]
    fn available_debt_uses_last_budget_by_id() {
        let budgets = [
            budget(2, date(2025, 1, 1), dec!(30.00), dec!(0)),
            budget(1, date(2025, 3, 1), dec!(25.00), dec!(0)),
        ];
        assert_eq!(available_debt(&budgets), dec!(90.00));
        assert_eq!(available_debt(&[]), dec!(0));
    }
}
