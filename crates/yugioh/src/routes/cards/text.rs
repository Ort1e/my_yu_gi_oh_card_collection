//! What a card is and says, for its page: ygoprodeck's data of the card
//! (`card_data.json_data`, in English) as plain text: its facts (card
//! type, the bracket line, attribute, level, ATK/DEF…) and its text (a
//! Pendulum card's two effects apart).

use serde_json::Value;

/// A card's facts and text.
#[derive(Debug, PartialEq, Eq)]
pub struct CardText {
    /// ygoprodeck's `frameType` (`effect`, `spell`, `effect_pendulum`…): the
    /// colour of the page's band, like the card's frame (`frame-<name>` in
    /// card.css); letters and `_` only.
    pub frame: String,
    /// `Pendulum Effect Monster`, `Field Spell`: the band's text.
    pub kind: String,
    /// `Type: Spellcaster / Pendulum / Effect`, `Level: 7`…, in the order of
    /// the page; the missing ones left out.
    pub facts: Vec<Fact>,
    /// The text: one part, or a Pendulum card's `("Pendulum effect", …)`
    /// and `("Monster effect", …)`.
    pub parts: Vec<(Option<&'static str>, String)>,
}

/// One line of the facts.
#[derive(Debug, PartialEq, Eq)]
pub struct Fact {
    pub label: &'static str,
    pub value: String,
    /// A level's or rank's stars (`★★★★`), else empty.
    pub stars: String,
    /// The attribute, shown as a badge of its colour (`attribute-dark`).
    pub badge: Option<String>,
}

/// A text field, trimmed, Windows line ends made plain; `None` when empty.
fn text(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim().replace("\r\n", "\n");
    Some(text).filter(|t| !t.is_empty())
}

impl CardText {
    /// From ygoprodeck's data of the card; `None` without a text.
    pub fn from_card_data(card: &Value) -> Option<CardText> {
        let parts = match (text(&card["pend_desc"]), text(&card["monster_desc"])) {
            (Some(pendulum), Some(monster)) => vec![
                (Some("Pendulum effect"), pendulum),
                (Some("Monster effect"), monster),
            ],
            _ => vec![(None, text(&card["desc"])?)],
        };

        let kind = card["type"].as_str().unwrap_or("");
        let monster = kind.contains("Monster");
        let number = |field: &str| card[field].as_i64();
        let mut facts: Vec<Fact> = Vec::new();
        let mut fact = |label: &'static str, value: Option<String>| {
            let Some(value) = value.filter(|v| !v.is_empty()) else {
                return;
            };
            let stars = match (label, value.parse::<usize>()) {
                ("Level" | "Rank", Ok(n)) if n <= 13 => "★".repeat(n),
                _ => String::new(),
            };
            let badge = (label == "Attribute").then(|| {
                let name: String = value.chars().filter(char::is_ascii_alphabetic).collect();
                format!("attribute-{}", name.to_ascii_lowercase())
            });
            facts.push(Fact {
                label,
                value,
                stars,
                badge,
            });
        };

        let typeline: Vec<&str> = card["typeline"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if monster {
            // the line in brackets on the card: [Spellcaster/Pendulum/Effect]
            let line = if typeline.is_empty() {
                text(&card["race"])
            } else {
                Some(typeline.join(" / "))
            };
            fact("Type", line);
            fact("Attribute", text(&card["attribute"]));
            let level = number("level").map(|n| n.to_string());
            if let Some(link) = number("linkval") {
                fact("Link rating", Some(link.to_string()));
            } else if kind.to_ascii_lowercase().contains("xyz") {
                fact("Rank", level);
            } else {
                fact("Level", level);
            }
            fact("Pendulum scale", number("scale").map(|n| n.to_string()));
            let value = |n: Option<i64>| n.map_or("?".to_string(), |n| n.to_string());
            match number("linkval") {
                // a Link monster has no DEF
                Some(_) => fact("ATK", Some(value(number("atk")))),
                None => fact(
                    "ATK / DEF",
                    Some(format!(
                        "{} / {}",
                        value(number("atk")),
                        value(number("def"))
                    )),
                ),
            }
            let arrows: Vec<&str> = card["linkmarkers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            fact("Link arrows", Some(arrows.join(", ")));
        }
        fact("Archetype", text(&card["archetype"]));

        let frame = card["frameType"]
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_ascii_lowercase() || *c == '_')
            .collect();
        let kind = text(&card["humanReadableCardType"])
            .or_else(|| text(&card["type"]))
            .unwrap_or_default();
        Some(CardText {
            frame,
            kind,
            facts,
            parts,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn facts(card: &CardText) -> Vec<(&str, &str)> {
        card.facts
            .iter()
            .map(|f| (f.label, f.value.as_str()))
            .collect()
    }

    #[test]
    fn a_pendulum_monster() {
        let card = json!({
            "type": "Pendulum Effect Monster", "humanReadableCardType": "Pendulum Effect Monster",
            "frameType": "effect_pendulum",
            "typeline": ["Spellcaster", "Pendulum", "Effect"], "race": "Spellcaster",
            "attribute": "DARK", "level": 7, "scale": 1, "atk": 2500, "def": 2000,
            "archetype": "Magician",
            "desc": "[ Pendulum Effect ] \r\nDestroy this card.\r\n\r\n[ Monster Effect ] \r\nSummon it.",
            "pend_desc": "Destroy this card.", "monster_desc": "Summon it.\r\nThen draw.",
        });
        let card = CardText::from_card_data(&card).unwrap();
        assert_eq!(
            (card.frame.as_str(), card.kind.as_str()),
            ("effect_pendulum", "Pendulum Effect Monster")
        );
        assert_eq!(card.facts[1].badge.as_deref(), Some("attribute-dark"));
        assert_eq!(card.facts[2].stars, "★★★★★★★");
        assert!(card.facts[3].stars.is_empty() && card.facts[3].badge.is_none());
        assert_eq!(
            facts(&card),
            [
                ("Type", "Spellcaster / Pendulum / Effect"),
                ("Attribute", "DARK"),
                ("Level", "7"),
                ("Pendulum scale", "1"),
                ("ATK / DEF", "2500 / 2000"),
                ("Archetype", "Magician"),
            ]
        );
        assert_eq!(
            card.parts,
            [
                (Some("Pendulum effect"), "Destroy this card.".to_string()),
                (Some("Monster effect"), "Summon it.\nThen draw.".to_string()),
            ]
        );
    }

    #[test]
    fn link_xyz_spells() {
        let link = json!({
            "type": "Link Monster", "humanReadableCardType": "Link Effect Monster",
            "typeline": ["Machine", "Link", "Effect"], "attribute": "DARK", "linkval": 3,
            "linkmarkers": ["Top-Left", "Top", "Bottom-Right"], "atk": 2300,
            "desc": "2+ monsters",
        });
        let link = CardText::from_card_data(&link).unwrap();
        assert_eq!(
            facts(&link)[2..],
            [
                ("Link rating", "3"),
                ("ATK", "2300"),
                ("Link arrows", "Top-Left, Top, Bottom-Right"),
            ]
        );
        assert_eq!(link.parts, [(None, "2+ monsters".to_string())]);

        // no typeline (an old cache): the race; no DEF known: "?"
        let xyz = json!({ "type": "XYZ Monster", "race": "Dragon", "attribute": "LIGHT",
                          "level": 4, "atk": 2500, "def": null, "desc": "2 Level 4 monsters" });
        assert_eq!(
            facts(&CardText::from_card_data(&xyz).unwrap()),
            [
                ("Type", "Dragon"),
                ("Attribute", "LIGHT"),
                ("Rank", "4"),
                ("ATK / DEF", "2500 / ?"),
            ]
        );

        let spell = json!({ "type": "Spell Card", "humanReadableCardType": "Field Spell",
                            "race": "Field", "archetype": "Orcust", "desc": "Draw 2 cards." });
        let spell = CardText::from_card_data(&spell).unwrap();
        assert_eq!(spell.kind, "Field Spell");
        assert_eq!(facts(&spell), [("Archetype", "Orcust")]);

        assert_eq!(
            CardText::from_card_data(&json!({ "type": "Token", "desc": " " })),
            None
        );
        assert_eq!(CardText::from_card_data(&json!({})), None);
    }
}
