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

/// JSON to put in a `<script type="application/json">`: `<`, `>` and `&`
/// written as `\u003c`… so no text in it (a deck or card name) can end the
/// tag. Read back as is by `JSON.parse`.
pub fn json_for_script(json: &serde_json::Value) -> String {
    json.to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

pub fn yes_no(b: &bool) -> askama::Result<&'static str> {
    Ok(if *b { "Yes" } else { "No" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_cannot_end_its_script_tag() {
        let json = serde_json::json!({ "name": "</script><script>alert(1)</script> & <!--" });
        let text = json_for_script(&json);
        assert!(
            !text.contains('<') && !text.contains('>') && !text.contains('&'),
            "{text}"
        );
        let back: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back, json);
    }

    #[test]
    fn money_rounds_half_up() {
        assert_eq!(money(&"1.005".parse().unwrap()).unwrap(), "1.01");
        assert_eq!(money(&"2".parse().unwrap()).unwrap(), "2.00");
        assert_eq!(money(&"-0.125".parse().unwrap()).unwrap(), "-0.13");
    }
}
