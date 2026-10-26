//! The [Pokémon TCG API](https://pokemontcg.io) (free, no key): only for
//! the pictures TCGdex doesn't have yet (Trainer Galleries, some promos,
//! the Classic Collection reprints). Its pictures are the right print's:
//! looked up by set and number, or by name and artist for a reprint (it
//! numbers those by the card they reprint: 33 for Pikachu & Zekrom-GX).
//!
//! The service is being replaced by a paid one (Scrydex) and its API
//! answers 500 or 502 to about half the requests (checked 2026-10-26),
//! while its image server is reliable. So the picture is first taken
//! straight from the image server when the set's id and the number are
//! the same as TCGdex's (`swsh10tg/TG29`, `swshp/SWSH282`); the API is only
//! asked otherwise (a reprint, another set id), each request retried up to
//! [`ATTEMPTS`] times, the set list kept a day. A failure is not
//! remembered as "no picture" (`cards::find_image` tries again later).

use std::time::{Duration, Instant};

use domain::pokemon::fold_name;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::web::error::AppError;

const BASE_URL: &str = "https://api.pokemontcg.io/v2";
const IMAGES_URL: &str = "https://images.pokemontcg.io";
const TIMEOUT: Duration = Duration::from_secs(20);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(1000);
const ATTEMPTS: u32 = 6;
const SETS_KEPT: Duration = Duration::from_secs(24 * 3600);

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::const_new(None);

async fn throttle() {
    let mut last = LAST_REQUEST.lock().await;
    if let Some(at) = *last {
        tokio::time::sleep_until((at + MIN_REQUEST_INTERVAL).into()).await;
    }
    *last = Some(Instant::now());
}

fn error(why: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("Pokémon TCG API: {why}"))
}

/// `GET {BASE_URL}/{path}`, its `data`; retried on a server error.
async fn get(
    http: &reqwest::Client,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Vec<Value>, AppError> {
    let mut why = String::new();
    for attempt in 1..=ATTEMPTS {
        throttle().await;
        let answer = http
            .get(format!("{BASE_URL}/{path}"))
            .query(query)
            .timeout(TIMEOUT)
            .send()
            .await;
        match answer {
            Ok(response) if response.status().is_success() => {
                let body: Value = response.json().await.map_err(error)?;
                return Ok(body["data"].as_array().cloned().unwrap_or_default());
            }
            Ok(response) if response.status().is_client_error() && response.status() != 429 => {
                return Err(error(format!("HTTP {}", response.status())));
            }
            Ok(response) => why = format!("HTTP {}", response.status()),
            Err(e) => why = e.to_string(),
        }
        if attempt < ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    Err(error(format!("{why} ({ATTEMPTS} tries)")))
}

/// The API's sets (id, name, code, size), asked once a day.
async fn sets(http: &reqwest::Client) -> Result<Vec<Value>, AppError> {
    static SETS: Mutex<Option<(Instant, Vec<Value>)>> = Mutex::const_new(None);
    let mut known = SETS.lock().await;
    if let Some((at, sets)) = known.as_ref() {
        if at.elapsed() < SETS_KEPT {
            return Ok(sets.clone());
        }
    }
    let sets = get(
        http,
        "sets",
        &[
            ("select", "id,name,ptcgoCode,printedTotal,total"),
            ("pageSize", "250"),
        ],
    )
    .await?;
    *known = Some((Instant::now(), sets.clone()));
    Ok(sets)
}

/// The image server's picture at TCGdex's set id and number, written its
/// way (`swsh10tg/TG29_hires.png`), when it exists: `(id, URL)`.
async fn picture_at_same_place(
    http: &reqwest::Client,
    wanted: &Wanted<'_>,
) -> Option<(String, String)> {
    let set = their_set_id(wanted.set_id);
    let number = wanted.number.trim_start_matches('0');
    let url = format!("{IMAGES_URL}/{set}/{number}_hires.png");
    let response = http.head(&url).timeout(TIMEOUT).send().await.ok()?;
    let is_png = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .is_some_and(|t| t.as_bytes().starts_with(b"image/"));
    (response.status().is_success() && is_png).then(|| (format!("{set}-{number}"), url))
}

/// What is known of the card whose picture is wanted (TCGdex's English
/// data).
#[derive(Debug, Clone)]
pub struct Wanted<'a> {
    pub name: &'a str,
    pub artist: Option<&'a str>,
    pub number: &'a str,
    pub set_id: &'a str,
    pub set_name: &'a str,
    /// The code printed on the cards (`ASR`, `30C`) and the set size.
    pub set_code: Option<&'a str>,
    pub official: Option<i64>,
}

/// The card of the Pokémon TCG API that is this print: `(its id, its
/// large picture's URL)`, `None` when it has none.
pub async fn find(
    http: &reqwest::Client,
    wanted: &Wanted<'_>,
) -> Result<Option<(String, String)>, AppError> {
    if let Some(found) = picture_at_same_place(http, wanted).await {
        return Ok(Some(found));
    }

    let sets = sets(http).await?;
    for set_id in matching_sets(&sets, wanted) {
        let number = wanted.number.trim_start_matches('0');
        let by_number = get(
            http,
            "cards",
            &[
                ("q", &format!("set.id:{set_id} number:{number}")),
                ("select", "id,name,number,artist,images"),
            ],
        )
        .await?;
        if let Some(found) = choose(&by_number, wanted, true) {
            return Ok(Some(found));
        }

        // a reprint keeps the number of the card it reprints: by name
        let first_word = wanted.name.split_whitespace().next().unwrap_or(wanted.name);
        let by_name = get(
            http,
            "cards",
            &[
                ("q", &format!("set.id:{set_id} name:\"{first_word}*\"")),
                ("select", "id,name,number,artist,images"),
            ],
        )
        .await?;
        if let Some(found) = choose(&by_name, wanted, false) {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// The Pokémon TCG API's sets that can be TCGdex's: same printed code and
/// size (30th Celebration's Classic Collection is `30C`, 30 cards in both),
/// same name, or the same id written their way (`sv06.5` → `sv6pt5`).
fn matching_sets(sets: &[Value], wanted: &Wanted<'_>) -> Vec<String> {
    let their_id = their_set_id(wanted.set_id);
    let mut found: Vec<String> = Vec::new();
    let mut add = |set: &Value| {
        if let Some(id) = set["id"].as_str() {
            if !found.iter().any(|known| known == id) {
                found.push(id.to_string());
            }
        }
    };

    for set in sets {
        let same_code = match (wanted.set_code, set["ptcgoCode"].as_str()) {
            (Some(code), Some(theirs)) => code.eq_ignore_ascii_case(theirs),
            _ => false,
        };
        // some sets have no printedTotal (the Classic Collection): total
        let size = set["printedTotal"].as_i64().or(set["total"].as_i64());
        let same_size = wanted.official.is_some() && size == wanted.official;
        if same_code && same_size {
            add(set);
        }
    }
    for set in sets {
        let same_name = set["name"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(wanted.set_name));
        let same_id = set["id"].as_str().is_some_and(|id| id == their_id);
        if same_name || same_id {
            add(set);
        }
    }
    found
}

/// TCGdex's set id written the Pokémon TCG API's way: `sv06.5` → `sv6pt5`,
/// `me01` → `me1` (the Sword & Shield ones are the same).
fn their_set_id(tcgdex: &str) -> String {
    let id = tcgdex.replace(".5", "pt5");
    let letters: String = id.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let rest = id[letters.len()..].trim_start_matches('0');
    if rest.is_empty() || letters.is_empty() {
        id
    } else {
        format!("{letters}{rest}")
    }
}

/// The card among the answers: same name (and number, when asked by
/// number), the same artist's first.
fn choose(cards: &[Value], wanted: &Wanted<'_>, by_number: bool) -> Option<(String, String)> {
    let name = fold_name(wanted.name);
    let candidates: Vec<&Value> = cards
        .iter()
        .filter(|card| card["name"].as_str().is_some_and(|n| fold_name(n) == name))
        .collect();
    let same_artist = |card: &Value| {
        wanted
            .artist
            .is_some_and(|artist| card["artist"].as_str() == Some(artist))
    };
    // asked by number, one answer spelt otherwise: the card if its artist
    // and its name's first word are the same (not another card with that
    // number: Mitsuhiro Arita drew many)
    let first_word = |text: &str| {
        fold_name(text)
            .split(' ')
            .next()
            .unwrap_or_default()
            .to_string()
    };
    let candidates = if candidates.is_empty() && by_number && cards.len() == 1 {
        cards
            .iter()
            .filter(|card| same_artist(card))
            .filter(|card| {
                card["name"]
                    .as_str()
                    .is_some_and(|n| first_word(n) == first_word(wanted.name))
            })
            .collect()
    } else {
        candidates
    };
    let card = candidates
        .iter()
        .copied()
        .find(|card| same_artist(card))
        .or(if by_number {
            candidates.first().copied()
        } else {
            None
        })?;
    Some((
        card["id"].as_str()?.to_string(),
        card["images"]["large"].as_str()?.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn wanted<'a>(
        name: &'a str,
        number: &'a str,
        set_id: &'a str,
        set_name: &'a str,
    ) -> Wanted<'a> {
        Wanted {
            name,
            artist: Some("Mitsuhiro Arita"),
            number,
            set_id,
            set_name,
            set_code: Some("30C"),
            official: Some(30),
        }
    }

    #[test]
    fn set_ids_their_way() {
        assert_eq!(their_set_id("sv06.5"), "sv6pt5");
        assert_eq!(their_set_id("sv01"), "sv1");
        assert_eq!(their_set_id("me01"), "me1");
        assert_eq!(their_set_id("swsh10tg"), "swsh10tg");
        assert_eq!(their_set_id("swshp"), "swshp");
    }

    #[test]
    fn sets_match_by_code_and_size_then_name() {
        let sets = [
            json!({"id": "me55", "name": "30th Celebration", "ptcgoCode": "30C", "printedTotal": 128}),
            json!({"id": "me55c", "name": "30th Celebration: Classic Collection",
                   "ptcgoCode": "30C", "total": 30}),
            json!({"id": "swsh10tg", "name": "Astral Radiance Trainer Gallery",
                   "ptcgoCode": "ASR", "printedTotal": 30}),
        ];
        let classic = wanted(
            "Pikachu & Zekrom GX",
            "008",
            "30th-c",
            "30th Classic Collection",
        );
        assert_eq!(matching_sets(&sets, &classic), ["me55c"]);
        let gallery = Wanted {
            set_code: Some("ASR:TG"),
            ..wanted(
                "Ice Rider Calyrex VMAX",
                "TG29",
                "swsh10tg",
                "Astral Radiance Trainer Gallery",
            )
        };
        assert_eq!(matching_sets(&sets, &gallery), ["swsh10tg"]);
    }

    #[test]
    fn a_reprint_is_chosen_by_name_and_artist() {
        let cards = [
            json!({"id": "me55c-58", "name": "Pikachu", "artist": "Mitsuhiro Arita",
                   "images": {"large": "https://x/me55c-58/large"}}),
            json!({"id": "me55c-33", "name": "Pikachu & Zekrom-GX", "artist": "Mitsuhiro Arita",
                   "images": {"large": "https://x/me55c-33/large"}}),
        ];
        let classic = wanted(
            "Pikachu & Zekrom GX",
            "008",
            "30th-c",
            "30th Classic Collection",
        );
        assert_eq!(
            choose(&cards, &classic, false),
            Some(("me55c-33".into(), "https://x/me55c-33/large".into()))
        );
        assert_eq!(
            choose(&cards[..1], &classic, false),
            None,
            "not the same name"
        );

        let one = [json!({"id": "swshp-SWSH282", "name": "Galarian Articuno",
                          "artist": "Someone", "images": {"large": "https://x/282"}})];
        let promo = Wanted {
            artist: Some("Other"),
            ..wanted(
                "Galarian Articuno",
                "SWSH282",
                "swshp",
                "SWSH Black Star Promos",
            )
        };
        assert_eq!(choose(&one, &promo, true).unwrap().0, "swshp-SWSH282");

        // a Classic Collection reprint asked by TCGdex's number (8): number
        // 8 there is another card
        let eight = [
            json!({"id": "me55c-8", "name": "Raichu", "artist": "Ken Sugimori",
                            "images": {"large": "https://x/8"}}),
        ];
        assert_eq!(choose(&eight, &classic, true), None);
        let arita = [
            json!({"id": "me55c-8", "name": "Charizard", "artist": "Mitsuhiro Arita",
                            "images": {"large": "https://x/8"}}),
        ];
        assert_eq!(
            choose(&arita, &classic, true),
            None,
            "same artist, another card"
        );
    }
}
