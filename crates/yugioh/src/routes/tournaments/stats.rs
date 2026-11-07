//! `GET /tournaments/stats` (`?since=`, `?deck=`, like the list): win rates
//! overall, by deck version, by your deck's labels, against which matchups,
//! die and first or second. Split from the list (`mod.rs`) so each page does
//! one thing; the numbers come from `yugioh_domain::tournament::stats`.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Query, State};
use yugioh_domain::tournament::{stats, Record, Stats};

use super::data::{self, Which};
use super::{deck_options, rate_text, record_text, DeckOption, Filter};
use crate::web::error::AppResult;
use crate::AppState;

pub struct StatLine {
    name: String,
    href: Option<String>,
    rounds: u32,
    record: String,
    rate: String,
}

impl StatLine {
    fn new(name: impl Into<String>, href: Option<String>, r: &Record) -> StatLine {
        StatLine {
            name: name.into(),
            href,
            rounds: r.wins + r.losses + r.draws,
            record: record_text(r),
            rate: rate_text(r),
        }
    }
}

/// One table of the stats.
pub struct StatSection {
    title: &'static str,
    /// The heading of the first column.
    what: &'static str,
    /// "Rounds" or "Games".
    counted: &'static str,
    lines: Vec<StatLine>,
}

#[derive(Template)]
#[template(path = "tournaments/stats.html")]
pub struct StatsPage {
    sections: Vec<StatSection>,
    byes: u32,
    since: String,
    decks: Vec<DeckOption>,
    filtered: bool,
    /// The list with the same filter.
    list_href: String,
}

impl StatsPage {
    /// Where the filter form goes (`filter.html`).
    fn filter_action(&self) -> &'static str {
        "/tournaments/stats"
    }
}

fn sections(s: &Stats, version_names: &HashMap<i64, String>) -> Vec<StatSection> {
    let labels = |lines: &[(String, Record)]| {
        lines
            .iter()
            .map(|(name, r)| StatLine::new(name.clone(), None, r))
            .collect()
    };
    vec![
        StatSection {
            title: "Overall",
            what: "",
            counted: "Played",
            lines: vec![
                StatLine::new("Rounds", None, &s.rounds),
                StatLine::new("Games", None, &s.games),
            ],
        },
        StatSection {
            title: "By deck version",
            what: "Version",
            counted: "Rounds",
            lines: s
                .per_version
                .iter()
                .map(|(id, r)| {
                    let name = version_names
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| format!("Version #{id}"));
                    StatLine::new(name, Some(format!("/deck-versions/{id}")), r)
                })
                .collect(),
        },
        StatSection {
            title: "By your deck's labels",
            what: "Label",
            counted: "Rounds",
            lines: labels(&s.per_own_label),
        },
        StatSection {
            title: "Matchups",
            what: "Against",
            counted: "Rounds",
            lines: labels(&s.matchups),
        },
        StatSection {
            title: "Die",
            what: "",
            counted: "Rounds",
            lines: vec![
                StatLine::new("Die won", None, &s.die_won),
                StatLine::new("Die lost", None, &s.die_lost),
            ],
        },
        StatSection {
            title: "First or second",
            what: "",
            counted: "Games",
            lines: vec![
                StatLine::new("Going first", None, &s.going_first),
                StatLine::new("Going second", None, &s.going_second),
            ],
        },
    ]
}

/// `GET /tournaments/stats`.
pub async fn page(
    State(state): State<AppState>,
    Query(filter): Query<Filter>,
) -> AppResult<StatsPage> {
    let which = Which {
        since: filter.since(),
        deck_id: filter.deck_id(),
        ..Which::default()
    };
    let loaded = data::load(&state.pool, &which).await?;
    let version_names: HashMap<i64, String> = loaded
        .tournaments
        .iter()
        .filter_map(|t| Some((t.version_id?, t.version_full_name()?)))
        .collect();
    let all = stats(&loaded.all_records());

    Ok(StatsPage {
        sections: sections(&all, &version_names),
        byes: all.byes,
        since: which.since.clone().unwrap_or_default(),
        decks: deck_options(&state.pool, which.deck_id).await?,
        filtered: which.since.is_some() || which.deck_id.is_some(),
        list_href: format!("/tournaments{}", filter.query()),
    })
}
