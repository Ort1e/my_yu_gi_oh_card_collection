//! Custom askama filters. Bring them into a template's module with
//! `use crate::web::filters;`.

use rust_decimal::{Decimal, RoundingStrategy};

/// Two decimals, rounding half away from zero (Django's `floatformat:2`).
pub fn money(d: &Decimal) -> askama::Result<String> {
    Ok(format!(
        "{:.2}",
        d.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
    ))
}

/// Colour name of a debt amount, for the `debt-<colour>` CSS classes
/// (style.css): red above the allowed debt, orange when in debt, green otherwise.
pub fn debt_color(debt: &Decimal, available: &Decimal) -> askama::Result<&'static str> {
    Ok(if debt > available {
        "red"
    } else if *debt > Decimal::ZERO {
        "orange"
    } else {
        "green"
    })
}

pub fn yes_no(b: &bool) -> askama::Result<&'static str> {
    Ok(if *b { "Yes" } else { "No" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_rounds_half_up() {
        assert_eq!(money(&"1.005".parse().unwrap()).unwrap(), "1.01");
        assert_eq!(money(&"2".parse().unwrap()).unwrap(), "2.00");
        assert_eq!(money(&"-0.125".parse().unwrap()).unwrap(), "-0.13");
    }
}
