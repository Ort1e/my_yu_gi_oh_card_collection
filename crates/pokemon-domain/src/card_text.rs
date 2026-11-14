//! What a card is and says, for its page: TCGdex's JSON of the card
//! (`pokemon_card.json`, in the language it was fetched in: English, else
//! French; Japanese for a Japanese card) as plain text: its facts (HP,
//! types, weakness, retreat…), its abilities and attacks, a Trainer's or
//! Energy's effect, the Pokédex entry. The prices in that JSON are left out.
//! Like `yugioh::routes::cards::text`.

use serde_json::Value;

use crate::type_in_english;

/// A card's facts and text.
#[derive(Debug, PartialEq, Eq)]
pub struct CardText {
    /// The band's colour: `pokemon`, `trainer` or `energy` (letters only).
    pub frame: &'static str,
    /// `Stage 1 Pokémon`, `Supporter`, `Basic Energy`: the band's text.
    pub kind: String,
    /// In the order of the page; the missing ones left out.
    pub facts: Vec<Fact>,
    /// Abilities, then attacks.
    pub moves: Vec<Move>,
    /// A Trainer's or Energy's effect.
    pub effect: Option<String>,
    /// The Pokédex entry (the card's flavour text).
    pub description: Option<String>,
}

/// One line of the facts.
#[derive(Debug, PartialEq, Eq)]
pub struct Fact {
    pub label: &'static str,
    /// English names, shown as badges of their colour (`Lightning`).
    pub types: Vec<String>,
    /// Text after the badges (`×2`, `60`), may be empty.
    pub value: String,
}

/// An ability or an attack.
#[derive(Debug, PartialEq, Eq)]
pub struct Move {
    /// `Ability`, `Poké-Power`…; `None` for an attack.
    pub ability: Option<String>,
    /// An attack's energy cost, as type names (`Lightning`, `Colorless`).
    pub cost: Vec<String>,
    pub name: String,
    /// `20`, `30+`, `×2`.
    pub damage: Option<String>,
    pub effect: Option<String>,
}

/// A non-blank string value, trimmed, Windows line ends made plain.
fn text(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim().replace("\r\n", "\n");
    Some(text).filter(|t| !t.is_empty())
}

/// A string or a number as text (`20`, `"30+"`).
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(_) => text(value),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn type_names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(text)
                .map(|name| type_in_english(&name))
                .collect()
        })
        .unwrap_or_default()
}

/// `Weakness` / `Resistance` lines: `[{type, value}]`.
fn modifiers(label: &'static str, value: &Value) -> Option<Fact> {
    let list = value.as_array()?;
    let types: Vec<String> = list
        .iter()
        .filter_map(|m| text(&m["type"]))
        .map(|name| type_in_english(&name))
        .collect();
    if types.is_empty() {
        return None;
    }
    let value = list
        .iter()
        .filter_map(|m| text(&m["value"]))
        .collect::<Vec<_>>()
        .join(", ");
    Some(Fact {
        label,
        types,
        value,
    })
}

fn stage_label(stage: &str) -> String {
    match stage {
        "Stage1" => "Stage 1".into(),
        "Stage2" => "Stage 2".into(),
        other => other.into(),
    }
}

impl CardText {
    /// From TCGdex's data of the card; `None` for a card with nothing to
    /// say (an empty JSON).
    pub fn from_tcgdex(card: &Value) -> Option<CardText> {
        let category = card["category"].as_str()?;
        let effect = text(&card["effect"]);

        let (frame, kind) = match category {
            "Pokemon" => {
                let stage = text(&card["stage"]).map(|s| stage_label(&s));
                let kind = match stage {
                    Some(stage) => format!("{stage} Pokémon"),
                    None => "Pokémon".to_string(),
                };
                ("pokemon", kind)
            }
            "Trainer" => (
                "trainer",
                text(&card["trainerType"]).unwrap_or_else(|| "Trainer".into()),
            ),
            "Energy" => (
                "energy",
                match text(&card["energyType"]) {
                    Some(kind) => format!("{kind} Energy"),
                    None => "Energy".into(),
                },
            ),
            _ => return None,
        };

        let mut facts = Vec::new();
        if let Some(hp) = scalar(&card["hp"]) {
            facts.push(Fact {
                label: "HP",
                types: Vec::new(),
                value: hp,
            });
        }
        let types = type_names(&card["types"]);
        if !types.is_empty() {
            facts.push(Fact {
                label: "Type",
                types,
                value: String::new(),
            });
        }
        if let Some(from) = text(&card["evolveFrom"]) {
            facts.push(Fact {
                label: "Evolves from",
                types: Vec::new(),
                value: from,
            });
        }
        facts.extend(modifiers("Weakness", &card["weaknesses"]));
        facts.extend(modifiers("Resistance", &card["resistances"]));
        if let Some(retreat) = scalar(&card["retreat"]) {
            facts.push(Fact {
                label: "Retreat",
                types: Vec::new(),
                value: retreat,
            });
        }
        for (label, key) in [
            ("Regulation mark", "regulationMark"),
            ("Illustrator", "illustrator"),
        ] {
            if let Some(value) = text(&card[key]) {
                facts.push(Fact {
                    label,
                    types: Vec::new(),
                    value,
                });
            }
        }

        let mut moves = Vec::new();
        for ability in card["abilities"].as_array().into_iter().flatten() {
            if let Some(name) = text(&ability["name"]) {
                moves.push(Move {
                    ability: Some(text(&ability["type"]).unwrap_or_else(|| "Ability".into())),
                    cost: Vec::new(),
                    name,
                    damage: None,
                    effect: text(&ability["effect"]),
                });
            }
        }
        for attack in card["attacks"].as_array().into_iter().flatten() {
            if let Some(name) = text(&attack["name"]) {
                moves.push(Move {
                    ability: None,
                    cost: type_names(&attack["cost"]),
                    name,
                    damage: scalar(&attack["damage"]),
                    effect: text(&attack["effect"]),
                });
            }
        }

        let description = text(&card["description"]);
        Some(CardText {
            frame,
            kind,
            facts,
            moves,
            effect,
            description,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_pokemon_with_an_ability_and_an_attack() {
        let card = json!({
            "category": "Pokemon", "stage": "Stage1", "evolveFrom": "Pichu", "hp": 90,
            "types": ["Lightning"], "illustrator": "Someone", "regulationMark": "J",
            "retreat": 1, "description": "A mouse.",
            "weaknesses": [{"type": "Fighting", "value": "×2"}],
            "abilities": [{"type": "Ability", "name": "Gaze", "effect": "Less damage."}],
            "attacks": [
                {"cost": ["Lightning", "Colorless"], "name": "Ball", "damage": 20},
                {"cost": ["Colorless"], "name": "Zap", "damage": "30+", "effect": "More."}
            ],
            "pricing": {"cardmarket": {"avg": 1.0}}
        });
        let text = CardText::from_tcgdex(&card).unwrap();

        assert_eq!(text.frame, "pokemon");
        assert_eq!(text.kind, "Stage 1 Pokémon");
        let labels: Vec<_> = text.facts.iter().map(|f| f.label).collect();
        assert_eq!(
            labels,
            [
                "HP",
                "Type",
                "Evolves from",
                "Weakness",
                "Retreat",
                "Regulation mark",
                "Illustrator"
            ]
        );
        assert_eq!(text.facts[3].types, ["Fighting"]);
        assert_eq!(text.facts[3].value, "×2");

        assert_eq!(text.moves.len(), 3);
        assert_eq!(text.moves[0].ability.as_deref(), Some("Ability"));
        assert_eq!(text.moves[1].cost, ["Lightning", "Colorless"]);
        assert_eq!(text.moves[1].damage.as_deref(), Some("20"));
        assert_eq!(text.moves[2].damage.as_deref(), Some("30+"));
        assert_eq!(text.moves[2].effect.as_deref(), Some("More."));
        assert_eq!(text.description.as_deref(), Some("A mouse."));
        assert_eq!(text.effect, None);
    }

    #[test]
    fn trainers_and_energies_have_an_effect() {
        let trainer = json!({"category": "Trainer", "trainerType": "Supporter",
            "effect": "Draw 2 cards.", "illustrator": "Fuse"});
        let text = CardText::from_tcgdex(&trainer).unwrap();
        assert_eq!((text.frame, text.kind.as_str()), ("trainer", "Supporter"));
        assert_eq!(text.effect.as_deref(), Some("Draw 2 cards."));
        assert!(text.moves.is_empty());

        let energy = json!({"category": "Energy", "energyType": "Special", "effect": "  "});
        let text = CardText::from_tcgdex(&energy).unwrap();
        assert_eq!(text.kind, "Special Energy");
        assert_eq!(text.effect, None);
    }

    #[test]
    fn an_empty_json_says_nothing() {
        assert_eq!(CardText::from_tcgdex(&json!({})), None);
    }
}
