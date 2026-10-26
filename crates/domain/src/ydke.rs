//! YDKE deck URLs (Django: `DeckVersion.get_ydke_url` / `parse_ydke`).
//!
//! Format: `ydke://<main>!<extra>!<side>!`, each part being the base64 of the
//! ygopro card ids as consecutive little-endian u32.

use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct YdkeDeck {
    pub main: Vec<u32>,
    pub extra: Vec<u32>,
    pub side: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YdkeError {
    MissingPrefix,
    MissingParts,
    InvalidBase64,
    TruncatedId,
}

impl fmt::Display for YdkeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            YdkeError::MissingPrefix => "Invalid YDKE URL",
            YdkeError::MissingParts => "YDKE must contain main, extra, and side parts",
            YdkeError::InvalidBase64 => "YDKE part is not valid base64",
            YdkeError::TruncatedId => "YDKE part length is not a multiple of 4 bytes",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for YdkeError {}

const PREFIX: &str = "ydke://";

impl YdkeDeck {
    pub fn parse(url: &str) -> Result<YdkeDeck, YdkeError> {
        let content = url.strip_prefix(PREFIX).ok_or(YdkeError::MissingPrefix)?;
        let content = content.strip_suffix('!').unwrap_or(content);
        let parts: Vec<&str> = content.split('!').collect();
        if parts.len() < 3 {
            return Err(YdkeError::MissingParts);
        }
        Ok(YdkeDeck {
            main: decode_part(parts[0])?,
            extra: decode_part(parts[1])?,
            side: decode_part(parts[2])?,
        })
    }

    pub fn to_url(&self) -> String {
        format!(
            "{PREFIX}{}!{}!{}!",
            encode_part(&self.main),
            encode_part(&self.extra),
            encode_part(&self.side)
        )
    }
}

fn decode_part(b64: &str) -> Result<Vec<u32>, YdkeError> {
    if b64.is_empty() {
        return Ok(Vec::new());
    }
    let raw = STANDARD.decode(b64).map_err(|_| YdkeError::InvalidBase64)?;
    if raw.len() % 4 != 0 {
        return Err(YdkeError::TruncatedId);
    }
    Ok(raw
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn encode_part(ids: &[u32]) -> String {
    let bytes: Vec<u8> = ids.iter().flat_map(|id| id.to_le_bytes()).collect();
    STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // From my_ygo_cards/tests/test_import_ydke.py
    const EXODIA_DECK_YDKE: &str = "ydke://ryPeAK8j3gCvI94A1Jj9ASoL0gXBMZ8DwTGfA6y8awSsvGsEiTk/A4k5PwOJOT8DXoC3ASqVZQEqlWUBb6pPAm+qTwJvqk8Cs8MRALPDEQCzwxEAjZR4AHBQpwJS5zkE+fl7AD6kcQE+pHEBRQTgBV/wPQJf8D0CX/A9Al1p3gJdad4CXWneApiRwQOYkcEDFe6rAtxgaAHcYGgBZ7n1BQ==!+0rlBCisxwJIiLMAfolDA7VMKwQmGXYDamj2BGpo9gRqaPYEMpglAHvs+AKkmisAt95HBPZQFwC6TtkF!!";

    #[test]
    fn parses_exodia_deck() {
        let deck = YdkeDeck::parse(EXODIA_DECK_YDKE).unwrap();
        assert_eq!(deck.main.len(), 40);
        assert_eq!(deck.extra.len(), 15);
        assert!(deck.side.is_empty());
        // Ash Blossom & Joyous Spring
        assert!(deck.main.contains(&14558127));
    }

    #[test]
    fn round_trips() {
        let deck = YdkeDeck::parse(EXODIA_DECK_YDKE).unwrap();
        assert_eq!(deck.to_url(), EXODIA_DECK_YDKE);
    }

    #[test]
    fn empty_deck() {
        let deck = YdkeDeck::default();
        assert_eq!(deck.to_url(), "ydke://!!!");
        assert_eq!(YdkeDeck::parse("ydke://!!!").unwrap(), deck);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(YdkeDeck::parse("http://x"), Err(YdkeError::MissingPrefix));
        assert_eq!(YdkeDeck::parse("ydke://abc!"), Err(YdkeError::MissingParts));
        assert_eq!(
            YdkeDeck::parse("ydke://***!!!"),
            Err(YdkeError::InvalidBase64)
        );
        // "AQI=" = 2 bytes
        assert_eq!(
            YdkeDeck::parse("ydke://AQI=!!!"),
            Err(YdkeError::TruncatedId)
        );
    }
}
