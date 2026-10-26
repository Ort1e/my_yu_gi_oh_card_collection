//! The public copy of the collections (`export-public`): static pages to put
//! on any web hosting, with nothing that runs there. Each collection gives
//! its [`Section`] (only what may be public: the cards and books owned, their
//! market value; never lots, sellers, prices paid, budget, decks,
//! tournaments or calendars); [`write_site`] writes the pages, small copies
//! of the card pictures and the stylesheets into a folder.
//!
//! ```text
//! index.html            the collections, with their totals
//! <section>.html        a grid of cards (`partials/card_grid.html`) or a table
//! img/<section>/…jpg    the pictures, 240 px wide
//! static/css/…          the app's stylesheets
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use askama::Template;

use crate::photos;
use crate::web::tiles::CardTile;

/// The file that marks a folder as an export: only such a folder (or a
/// missing or empty one) is emptied and written again.
const MARKER: &str = ".public-export";

/// The stylesheets the pages use (copied from `static/css/`).
const STYLESHEETS: [&str; 6] = [
    "style.css",
    "header.css",
    "card.css",
    "tiles.css",
    "pokemon.css",
    "books.css",
];

/// The width of the pictures' copies.
const PICTURE_WIDTH: u32 = 240;

/// A card (or several identical copies) of a public grid.
pub struct ExportCard {
    /// Its tile: its picture is the app's address of it (`/card-image/12`),
    /// to download it if missing; its link and picture are then set by
    /// [`write_site`].
    pub tile: CardTile,
    /// Its picture, relative to the media folder (`card_images/x.jpg`), if
    /// the app has downloaded it.
    pub picture: Option<String>,
}

/// What a section shows.
pub enum Content {
    /// A grid of pictures.
    Cards(Vec<ExportCard>),
    /// A table: headings, then rows of cells.
    Table {
        headings: Vec<&'static str>,
        rows: Vec<Vec<String>>,
    },
}

/// One collection's page.
pub struct Section {
    /// Its file name (`yugioh` → `yugioh.html`) and pictures' folder.
    pub slug: &'static str,
    pub title: &'static str,
    /// The `body` class of its colours (`pokemon`, `books`, `""`).
    pub body_class: &'static str,
    /// Its totals: `512 cards · 1038.80 €`.
    pub summary: String,
    pub content: Content,
}

/// The navigation bar: every section's page.
struct NavLink {
    href: String,
    title: &'static str,
}

#[derive(Template)]
#[template(path = "export/index.html")]
struct IndexPage<'a> {
    nav: &'a [NavLink],
    sections: &'a [Section],
    date: String,
}

#[derive(Template)]
#[template(path = "export/section.html")]
struct SectionPage<'a> {
    nav: &'a [NavLink],
    section: &'a Section,
    tiles: &'a [CardTile],
    date: String,
}

impl SectionPage<'_> {
    fn headings(&self) -> &[&'static str] {
        match &self.section.content {
            Content::Table { headings, .. } => headings,
            Content::Cards(_) => &[],
        }
    }

    fn rows(&self) -> &[Vec<String>] {
        match &self.section.content {
            Content::Table { rows, .. } => rows,
            Content::Cards(_) => &[],
        }
    }
}

/// What [`write_site`] wrote.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Written {
    pub pages: usize,
    pub pictures: usize,
    /// Pictures the app hasn't downloaded yet, or unreadable: their tile
    /// shows the name instead.
    pub missing_pictures: usize,
    pub bytes: u64,
}

/// Write the site into `out` (emptied first; refused if it holds something
/// else than an earlier export), the pictures from `media`, the stylesheets
/// from `static_dir`.
pub fn write_site(
    mut sections: Vec<Section>,
    media: &Path,
    static_dir: &Path,
    out: &Path,
) -> anyhow::Result<Written> {
    prepare(out)?;
    let mut written = Written::default();

    std::fs::create_dir_all(out.join("static/css"))?;
    for sheet in STYLESHEETS {
        let to = out.join("static/css").join(sheet);
        std::fs::copy(static_dir.join("css").join(sheet), &to)?;
    }

    // the pictures, once each (identical copies share theirs)
    for section in &mut sections {
        let Content::Cards(cards) = &mut section.content else {
            continue;
        };
        let folder = out.join("img").join(section.slug);
        std::fs::create_dir_all(&folder)?;
        let mut done: HashMap<String, Option<String>> = HashMap::new();
        for card in cards.iter_mut() {
            card.tile.href = String::new();
            card.tile.image = match &card.picture {
                None => None,
                Some(picture) => done
                    .entry(picture.clone())
                    .or_insert_with(|| {
                        let name = format!("{}.jpg", picture_name(picture));
                        let copy = std::fs::read(media.join(picture))
                            .ok()
                            .and_then(|bytes| photos::thumbnail(&bytes, PICTURE_WIDTH).ok())?;
                        std::fs::write(folder.join(&name), copy).ok()?;
                        written.pictures += 1;
                        Some(format!("img/{}/{name}", section.slug))
                    })
                    .clone(),
            };
            if card.tile.image.is_none() {
                written.missing_pictures += 1;
            }
        }
    }

    let nav: Vec<NavLink> = sections
        .iter()
        .map(|s| NavLink {
            href: format!("{}.html", s.slug),
            title: s.title,
        })
        .collect();
    let date = crate::db::today().to_string();

    let index = IndexPage {
        nav: &nav,
        sections: &sections,
        date: date.clone(),
    };
    std::fs::write(out.join("index.html"), index.render()?)?;
    written.pages += 1;

    for section in &sections {
        let tiles: Vec<CardTile> = match &section.content {
            Content::Cards(cards) => cards.iter().map(|c| c.tile.clone()).collect(),
            Content::Table { .. } => Vec::new(),
        };
        let page = SectionPage {
            nav: &nav,
            section,
            tiles: &tiles,
            date: date.clone(),
        };
        std::fs::write(out.join(format!("{}.html", section.slug)), page.render()?)?;
        written.pages += 1;
    }

    written.bytes = folder_size(out)?;
    Ok(written)
}

/// A picture's file name from its path in the media folder: its name, kept
/// to letters, digits, `-` and `_`.
fn picture_name(picture: &str) -> String {
    let stem = Path::new(picture)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    stem.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Empty `out` for a new export: a missing or empty folder, or an earlier
/// export (it has the [`MARKER`]); anything else is refused.
fn prepare(out: &Path) -> anyhow::Result<()> {
    if out.exists() {
        let empty = std::fs::read_dir(out)?.next().is_none();
        if !empty && !out.join(MARKER).exists() {
            anyhow::bail!(
                "{} is not an earlier export (no {MARKER} file): choose an empty folder",
                out.display()
            );
        }
        std::fs::remove_dir_all(out)?;
    }
    std::fs::create_dir_all(out)?;
    std::fs::write(
        out.join(MARKER),
        "Written by `backend export-public`: emptied at each export.\n",
    )?;
    Ok(())
}

fn folder_size(dir: &Path) -> std::io::Result<u64> {
    let mut size = 0;
    let mut folders = vec![PathBuf::from(dir)];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                folders.push(entry.path());
            } else {
                size += metadata.len();
            }
        }
    }
    Ok(size)
}
