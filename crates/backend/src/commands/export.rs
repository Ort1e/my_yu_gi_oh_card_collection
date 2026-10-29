//! `export-public`: the public copy of the collections (`common::export`),
//! written into a folder to put on the web hosting: the app's own pages of
//! the Yu-Gi-Oh! cards, their statistics and decks, the Pokémon cards and
//! wishlist, the books, the rarity guides. Nothing else of the app: only
//! the addresses of [`public`] are asked for, and the templates leave out
//! what is private (`common::export::public_copy`). The decks: only those
//! played in a tournament or ticked "public" (`yugioh::routes::decks`).

use std::path::Path;

use common::export::Site;
use common::web::auth::{Auth, AuthConfig};
use common::AppState;

/// The pages the copy starts from; the first one is its home page.
const START: [&str; 10] = [
    "/cards",
    "/collection/stats",
    "/decks",
    "/rarities",
    "/pokemon",
    "/pokemon/wishlist",
    "/pokemon/rarities",
    "/books",
    "/books/authors",
    "/books/series",
];

/// The addresses the copy may hold: the start pages, a card's, a deck's, a
/// deck version's, a copy's, a book's, an author's, a series' page, and the
/// card pictures.
fn public(path: &str) -> bool {
    const NUMBERED: [&str; 7] = [
        "/cards/",
        "/decks/",
        "/deck-versions/",
        "/pokemon/",
        "/books/",
        "/books/authors/",
        "/books/series/",
    ];
    START.contains(&path)
        || NUMBERED.iter().any(|prefix| {
            path.strip_prefix(prefix)
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        })
        || path.starts_with("/card-image/")
        || path.starts_with("/pokemon/card-image/")
}

/// Write the copy into `out`, the stylesheets and scripts from
/// `static_dir` (`static`).
pub(crate) async fn export(state: &AppState, out: &Path, static_dir: &Path) -> anyhow::Result<()> {
    // the app asked here, not over the network: no login
    let mut local = state.clone();
    local.auth = Auth::load(&state.pool, AuthConfig::default()).await?;
    let site = Site {
        app: crate::app(local),
        start: &START,
        public,
        static_dir,
    };
    let written = common::export::write_site(site, out).await?;

    println!(
        "{}: {} pages, {} pictures, {} missing, {:.1} MB",
        out.display(),
        written.pages,
        written.pictures,
        written.missing,
        written.bytes as f64 / 1_000_000.0
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_public_addresses() {
        for path in [
            "/cards",
            "/cards/12",
            "/books/series/3",
            "/pokemon/7",
            "/decks/1",
            "/deck-versions/4",
            "/card-image/1",
        ] {
            assert!(public(path), "{path}");
        }
        for path in [
            "/",
            "/lots",
            "/lots/1",
            "/sellers/1",
            "/cards/12/edit",
            "/pokemon/openings",
            "/pokemon/add",
            "/budget",
            "/decks/1/edit",
            "/decks/built",
            "/deck-builder/4",
            "/tournaments/1",
            "/books/advent-calendar",
            "/media/shipment_files/x.pdf",
        ] {
            assert!(!public(path), "{path}");
        }
    }
}
