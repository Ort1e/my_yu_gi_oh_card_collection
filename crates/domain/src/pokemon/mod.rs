//! The Pokémon card collection (routes `/pokemon`): what TCGdex says about a
//! card, and the rules of the collection. New in the Rust app (Django had
//! no Pokémon cards).
//!
//! - [`card_ref`]: the code printed at the bottom of a card (`FNE 067/064`);
//! - [`pulls`]: a booster opening's numbers (`67 45r 12x2`);
//! - [`price`]: a copy's market price from Cardmarket's numbers, and its value;
//! - [`wish`]: whether a wish's price is reached, whether a copy fulfils it.
//!
//! TCGdex has two catalogues ([`Catalogue`]): the international cards (one
//! card id for its French, English, German… prints) and the Japanese cards
//! (their own sets and ids). A copy's catalogue follows its language.

pub mod card_ref;
pub mod price;
pub mod pulls;
pub mod wish;

use serde_json::Value;

/// Which TCGdex catalogue a card belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Catalogue {
    International,
    Japanese,
}

impl Catalogue {
    /// Value stored in the database.
    pub fn code(self) -> &'static str {
        match self {
            Catalogue::International => "intl",
            Catalogue::Japanese => "ja",
        }
    }

    pub fn from_code(code: &str) -> Option<Catalogue> {
        match code {
            "intl" => Some(Catalogue::International),
            "ja" => Some(Catalogue::Japanese),
            _ => None,
        }
    }

    /// The catalogue of a copy in this language (`ja` → Japanese).
    pub fn of_language(language: &str) -> Catalogue {
        if language == "ja" {
            Catalogue::Japanese
        } else {
            Catalogue::International
        }
    }

    /// The language TCGdex is asked in for this catalogue's card data
    /// (English: its categories, types and rarities are the stored labels).
    pub fn data_language(self) -> &'static str {
        match self {
            Catalogue::International => "en",
            Catalogue::Japanese => "ja",
        }
    }
}

/// The languages offered by the forms: `(code, label)`. Any other two-letter
/// code TCGdex knows is accepted too.
pub const LANGUAGES: [(&str, &str); 7] = [
    ("fr", "French"),
    ("en", "English"),
    ("ja", "Japanese"),
    ("de", "German"),
    ("it", "Italian"),
    ("es", "Spanish"),
    ("pt", "Portuguese"),
];

pub fn language_label(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(code, |(_, label)| label)
}

/// A language code as typed: two or three lowercase letters (`fr`, `zh`).
pub fn parse_language(text: &str) -> Option<String> {
    let code = text.trim().to_lowercase();
    let ok = (2..=3).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase());
    ok.then_some(code)
}

/// A print of a card. TCGdex's names (`firstEdition`, `wPromo`) are the
/// stored values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variant {
    Normal,
    Holo,
    Reverse,
    FirstEdition,
    WPromo,
}

impl Variant {
    pub const ALL: [Variant; 5] = [
        Variant::Normal,
        Variant::Holo,
        Variant::Reverse,
        Variant::FirstEdition,
        Variant::WPromo,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Variant::Normal => "normal",
            Variant::Holo => "holo",
            Variant::Reverse => "reverse",
            Variant::FirstEdition => "firstEdition",
            Variant::WPromo => "wPromo",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Variant::Normal => "Normal",
            Variant::Holo => "Holo",
            Variant::Reverse => "Reverse holo",
            Variant::FirstEdition => "1st edition",
            Variant::WPromo => "W promo",
        }
    }

    pub fn from_code(code: &str) -> Option<Variant> {
        Variant::ALL.into_iter().find(|v| v.code() == code)
    }

    /// TCGdex's `variants_detailed[].type` (`Normal`, `Holo`, `Reverse`).
    fn from_detailed_type(text: &str) -> Option<Variant> {
        match text.to_ascii_lowercase().as_str() {
            "normal" => Some(Variant::Normal),
            "holo" => Some(Variant::Holo),
            "reverse" => Some(Variant::Reverse),
            "firstedition" | "1st edition" => Some(Variant::FirstEdition),
            "wpromo" => Some(Variant::WPromo),
            _ => None,
        }
    }

    /// `"normal,reverse"` (the `variants` column) → the variants.
    pub fn parse_list(text: &str) -> Vec<Variant> {
        text.split(',').filter_map(Variant::from_code).collect()
    }

    pub fn list_code(variants: &[Variant]) -> String {
        variants
            .iter()
            .map(|v| v.code())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The variant a copy is when nothing says otherwise: normal when the card
/// has a normal print, else its only (or first) print.
pub fn default_variant(variants: &[Variant]) -> Variant {
    if variants.is_empty() || variants.contains(&Variant::Normal) {
        Variant::Normal
    } else {
        variants[0]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Pokemon,
    Trainer,
    Energy,
}

impl Category {
    pub fn code(self) -> &'static str {
        match self {
            Category::Pokemon => "Pokemon",
            Category::Trainer => "Trainer",
            Category::Energy => "Energy",
        }
    }

    /// TCGdex's category, in English or French (`Dresseur`).
    pub fn from_tcgdex(text: &str) -> Option<Category> {
        match text {
            "Pokemon" | "Pokémon" => Some(Category::Pokemon),
            "Trainer" | "Dresseur" => Some(Category::Trainer),
            "Energy" | "Énergie" | "Energie" => Some(Category::Energy),
            _ => None,
        }
    }
}

/// How a copy came into the collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Booster,
    Single,
    Gift,
    Trade,
    Other,
}

impl Source {
    pub const ALL: [Source; 5] = [
        Source::Booster,
        Source::Single,
        Source::Gift,
        Source::Trade,
        Source::Other,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Source::Booster => "booster",
            Source::Single => "single",
            Source::Gift => "gift",
            Source::Trade => "trade",
            Source::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Booster => "Booster",
            Source::Single => "Bought as a single",
            Source::Gift => "Gift",
            Source::Trade => "Trade",
            Source::Other => "Other",
        }
    }

    pub fn from_code(code: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|s| s.code() == code)
    }
}

/// The Pokémon types, TCGdex's English names (the stored ones) and French.
pub const TYPES: [(&str, &str); 11] = [
    ("Grass", "Plante"),
    ("Fire", "Feu"),
    ("Water", "Eau"),
    ("Lightning", "Électrique"),
    ("Psychic", "Psy"),
    ("Fighting", "Combat"),
    ("Darkness", "Obscurité"),
    ("Metal", "Métal"),
    ("Fairy", "Fée"),
    ("Dragon", "Dragon"),
    ("Colorless", "Incolore"),
];

/// A type's English name, from English or French.
pub fn type_in_english(name: &str) -> String {
    TYPES
        .iter()
        .find(|(en, fr)| *en == name || *fr == name)
        .map_or(name, |(en, _)| en)
        .to_string()
}

/// What the collection keeps of a TCGdex card (one language's answer).
#[derive(Debug, Clone, PartialEq)]
pub struct CardInfo {
    pub tcgdex_id: String,
    pub set_id: String,
    pub set_name: String,
    pub local_id: String,
    pub name: String,
    pub rarity: Option<String>,
    pub category: Category,
    /// English names.
    pub types: Vec<String>,
    pub dex_id: Option<i64>,
    pub variants: Vec<Variant>,
    /// Base URL of the picture (add `/high.webp`), when TCGdex has one.
    pub image: Option<String>,
}

/// A non-blank string value, trimmed.
fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|s| !s.is_empty())
}

impl CardInfo {
    /// TCGdex's `GET /v2/{lang}/cards/{id}` answer. `None` when a required
    /// field is missing.
    pub fn from_tcgdex(json: &Value) -> Option<CardInfo> {
        let variants = match json.get("variants").and_then(Value::as_object) {
            Some(flags) => Variant::ALL
                .into_iter()
                .filter(|v| flags.get(v.code()).and_then(Value::as_bool) == Some(true))
                .collect(),
            None => Vec::new(),
        };
        let types = json
            .get("types")
            .and_then(Value::as_array)
            .map(|types| types.iter().filter_map(text).map(type_in_english).collect())
            .unwrap_or_default();

        Some(CardInfo {
            tcgdex_id: text(&json["id"])?.to_string(),
            set_id: text(&json["set"]["id"])?.to_string(),
            set_name: text(&json["set"]["name"])?.to_string(),
            local_id: text(&json["localId"])?.to_string(),
            name: text(&json["name"])?.to_string(),
            rarity: text(&json["rarity"])
                .filter(|r| *r != "None")
                .map(str::to_string),
            category: Category::from_tcgdex(text(&json["category"])?)?,
            types,
            dex_id: json["dexId"]
                .as_array()
                .and_then(|ids| ids.first()?.as_i64()),
            variants,
            image: text(&json["image"]).map(str::to_string),
        })
    }

    pub fn default_variant(&self) -> Variant {
        default_variant(&self.variants)
    }
}

/// A card name to compare: lower case, without accents, `&`, `et` and
/// `and` the same (French writes "Pikachu et Zekrom GX", English
/// "Pikachu & Zekrom GX"), `-` as a space ("Mew-ex" = "Mew ex").
pub fn fold_name(name: &str) -> String {
    let plain: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            '-' | '’' | '\'' => ' ',
            c => c,
        })
        .collect();
    plain
        .split_whitespace()
        .map(|word| match word {
            "et" | "and" => "&",
            word => word,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a card's name contains what was typed (both [`fold_name`]d).
pub fn name_matches(name: &str, typed: &str) -> bool {
    let typed = fold_name(typed);
    !typed.is_empty() && fold_name(name).contains(&typed)
}

/// Whether a card of this rarity is (most likely) a full art: the
/// suggestion of the forms, the owner ticks or unticks it. TCGdex has no
/// "full art" field; its English rarities (also used for the Japanese
/// cards) are mapped here.
pub fn is_full_art(rarity: Option<&str>) -> bool {
    let Some(rarity) = rarity else {
        return false;
    };

    const FULL_ART: [&str; 12] = [
        "illustration rare",
        "special illustration rare",
        "hyper rare",
        "full art trainer",
        "ultra rare",
        "secret rare",
        "shiny ultra rare",
        "shiny rare v",
        "shiny rare vmax",
        "mega hyper rare",
        "mega attack rare",
        "black white rare",
    ];
    let rarity = rarity.to_lowercase();
    FULL_ART.contains(&rarity.as_str()) || rarity.contains("art rare")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn horsea() -> Value {
        json!({
            "category": "Pokemon",
            "id": "sv06.5-067",
            "localId": "067",
            "name": "Horsea",
            "rarity": "Illustration rare",
            "image": "https://assets.tcgdex.net/en/sv/sv06.5/067",
            "set": {"id": "sv06.5", "name": "Shrouded Fable"},
            "types": ["Water"],
            "dexId": [116],
            "variants": {"firstEdition": false, "holo": true, "normal": false,
                         "reverse": false, "wPromo": false}
        })
    }

    #[test]
    fn reads_a_tcgdex_card() {
        let card = CardInfo::from_tcgdex(&horsea()).unwrap();
        assert_eq!(card.tcgdex_id, "sv06.5-067");
        assert_eq!(
            (card.set_id.as_str(), card.local_id.as_str()),
            ("sv06.5", "067")
        );
        assert_eq!(card.category, Category::Pokemon);
        assert_eq!(card.types, ["Water"]);
        assert_eq!(card.dex_id, Some(116));
        assert_eq!(card.variants, [Variant::Holo]);
        assert_eq!(card.default_variant(), Variant::Holo);
        assert!(is_full_art(card.rarity.as_deref()));
    }

    #[test]
    fn reads_french_and_japanese_answers() {
        let french = json!({
            "category": "Dresseur", "id": "sv06.5-055", "localId": "055",
            "name": "Faux Bonbon", "rarity": "Commune",
            "set": {"id": "sv06.5", "name": "Fable Nébuleuse"},
            "types": ["Eau"]
        });
        let card = CardInfo::from_tcgdex(&french).unwrap();
        assert_eq!(card.category, Category::Trainer);
        assert_eq!(card.types, ["Water"], "types stored in English");
        assert_eq!(card.variants, []);
        assert_eq!(card.default_variant(), Variant::Normal);
        assert_eq!(card.image, None);

        let japanese = json!({
            "category": "Pokemon", "id": "SV6a-010", "localId": "010", "name": "タッツー",
            "rarity": "Common", "set": {"id": "SV6a", "name": "ナイトワンダラー"},
            "types": ["Water"], "dexId": [116],
            "variants": {"normal": true, "holo": false}
        });
        let card = CardInfo::from_tcgdex(&japanese).unwrap();
        assert_eq!(card.name, "タッツー");
        assert_eq!(card.variants, [Variant::Normal]);

        assert_eq!(CardInfo::from_tcgdex(&json!({"id": "x"})), None);
    }

    #[test]
    fn full_arts_by_rarity() {
        for rarity in [
            "Special illustration rare",
            "Ultra Rare",
            "Full Art Trainer",
            "Hyper rare",
            "Special Art Rare",
        ] {
            assert!(is_full_art(Some(rarity)), "{rarity}");
        }
        for rarity in [
            "Common",
            "Double rare",
            "Holo Rare V",
            "Radiant Rare",
            "Shiny rare",
        ] {
            assert!(!is_full_art(Some(rarity)), "{rarity}");
        }
        assert!(!is_full_art(None));
    }

    #[test]
    fn catalogue_follows_the_language() {
        assert_eq!(Catalogue::of_language("ja"), Catalogue::Japanese);
        assert_eq!(Catalogue::of_language("fr"), Catalogue::International);
        assert_eq!(Catalogue::from_code("ja"), Some(Catalogue::Japanese));
        assert_eq!(parse_language(" FR "), Some("fr".into()));
        assert_eq!(parse_language("french"), None);
        assert_eq!(language_label("ja"), "Japanese");
        assert_eq!(language_label("ko"), "ko");
    }

    #[test]
    fn names_match_across_languages() {
        assert!(name_matches("Pikachu et Zekrom GX", "pikachu & zekrom"));
        assert!(name_matches("Pikachu & Zekrom GX", "Pikachu and Zekrom"));
        assert!(name_matches("Pêchaminus-ex", "pechaminus ex"));
        assert!(name_matches("Pikachu et Zekrom GX", "zekrom"));
        assert!(!name_matches("Zekrom", "pikachu"));
        assert!(!name_matches("Zekrom", "  "));
    }

    #[test]
    fn variant_lists() {
        let list = [Variant::Normal, Variant::Reverse];
        assert_eq!(Variant::list_code(&list), "normal,reverse");
        assert_eq!(Variant::parse_list("normal,reverse"), list);
        assert_eq!(Variant::parse_list(""), []);
        assert_eq!(
            default_variant(&[Variant::Holo, Variant::Reverse]),
            Variant::Holo
        );
    }
}
