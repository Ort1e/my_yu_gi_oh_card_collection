//! `export-public`: the public copy of the collections (`common::export`),
//! written into a folder to put on the web hosting: the Yu-Gi-Oh! cards,
//! the Pokémon cards and wishlist, the books. Nothing else of the app.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::Request;
use common::export::{Content, Section};
use common::web::auth::{Auth, AuthConfig};
use common::AppState;
use tower::ServiceExt;

/// Write the copy into `out`, the stylesheets from `static_dir` (`static`).
pub(crate) async fn export(state: &AppState, out: &Path, static_dir: &Path) -> anyhow::Result<()> {
    let pool = &state.pool;
    let mut sections = vec![yugioh::public::section(pool).await?];
    sections.extend(pokemon::public::sections(pool).await?);
    sections.push(books::public::section(pool).await?);
    let fetched = fetch_missing_pictures(state, &sections).await?;

    let media = state.media_dir.clone();
    let folder = PathBuf::from(out);
    let static_dir = PathBuf::from(static_dir);
    let written = tokio::task::spawn_blocking(move || {
        common::export::write_site(sections, &media, &static_dir, &folder)
    })
    .await??;

    println!(
        "{}: {} pages, {} pictures ({fetched} downloaded now, {} without one), {:.1} MB",
        out.display(),
        written.pages,
        written.pictures,
        written.missing_pictures,
        written.bytes as f64 / 1_000_000.0
    );
    Ok(())
}

/// The pictures the app hasn't downloaded yet (a page never showed them):
/// asked to the app's own picture routes, which download them into the
/// media folder like a page would (throttled, cached). The login is
/// bypassed: the app is asked here, not over the network. How many.
async fn fetch_missing_pictures(state: &AppState, sections: &[Section]) -> anyhow::Result<usize> {
    let mut local = state.clone();
    local.auth = Auth::load(&state.pool, AuthConfig::default()).await?;
    let app = crate::app(local);

    let mut fetched = 0;
    for section in sections {
        let Content::Cards(cards) = &section.content else {
            continue;
        };
        for card in cards {
            let (Some(url), Some(picture)) = (&card.tile.image, &card.picture) else {
                continue;
            };
            if state.media_dir.join(picture).exists() {
                continue;
            }
            let response = app
                .clone()
                .oneshot(Request::get(url).body(Body::empty())?)
                .await?;
            if response.status().is_success() {
                fetched += 1;
            } else {
                tracing::warn!(url, status = %response.status(), "picture not found");
            }
        }
    }
    Ok(fetched)
}
