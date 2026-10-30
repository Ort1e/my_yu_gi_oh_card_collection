//! Pictures of a card's printing, from Yugipedia's card galleries (not in
//! Django). ygoprodeck has one picture per artwork, in English; Yugipedia
//! has one per printing, named after it:
//! `WitnessoftheAncient-CORI-FR-UR-1E-EA.png` is the French Ultra Rare of
//! Chaos Origins, 1st edition, Extended Art.
//!
//! A copy (`CORI-FR081`, Ultra Rare) is matched by set, language and rarity
//! ([`candidates`]); the edition isn't noted in the collection, so it is
//! ignored. A picture is picked by itself ([`automatic`]) only when the
//! printing has one artwork: the normal and Extended (or alternate) Art
//! prints share their code and rarity, so between them a person chooses.

use crate::card::Language;
use crate::rarity::Rarity;

/// A file of a Yugipedia card gallery, read from its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureFile<'a> {
    /// The file name, as given.
    pub file: &'a str,
    /// `CORI`.
    pub set: &'a str,
    /// Yugipedia's language: `EN`, `FR`, `FC` (French Canadian), `NA`…
    pub language: &'a str,
    /// Yugipedia's rarity code: `UR`, `QCScR`…
    pub rarity: &'a str,
    /// `1E`, `UE` (unlimited), `LE` (limited), when given.
    pub edition: Option<&'a str>,
    /// What else the name says: `EA` (Extended Art), `AA` (alternate art),
    /// `OP` (official proxy), `2` (another print)…
    pub extras: Vec<&'a str>,
}

impl PictureFile<'_> {
    /// The artwork: the name without its edition (`UR` and `UR-EA` are two
    /// artworks, `UR-1E` and `UR-UE` the same one).
    fn artwork(&self) -> (&str, &[&str]) {
        (self.rarity, &self.extras)
    }
}

/// Read a gallery file name: `<Name>-<SET>-<LANG>-<RARITY>[-<EDITION>][-<extra>…].<ext>`
/// (an optional `File:` first). `None` for the other files of a gallery:
/// the anime, manga and video game pictures (`FoolishBurial-EN-Anime-DM`,
/// `FoolishBurial-LD-EN-VG`), the artwork alone (`FoolishBurial-OW`).
pub fn parse(file: &str) -> Option<PictureFile<'_>> {
    let name = file.strip_prefix("File:").unwrap_or(file);
    let (stem, extension) = name.rsplit_once('.')?;
    if !["png", "jpg", "jpeg"].contains(&extension.to_ascii_lowercase().as_str()) {
        return None;
    }

    // the card's name never has a hyphen in these names
    let mut parts = stem.split('-').skip(1);
    let set = parts.next()?;
    let language = parts.next()?;
    let rarity = parts.next()?;
    let printed = |part: &str| {
        !part.is_empty()
            && part.chars().all(|c| c.is_ascii_alphanumeric())
            && part.chars().any(|c| c.is_ascii_uppercase())
    };
    let is_language = language.len() == 2 && language.chars().all(|c| c.is_ascii_uppercase());
    if !printed(set) || !is_language || !printed(rarity) || rarity == "VG" {
        return None;
    }

    let mut rest: Vec<&str> = parts.collect();
    let edition = match rest.first() {
        Some(&e @ ("1E" | "UE" | "LE")) => {
            rest.remove(0);
            Some(e)
        }
        _ => None,
    };
    Some(PictureFile {
        file,
        set,
        language,
        rarity,
        edition,
        extras: rest,
    })
}

/// Yugipedia's code of a rarity (not always ygoprodeck's: Premium Gold Rare
/// is `PGR` there, `PG` at ygoprodeck).
pub fn rarity_code(rarity: Rarity) -> &'static str {
    use Rarity::*;
    match rarity {
        Common => "C",
        ShortPrint => "SP",
        SuperShortPrint => "SSP",
        Rare => "R",
        SuperRare => "SR",
        UltraRare => "UR",
        UltraRarePharaohs => "URPR",
        UltimateRare => "UtR",
        SecretRare => "ScR",
        UltraSecretRare => "UScR",
        ExtraSecretRare => "EScR",
        TenThousandSecretRare => "10000ScR",
        GhostRare => "GR",
        GoldRare => "GUR",
        GoldSecretRare => "GScR",
        GhostGoldRare => "GGR",
        PremiumGoldRare => "PGR",
        PlatinumRare => "PlR",
        PlatinumSecretRare => "PlScR",
        PrismaticSecretRare => "PScR",
        CollectorsRare => "CR",
        StarlightRare => "StR",
        QuarterCenturySecretRare => "QCScR",
        GrandMasterRare => "GMR",
        StarfoilRare => "SFR",
        ShatterfoilRare => "SHR",
        MosaicRare => "MSR",
        NormalParallelRare => "NPR",
        SuperParallelRare => "SPR",
        UltraParallelRare => "UPR",
        DuelTerminalNormalParallelRare => "DNPR",
        DuelTerminalRareParallelRare => "DRPR",
        DuelTerminalSuperParallelRare => "DSPR",
        DuelTerminalUltraParallelRare => "DUPR",
    }
}

/// Yugipedia's languages for a copy's language, the best first: French is
/// `FR` (then `FC`, the Canadian print), English `EN` (then `NA` and `EU`,
/// the North American and European prints of the old sets). A code without
/// language (`SDY-046`) is an old English one.
fn languages(code: &str) -> &'static [&'static str] {
    match Language::of_code(code) {
        Some(Language::English) | None => &["EN", "NA", "EU"],
        Some(Language::French) => &["FR", "FC"],
        Some(Language::German) => &["DE"],
        Some(Language::Italian) => &["IT"],
        Some(Language::Spanish) => &["SP"],
        Some(Language::Portuguese) => &["PT"],
    }
}

/// The pictures of a copy's printing among a gallery's `files`: the same
/// set and language (its best one present: `FR` before `FC`), the same
/// rarity when known; not the official proxies (`OP`). In the gallery's
/// order, 1st edition first.
pub fn candidates<'a>(files: &'a [String], code: &str, rarity: Option<Rarity>) -> Vec<&'a str> {
    let Some((set, _)) = code.trim().split_once('-') else {
        return Vec::new();
    };
    let set = set.to_ascii_uppercase();
    let wanted_rarity = rarity.map(rarity_code);

    let printing: Vec<PictureFile> = files
        .iter()
        .filter_map(|f| parse(f))
        .filter(|p| p.set == set)
        .filter(|p| wanted_rarity.is_none_or(|r| p.rarity == r))
        .filter(|p| !p.extras.contains(&"OP"))
        .collect();
    let Some(language) = languages(code)
        .iter()
        .find(|l| printing.iter().any(|p| p.language == **l))
    else {
        return Vec::new();
    };

    let mut found: Vec<PictureFile> = printing
        .into_iter()
        .filter(|p| p.language == *language)
        .collect();
    found.sort_by_key(|p| p.edition != Some("1E"));
    found.into_iter().map(|p| p.file).collect()
}

/// The picture to give a copy by itself: the printing's ([`candidates`])
/// when they are all one artwork (one rarity, one set of extras), the 1st
/// edition's if there is one. `None` when there is none, or a choice to make
/// (normal and Extended Art, or the rarity not noted and several printed).
pub fn automatic<'a>(files: &'a [String], code: &str, rarity: Option<Rarity>) -> Option<&'a str> {
    let found = candidates(files, code, rarity);
    let first = parse(found.first()?)?;
    let one_artwork = found
        .iter()
        .filter_map(|f| parse(f))
        .all(|p| p.artwork() == first.artwork());
    one_artwork.then_some(found[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn reads_a_printing() {
        let p = parse("File:WitnessoftheAncient-CORI-FR-UR-1E-EA.png").unwrap();
        assert_eq!(
            (p.set, p.language, p.rarity, p.edition, p.extras),
            ("CORI", "FR", "UR", Some("1E"), vec!["EA"])
        );
        let p = parse("FoolishBurial-QCLP-JP-QCScR.jpg").unwrap();
        assert_eq!((p.rarity, p.edition), ("QCScR", None));
        assert_eq!(
            parse("DrollLockBird-RA02-DE-CR-1E-AA.png").unwrap().extras,
            ["AA"]
        );
    }

    #[test]
    fn leaves_out_what_isnt_a_printing() {
        for file in [
            "FoolishBurial-EN-Anime-DM.png",
            "FoolishBurial-JP-Manga-GX.png",
            "FoolishBurial-LD-EN-VG.png",
            "FoolishBurial-OW.png",
            "FoolishBurial-SD27-JP-C.svg",
            "FoolishBurial.png",
        ] {
            assert_eq!(parse(file), None, "{file}");
        }
    }

    #[test]
    fn yugipedia_rarity_codes() {
        assert_eq!(rarity_code(Rarity::UltraRare), "UR");
        assert_eq!(rarity_code(Rarity::PremiumGoldRare), "PGR");
        assert_eq!(rarity_code(Rarity::PlatinumSecretRare), "PlScR");
        assert_eq!(rarity_code(Rarity::QuarterCenturySecretRare), "QCScR");
    }

    const WITNESS: [&str; 7] = [
        "File:WitnessoftheAncient-CORI-EN-UR-1E.png",
        "File:WitnessoftheAncient-CORI-EN-UR-1E-EA.png",
        "File:WitnessoftheAncient-CORI-EN-StR-1E.png",
        "File:WitnessoftheAncient-CORI-EN-1E-OP.png",
        "File:WitnessoftheAncient-CORI-FR-UR-1E.png",
        "File:WitnessoftheAncient-CORI-FR-UR-1E-EA.png",
        "File:WitnessoftheAncient-CORI-FR-StR-1E.png",
    ];

    #[test]
    fn the_printings_of_a_copy() {
        let gallery = files(&WITNESS);
        assert_eq!(
            candidates(&gallery, "CORI-FR081", Some(Rarity::UltraRare)),
            [
                "File:WitnessoftheAncient-CORI-FR-UR-1E.png",
                "File:WitnessoftheAncient-CORI-FR-UR-1E-EA.png"
            ]
        );
        assert_eq!(
            candidates(&gallery, "CORI-EN081", Some(Rarity::StarlightRare)),
            ["File:WitnessoftheAncient-CORI-EN-StR-1E.png"]
        );
        // rarity not noted: every rarity of the printing
        assert_eq!(candidates(&gallery, "cori-en081", None).len(), 3);
        assert!(candidates(&gallery, "CORI-DE081", None).is_empty());
        assert!(candidates(&gallery, "MAMO-FR081", None).is_empty());
        assert!(candidates(&gallery, "nonsense", None).is_empty());
    }

    #[test]
    fn the_best_language_present_and_the_1st_edition_first() {
        let gallery = files(&[
            "BlueEyesWhiteDragon-DDK-FC-UR-1E.png",
            "BlueEyesWhiteDragon-DDK-FR-UR-UE.png",
            "BlueEyesWhiteDragon-DDK-FR-UR-1E.png",
            "BlueEyesWhiteDragon-SDK-NA-UR-UE.png",
        ]);
        assert_eq!(
            candidates(&gallery, "DDK-F001", Some(Rarity::UltraRare)),
            [
                "BlueEyesWhiteDragon-DDK-FR-UR-1E.png",
                "BlueEyesWhiteDragon-DDK-FR-UR-UE.png"
            ]
        );
        // an old English code without language: the North American print
        assert_eq!(
            candidates(&gallery, "SDK-001", None),
            ["BlueEyesWhiteDragon-SDK-NA-UR-UE.png"]
        );
    }

    #[test]
    fn picks_by_itself_only_one_artwork() {
        let gallery = files(&WITNESS);
        // normal or Extended Art: a person chooses
        assert_eq!(
            automatic(&gallery, "CORI-FR081", Some(Rarity::UltraRare)),
            None
        );
        assert_eq!(
            automatic(&gallery, "CORI-FR081", Some(Rarity::StarlightRare)),
            Some("File:WitnessoftheAncient-CORI-FR-StR-1E.png")
        );
        // rarity not noted and two printed
        assert_eq!(automatic(&gallery, "CORI-FR081", None), None);

        let gallery = files(&[
            "AshBlossomJoyousSpring-SHVA-FR-SR-UE.png",
            "AshBlossomJoyousSpring-SHVA-FR-SR-1E.png",
        ]);
        // two editions of one artwork: the 1st edition's
        assert_eq!(
            automatic(&gallery, "SHVA-FR047", None),
            Some("AshBlossomJoyousSpring-SHVA-FR-SR-1E.png")
        );
        assert_eq!(
            automatic(&gallery, "SHVA-FR047", Some(Rarity::UltraRare)),
            None
        );
    }
}
