//! Card rarities (`card.rarity`): not in Django, which only kept the rarity
//! in the Cardmarket names (`Raigeki (V.4 - Ultra Rare)`).
//!
//! The stored value is the label (`Ultra Rare`), from the names ygoprodeck
//! uses; there is no CHECK constraint on the column, the forms validate
//! against [`Rarity::ALL`]. The frontend guide (`static/js/rarities.js`)
//! describes the same names.

use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rarity {
    Common,
    ShortPrint,
    SuperShortPrint,
    Rare,
    SuperRare,
    UltraRare,
    UltraRarePharaohs,
    UltimateRare,
    SecretRare,
    UltraSecretRare,
    ExtraSecretRare,
    TenThousandSecretRare,
    GhostRare,
    GoldRare,
    GoldSecretRare,
    GhostGoldRare,
    PremiumGoldRare,
    PlatinumRare,
    PlatinumSecretRare,
    PrismaticSecretRare,
    CollectorsRare,
    StarlightRare,
    QuarterCenturySecretRare,
    GrandMasterRare,
    StarfoilRare,
    ShatterfoilRare,
    MosaicRare,
    NormalParallelRare,
    SuperParallelRare,
    UltraParallelRare,
    DuelTerminalNormalParallelRare,
    DuelTerminalRareParallelRare,
    DuelTerminalSuperParallelRare,
    DuelTerminalUltraParallelRare,
}

use Rarity::*;

impl Rarity {
    /// In the order of the select of the forms: the common ones first.
    pub const ALL: [Rarity; 34] = [
        Common,
        Rare,
        SuperRare,
        UltraRare,
        SecretRare,
        UltimateRare,
        CollectorsRare,
        StarlightRare,
        QuarterCenturySecretRare,
        PlatinumSecretRare,
        PrismaticSecretRare,
        GhostRare,
        GoldRare,
        GoldSecretRare,
        GhostGoldRare,
        PremiumGoldRare,
        PlatinumRare,
        UltraSecretRare,
        ExtraSecretRare,
        TenThousandSecretRare,
        UltraRarePharaohs,
        GrandMasterRare,
        ShortPrint,
        SuperShortPrint,
        StarfoilRare,
        ShatterfoilRare,
        MosaicRare,
        NormalParallelRare,
        SuperParallelRare,
        UltraParallelRare,
        DuelTerminalNormalParallelRare,
        DuelTerminalRareParallelRare,
        DuelTerminalSuperParallelRare,
        DuelTerminalUltraParallelRare,
    ];

    /// The value stored in `card.rarity`.
    pub fn label(self) -> &'static str {
        match self {
            Common => "Common",
            ShortPrint => "Short Print",
            SuperShortPrint => "Super Short Print",
            Rare => "Rare",
            SuperRare => "Super Rare",
            UltraRare => "Ultra Rare",
            UltraRarePharaohs => "Ultra Rare (Pharaoh's Rare)",
            UltimateRare => "Ultimate Rare",
            SecretRare => "Secret Rare",
            UltraSecretRare => "Ultra Secret Rare",
            ExtraSecretRare => "Extra Secret Rare",
            TenThousandSecretRare => "10000 Secret Rare",
            GhostRare => "Ghost Rare",
            GoldRare => "Gold Rare",
            GoldSecretRare => "Gold Secret Rare",
            GhostGoldRare => "Ghost/Gold Rare",
            PremiumGoldRare => "Premium Gold Rare",
            PlatinumRare => "Platinum Rare",
            PlatinumSecretRare => "Platinum Secret Rare",
            PrismaticSecretRare => "Prismatic Secret Rare",
            CollectorsRare => "Collector's Rare",
            StarlightRare => "Starlight Rare",
            QuarterCenturySecretRare => "Quarter Century Secret Rare",
            GrandMasterRare => "Grand Master Rare",
            StarfoilRare => "Starfoil Rare",
            ShatterfoilRare => "Shatterfoil Rare",
            MosaicRare => "Mosaic Rare",
            NormalParallelRare => "Normal Parallel Rare",
            SuperParallelRare => "Super Parallel Rare",
            UltraParallelRare => "Ultra Parallel Rare",
            DuelTerminalNormalParallelRare => "Duel Terminal Normal Parallel Rare",
            DuelTerminalRareParallelRare => "Duel Terminal Rare Parallel Rare",
            DuelTerminalSuperParallelRare => "Duel Terminal Super Parallel Rare",
            DuelTerminalUltraParallelRare => "Duel Terminal Ultra Parallel Rare",
        }
    }

    /// ygoprodeck's short code (in brackets after its rarity names), when
    /// there is one.
    pub fn code(self) -> Option<&'static str> {
        Some(match self {
            Common => "C",
            ShortPrint => "SP",
            SuperShortPrint => "SSP",
            Rare => "R",
            SuperRare => "SR",
            UltraRare => "UR",
            UltimateRare => "UtR",
            SecretRare => "ScR",
            UltraSecretRare => "UScR",
            TenThousandSecretRare => "10000ScR",
            GhostRare => "GR",
            GoldRare => "GUR",
            GoldSecretRare => "GScR",
            GhostGoldRare => "GGR",
            PremiumGoldRare => "PG",
            PlatinumRare => "PIR",
            PlatinumSecretRare => "PS",
            PrismaticSecretRare => "PScR",
            CollectorsRare => "CR",
            StarlightRare => "StR",
            StarfoilRare => "SFR",
            ShatterfoilRare => "SHR",
            MosaicRare => "MSR",
            SuperParallelRare => "SPR",
            UltraParallelRare => "UPR",
            DuelTerminalNormalParallelRare => "DNPR",
            DuelTerminalRareParallelRare => "DRPR",
            DuelTerminalSuperParallelRare => "DSPR",
            DuelTerminalUltraParallelRare => "DUPR",
            UltraRarePharaohs
            | ExtraSecretRare
            | QuarterCenturySecretRare
            | GrandMasterRare
            | NormalParallelRare => return None,
        })
    }

    /// A rarity name as written by ygoprodeck, Cardmarket or a person:
    /// case, spaces and punctuation are ignored (`Collectors Rare`,
    /// `PLatinum Secret Rare`), and a few other spellings are accepted.
    pub fn from_label(name: &str) -> Option<Rarity> {
        let wanted = key(name);
        if wanted.is_empty() {
            return None;
        }
        let alias = match wanted.as_str() {
            "extrasecret" => Some(ExtraSecretRare),
            "starfoil" => Some(StarfoilRare),
            "pharaohsrare" => Some(UltraRarePharaohs),
            _ => None,
        };
        alias.or_else(|| Rarity::ALL.into_iter().find(|r| key(r.label()) == wanted))
    }

    /// A short code (`UR`, `ScR`…), case-insensitive.
    pub fn from_code(code: &str) -> Option<Rarity> {
        Rarity::ALL
            .into_iter()
            .find(|r| r.code().is_some_and(|c| c.eq_ignore_ascii_case(code)))
    }
}

impl Rarity {
    /// The rarity code of a Cardmarket shipment line. Cardmarket has its own
    /// codes for some rarities (matched against the real shipments on
    /// 2026-10-18) and uses ygoprodeck's for the others.
    pub fn from_cardmarket_code(code: &str) -> Option<Rarity> {
        match code.to_ascii_uppercase().as_str() {
            "SUR" => Some(SuperRare),
            "SER" => Some(SecretRare),
            "QSCR" => Some(QuarterCenturySecretRare),
            "PSCR" => Some(PlatinumSecretRare),
            "SLR" => Some(StarlightRare),
            "PGR" => Some(PremiumGoldRare),
            _ => Rarity::from_code(code),
        }
    }
}

/// Lowercase letters and digits only.
fn key(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

static VERSION_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<name>.+?)\s*\(V\.\d+\s*-\s*(?P<rarity>[^()]+)\)\s*$").unwrap()
});

/// Split a Cardmarket name with a version suffix:
/// `"Raigeki (V.4 - Ultra Rare)"` → `("Raigeki", Some(UltraRare))`.
///
/// A name without the suffix, or with an unknown rarity in it, is returned
/// whole with `None`.
pub fn split_cardmarket_name(name: &str) -> (&str, Option<Rarity>) {
    let Some(c) = VERSION_SUFFIX.captures(name) else {
        return (name, None);
    };
    match Rarity::from_label(&c["rarity"]) {
        Some(rarity) => (c.name("name").unwrap().as_str(), Some(rarity)),
        None => (name, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip() {
        for r in Rarity::ALL {
            assert_eq!(Rarity::from_label(r.label()), Some(r), "{r:?}");
        }
    }

    #[test]
    fn codes_round_trip_and_are_unique() {
        for r in Rarity::ALL {
            if let Some(code) = r.code() {
                assert_eq!(Rarity::from_code(code), Some(r), "{code}");
            }
        }
        assert_eq!(Rarity::from_code("ur"), Some(UltraRare));
        assert_eq!(Rarity::from_code("XX"), None);
    }

    #[test]
    fn cardmarket_codes() {
        assert_eq!(Rarity::from_cardmarket_code("SUR"), Some(SuperRare));
        assert_eq!(Rarity::from_cardmarket_code("SER"), Some(SecretRare));
        assert_eq!(
            Rarity::from_cardmarket_code("QSCR"),
            Some(QuarterCenturySecretRare)
        );
        assert_eq!(Rarity::from_cardmarket_code("PIR"), Some(PlatinumRare));
        assert_eq!(Rarity::from_cardmarket_code("UR"), Some(UltraRare));
        assert_eq!(Rarity::from_cardmarket_code("T"), None);
    }

    #[test]
    fn labels_ignore_case_and_punctuation() {
        assert_eq!(Rarity::from_label("Collectors Rare"), Some(CollectorsRare));
        assert_eq!(
            Rarity::from_label("PLatinum Secret Rare"),
            Some(PlatinumSecretRare)
        );
        assert_eq!(Rarity::from_label("ultra rare"), Some(UltraRare));
        assert_eq!(Rarity::from_label("Extra Secret"), Some(ExtraSecretRare));
        assert_eq!(Rarity::from_label("Starfoil"), Some(StarfoilRare));
    }

    #[test]
    fn digits_count() {
        assert_eq!(
            Rarity::from_label("10000 Secret Rare"),
            Some(TenThousandSecretRare)
        );
        assert_eq!(Rarity::from_label("Secret Rare"), Some(SecretRare));
    }

    #[test]
    fn notes_are_not_rarities() {
        for note in ["New", "2", "Reprint", "New artwork", "European debut", ""] {
            assert_eq!(Rarity::from_label(note), None, "{note}");
        }
    }

    #[test]
    fn splits_cardmarket_names() {
        assert_eq!(
            split_cardmarket_name("Raigeki (V.4 - Ultra Rare)"),
            ("Raigeki", Some(UltraRare))
        );
        assert_eq!(
            split_cardmarket_name(
                "Ash Blossom & Joyous Spring (V.2 - Quarter Century Secret Rare)"
            ),
            (
                "Ash Blossom & Joyous Spring",
                Some(QuarterCenturySecretRare)
            )
        );
        assert_eq!(
            split_cardmarket_name("Kuriboh (V.6 - Collectors Rare)"),
            ("Kuriboh", Some(CollectorsRare))
        );
    }

    #[test]
    fn names_with_a_dash_keep_it() {
        assert_eq!(
            split_cardmarket_name("Number 39: Utopia - Beyond (V.1 - Super Rare)"),
            ("Number 39: Utopia - Beyond", Some(SuperRare))
        );
    }

    #[test]
    fn names_without_a_known_suffix_are_whole() {
        assert_eq!(split_cardmarket_name("Raigeki"), ("Raigeki", None));
        assert_eq!(
            split_cardmarket_name("Raigeki (V.4 - Shiny)"),
            ("Raigeki (V.4 - Shiny)", None)
        );
        assert_eq!(
            split_cardmarket_name("Raigeki (Limited Edition)"),
            ("Raigeki (Limited Edition)", None)
        );
    }
}
