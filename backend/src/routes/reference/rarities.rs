//! The rarity recognition guide. Its content lives in `static/js/rarities.js`
//! (shared with the quick-add panel of the lot forms).

use askama::Template;

#[derive(Template)]
#[template(path = "reference/rarities.html")]
pub struct RaritiesPage;

pub async fn page() -> RaritiesPage {
    RaritiesPage
}
