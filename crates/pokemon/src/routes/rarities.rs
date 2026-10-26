//! `/pokemon/rarities`: how to recognise a Pokémon card's rarity and its
//! variant (normal, reverse holo, holo) on the physical card. Its content
//! is in `static/js/pokemon_rarities.js`, shared with the card finder and
//! the booster form (the variant choices' explanations).

use askama::Template;

#[derive(Template)]
#[template(path = "pokemon/rarities.html")]
pub struct RaritiesPage;

pub async fn page() -> RaritiesPage {
    RaritiesPage
}

/// The guide's link for a TCGdex rarity (the page finds the box by name).
pub fn href(rarity: &str) -> String {
    if rarity.is_empty() {
        return "/pokemon/rarities".into();
    }
    let encoded: String = url::form_urlencoded::byte_serialize(rarity.as_bytes()).collect();
    format!("/pokemon/rarities?rarity={encoded}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn links_to_the_guide() {
        assert_eq!(
            super::href("Special illustration rare"),
            "/pokemon/rarities?rarity=Special+illustration+rare"
        );
        assert_eq!(super::href(""), "/pokemon/rarities");
    }
}
