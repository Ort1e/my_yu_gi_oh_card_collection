//! [Limitless TCG](https://limitlesstcg.com)'s card pictures, the official
//! scans in English and French, for the prints TCGdex has no picture of
//! yet (recent promos: `MEP`). Only used then, before the Pokémon TCG API
//! (`ptcgio.rs`), which has no recent set at all.
//!
//! The scans sit at `tpci/{code}/{code}_{number on 3 digits}_R_{EN|FR}_LG.png`
//! on their file server, `code` being the set's printed code (TCGdex's
//! `abbreviation.official`: `MEP`, `MEG`, `SFA`). A print that isn't there
//! answers 404: a `HEAD` tells, nothing is downloaded here.

use std::time::Duration;

use common::http::Throttle;

const BASE_URL: &str = "https://limitlesstcg.nyc3.digitaloceanspaces.com/tpci";
const TIMEOUT: Duration = Duration::from_secs(15);
static LIMITLESS: Throttle = Throttle::new(Duration::from_millis(500));

/// What Limitless has of one print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// `MEP_057`: kept in `image_from` (`limitless:MEP_057`).
    pub id: String,
    pub en: Option<String>,
    pub fr: Option<String>,
}

/// The scan's URL: a numeric number is padded to 3 digits (`57` → `057`),
/// a lettered one (`TG05`) stays as it is. `lang` is `EN` or `FR`.
pub fn picture_url(code: &str, number: &str, lang: &str) -> String {
    format!(
        "{BASE_URL}/{code}/{code}_{}_R_{lang}_LG.png",
        padded(number)
    )
}

fn padded(number: &str) -> String {
    if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
        format!("{number:0>3}")
    } else {
        number.to_string()
    }
}

/// `url` when it answers with an image.
async fn existing(http: &reqwest::Client, url: String) -> Option<String> {
    LIMITLESS.wait().await;
    let response = http.head(&url).timeout(TIMEOUT).send().await.ok()?;
    let is_image = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .is_some_and(|t| t.as_bytes().starts_with(b"image/"));
    (response.status().is_success() && is_image).then_some(url)
}

/// The English and French scans of a print; `None` when there is neither
/// (or the service can't be reached: the caller moves on).
pub async fn find(http: &reqwest::Client, code: &str, number: &str) -> Option<Found> {
    let en = existing(http, picture_url(code, number, "EN")).await;
    let fr = existing(http, picture_url(code, number, "FR")).await;
    if en.is_none() && fr.is_none() {
        return None;
    }
    Some(Found {
        id: format!("{code}_{}", padded(number)),
        en,
        fr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_pad_numbers() {
        assert!(picture_url("MEP", "057", "EN").ends_with("/MEP/MEP_057_R_EN_LG.png"));
        assert!(picture_url("MEP", "57", "FR").ends_with("/MEP/MEP_057_R_FR_LG.png"));
        assert!(picture_url("ASR", "TG05", "EN").ends_with("/ASR/ASR_TG05_R_EN_LG.png"));
    }
}
