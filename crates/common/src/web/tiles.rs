//! The card collections' display (Yu-Gi-Oh! and Pokémon): a grid of
//! pictures (`partials/card_grid.html`, `static/css/tiles.css`), or a table
//! of the same cards ([`View`]).

use crate::web::pagination;

/// One card in a picture grid. Every tile of a page has the same size and
/// the same lines, in the same places (empty ones keep their room): the
/// picture, the name (two lines), the other name, the print, the details,
/// then the condition, the tags and the value at the bottom.
#[derive(Debug, Default, Clone)]
pub struct CardTile {
    /// Its page; empty: no link (the public export has no card pages).
    pub href: String,
    /// The card's proportions, for its picture.
    pub shape: CardShape,
    /// Its picture; `None`: its name and print in its place.
    pub image: Option<String>,
    pub name: String,
    /// Its name in another language, empty when none: the English name of
    /// a French card, the French name of a Japanese one.
    pub other_name: String,
    /// Which print: `JUSH-FR040`, `30e Anniversaire · 016`.
    pub print: String,
    /// What this print is: `Ultra Rare`, `Illustration rare · Holo`.
    pub details: String,
    /// Its condition, `(code, label)`: `("NM", "Near Mint")`, shown as a
    /// coloured badge (`.status-badge`, card.css); `None` when not noted.
    pub condition: Option<(&'static str, &'static str)>,
    /// Badges: Full art, PSA 10, Proxy, Sold.
    pub tags: Vec<String>,
    /// Its value (`12.50`, shown with €), empty when unknown.
    pub value: String,
    /// Faded: sold.
    pub faded: bool,
}

/// The proportions of a collection's cards (their pictures are shown
/// whole, never cropped).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CardShape {
    /// 59 × 86 mm.
    #[default]
    YuGiOh,
    /// 63 × 88 mm.
    Pokemon,
}

impl CardShape {
    /// The class of its tiles (`static/css/tiles.css`).
    pub fn class(self) -> &'static str {
        match self {
            CardShape::YuGiOh => "yugioh",
            CardShape::Pokemon => "pokemon",
        }
    }
}

/// `parts` that aren't empty, joined by ` · `.
pub fn dotted<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// How a list shows its cards: pictures (the default) or a table
/// (`?view=table`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Grid,
    Table,
}

impl View {
    /// From the `view` parameter: `table`, else the grid.
    pub fn from_param(view: Option<&str>) -> View {
        match view.map(str::trim) {
            Some("table") => View::Table,
            _ => View::Grid,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            View::Grid => "grid",
            View::Table => "table",
        }
    }

    pub fn is_table(self) -> bool {
        self == View::Table
    }

    /// Cards per page: a few rows of pictures, or a long table.
    pub fn page_size(self) -> i64 {
        match self {
            View::Grid => 24,
            View::Table => 100,
        }
    }

    /// The link to the other view's name: `Pictures` or `Table`.
    pub fn other_label(self) -> &'static str {
        match self {
            View::Grid => "Table",
            View::Table => "Pictures",
        }
    }

    /// The filters (`querystring`, URL-encoded) with this view, for the page
    /// links: the grid needs nothing.
    pub fn keep(self, querystring: &str) -> String {
        match (self, querystring.is_empty()) {
            (View::Grid, _) => querystring.to_string(),
            (View::Table, true) => "view=table".to_string(),
            (View::Table, false) => format!("{querystring}&view=table"),
        }
    }

    /// The same list in the other view: `path`, the filters (`querystring`,
    /// URL-encoded, without `view`) and the view.
    pub fn other_href(self, path: &str, querystring: &str) -> String {
        let other = match self {
            View::Grid => View::Table,
            View::Table => View::Grid,
        };
        let view = pagination::querystring([("view", Some(other.code()))]);
        if querystring.is_empty() {
            format!("{path}?{view}")
        } else {
            format!("{path}?{querystring}&{view}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_skips_the_empty_parts() {
        assert_eq!(
            dotted(["Ultra Rare", "", " Near Mint "]),
            "Ultra Rare · Near Mint"
        );
        assert_eq!(dotted(["", ""]), "");
    }

    #[test]
    fn views() {
        assert_eq!(View::from_param(None), View::Grid);
        assert_eq!(View::from_param(Some("table")), View::Table);
        assert_eq!(View::from_param(Some("grid")), View::Grid);
        assert_eq!(
            View::Grid.other_href("/cards", "name=a"),
            "/cards?name=a&view=table"
        );
        assert_eq!(View::Table.other_href("/pokemon", ""), "/pokemon?view=grid");
        assert_eq!(View::Grid.keep("q=a"), "q=a");
        assert_eq!(View::Table.keep("q=a"), "q=a&view=table");
        assert_eq!(View::Table.keep(""), "view=table");
    }
}
