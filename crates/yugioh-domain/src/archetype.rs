//! Archetypes (new, not in Django): ygoprodeck's list of archetype names
//! (`archetypes.php`), offered as names for the opponents' decks, and the
//! key that makes a typed label match one (`snake eye` = `Snake-Eye`), so a
//! label takes the archetype's spelling.

use serde_json::Value;

/// The archetype names of ygoprodeck's `archetypes.php`
/// (`[{"archetype_name": "…"}, …]`), trimmed, blanks left out, and the
/// fragments of names too (`-Eyes Dragon`, `/Assault Mode`, `Abyss-`): no deck
/// is called that, and `Abyss-` would take the label `Abyss`.
pub fn names(answer: &Value) -> Vec<String> {
    answer
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["archetype_name"].as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty() && !is_fragment(name))
        .map(str::to_string)
        .collect()
}

/// A piece of card names rather than a name: starts with `-` or `/`, or
/// ends with `-`.
fn is_fragment(name: &str) -> bool {
    name.starts_with(['-', '/']) || name.ends_with('-')
}

/// What two names must share to be the same archetype: their letters and
/// digits, lower case. Spaces, hyphens and punctuation don't count.
pub fn key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The labels to rename to an archetype's spelling: `(label id, new name)`
/// for each label whose [`key`] is an archetype's and whose name differs
/// from it (case included). `archetypes` are `(name, key)`; when two share a
/// key, the first wins.
pub fn relabels(labels: &[(i64, String)], archetypes: &[(String, String)]) -> Vec<(i64, String)> {
    labels
        .iter()
        .filter_map(|(id, name)| {
            let k = key(name);
            if k.is_empty() {
                return None;
            }
            let (archetype, _) = archetypes.iter().find(|(_, a)| *a == k)?;
            (archetype != name).then(|| (*id, archetype.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn names_from_the_answer() {
        let answer = json!([
            {"archetype_name": "Snake-Eye"},
            {"archetype_name": " @Ignister "},
            {"archetype_name": ""},
            {"archetype_name": "-Eyes Dragon"},
            {"archetype_name": "/Assault Mode"},
            {"archetype_name": "Abyss-"},
            {"archetype_name": "\"C\""},
            {"other": 1}
        ]);
        assert_eq!(names(&answer), ["Snake-Eye", "@Ignister", "\"C\""]);
        assert!(names(&json!({"error": "x"})).is_empty());
    }

    #[test]
    fn keys_ignore_case_spaces_and_punctuation() {
        assert_eq!(key("Snake-Eye"), "snakeeye");
        assert_eq!(key("snake eye"), key("Snake-Eye"));
        assert_eq!(key("D/D/D"), "ddd");
        assert_eq!(key("@Ignister"), key("ignister"));
        assert_eq!(key("Tri-Brigade"), key("TRI BRIGADE"));
        assert_ne!(key("Snake-Eye Fire King"), key("Snake-Eye"));
        assert_eq!(key(" - "), "");
    }

    #[test]
    fn relabels_only_the_labels_spelt_otherwise() {
        let archetypes: Vec<(String, String)> = ["Snake-Eye", "Tenpai Dragon", "Labrynth"]
            .iter()
            .map(|n| (n.to_string(), key(n)))
            .collect();
        let labels = vec![
            (1, "snake eye".to_string()),
            (2, "Tenpai Dragon".to_string()),
            (3, "Snake-Eye Fire King".to_string()),
            (4, "labrynth".to_string()),
            (5, "-".to_string()),
        ];
        assert_eq!(
            relabels(&labels, &archetypes),
            [(1, "Snake-Eye".to_string()), (4, "Labrynth".to_string())]
        );
    }
}
