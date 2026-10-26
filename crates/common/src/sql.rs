//! SQL helpers shared by the collections' list filters.

/// `%text%` for LIKE, with `\` escaping the wildcards.
pub fn like_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("50%_off"), "%50\\%\\_off%");
    }
}
