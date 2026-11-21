//! The live tournament notes in a Discord channel (`DISCORD_WEBHOOK_URL`),
//! for a tournament opted in on the live start page
//! (`tournament.post_to_discord`).
//!
//! The messages are written here from the tournament **as saved**, after
//! every save of it (the live start, a round `PUT` or the end `PATCH` of the
//! live page, the edit form): first the deck list played (its snapshot), so
//! the friends in the channel see it during the day while the public copy only
//! shows it from the next nightly export; then one message per round, the
//! first one under the tournament's head, and the day's summary once its end
//! is noted (placement, players or conclusion). `store::discord::publish` then posts what is new, edits what
//! changed (the opponent's deck or a comment written after the round) and
//! deletes what is gone. In a task, one at a time, reading the tournament
//! inside its turn (the last one sees the last save); a failure is only a
//! `warn!` (the notes are saved anyway). Nothing under `cfg(test)`.
//!
//! The messages are in **French** (the channel's readers; the owner's
//! choice, 2026-11-21): the words are here, not the domain's English labels.

use common::web::filters::french_date;
use tokio::sync::Mutex;
use yugioh_domain::deck_history::{DeckZone, SnapshotList};
use yugioh_domain::tournament::{stats, Die, Game, GameResult, RoundRecord, RoundResult};

use super::data::{self, RoundView, Which};
use super::record_text;
use crate::store::discord::{publish, DECK, DECK_SIDE, SUMMARY};
use crate::store::snapshots;
use crate::AppState;

/// One tournament's messages at a time: two quick saves must not both see
/// "not posted yet" and post twice, and the last save must be published last.
static PUBLISHING: Mutex<()> = Mutex::const_new(());

/// Brings tournament `id`'s Discord messages up to date with what is saved,
/// in the background, when a webhook is set and the tournament is opted in.
pub fn spawn_sync(state: &AppState, id: i64) {
    // never Discord from a test
    if cfg!(test) {
        return;
    }
    let Some(webhook) = state.discord.clone() else {
        return;
    };
    let pool = state.pool.clone();
    let http = state.http.clone();
    let site = state.public_site.clone();
    tokio::spawn(async move {
        let _turn = PUBLISHING.lock().await;
        let messages = match messages_of(&pool, site.as_deref(), id).await {
            Ok(Some(messages)) => messages,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!(tournament_id = id, "Discord: reading the tournament: {e}");
                return;
            }
        };
        if let Err(e) = publish(&pool, &http, &webhook, id, &messages).await {
            tracing::warn!(tournament_id = id, "Discord post failed: {e}");
        }
    });
}

/// The messages of tournament `id`, `None` when it isn't opted in (or gone).
/// `site`: the public copy's address, for the summary's link to the list.
async fn messages_of(
    pool: &crate::store::db::Pool,
    site: Option<&str>,
    id: i64,
) -> Result<Option<Vec<(i64, String)>>, String> {
    let opted_in: Option<bool> =
        sqlx::query_scalar("SELECT post_to_discord FROM tournament WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if opted_in != Some(true) {
        return Ok(None);
    }
    let which = Which {
        id: Some(id),
        ..Which::default()
    };
    let loaded = data::load(pool, &which)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let Some(t) = loaded.tournaments.first() else {
        return Ok(None);
    };
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;
    let snapshot = snapshots::of_tournament(&mut conn, id)
        .await
        .map_err(|e| e.to_string())?;
    let deck = match &snapshot {
        Some(snapshot) => snapshots::cards(&mut conn, snapshot.id)
            .await
            .map_err(|e| e.to_string())?,
        None => SnapshotList::default(),
    };
    // a tournament's list is always in the public copy (`PUBLIC_SNAPSHOT`)
    let deck_url = site
        .zip(snapshot.as_ref())
        .map(|(site, s)| common::export::page_url(site, &format!("/deck-snapshots/{}", s.id)));
    let head = Head {
        name: t.name.clone(),
        date: t.date.clone(),
        location: t.location.clone(),
        is_team: t.is_team,
        deck: t.version_full_name(),
        placement: t.placement,
        players: t.players,
        conclusion: t.conclusion.clone(),
        deck_url,
    };
    Ok(Some(messages(&head, &deck, loaded.rounds_of(id))))
}

/// What the messages say of the tournament itself.
struct Head {
    name: String,
    date: String,
    location: Option<String>,
    is_team: bool,
    /// "Deck - list" of the list played.
    deck: Option<String>,
    placement: Option<i64>,
    players: Option<i64>,
    conclusion: Option<String>,
    /// The list played on the public site, when its address is known.
    deck_url: Option<String>,
}

/// Every message (slot, content), in the order they are posted: the deck
/// list, the rounds, then the summary when the end of the day is noted.
fn messages(head: &Head, deck: &SnapshotList, rounds: &[RoundView]) -> Vec<(i64, String)> {
    let mut messages = deck_messages(head, deck);
    messages.extend(rounds.iter().map(|round| {
        let text = round_text(round);
        if round.number == 1 {
            (round.number, format!("{}\n\n{text}", head_text(head)))
        } else {
            (round.number, text)
        }
    }));
    let ended = head.placement.is_some() || head.players.is_some() || head.conclusion.is_some();
    if ended {
        messages.push((SUMMARY, summary_text(head, rounds)));
    }
    messages
}

/// The deck list played, `None` for an empty zone: `**Main (40)**` then a
/// line per card, `3× Name`.
fn zone_text(deck: &SnapshotList, zone: DeckZone) -> Option<String> {
    let entries: Vec<_> = deck.entries.iter().filter(|e| e.zone == zone).collect();
    if entries.is_empty() {
        return None;
    }
    let total: u32 = entries.iter().map(|e| e.count).sum();
    let title = match zone {
        DeckZone::Main => "Main",
        DeckZone::Extra => "Extra",
        DeckZone::Side => "Side",
    };
    let mut lines = vec![format!("**{title} ({total})**")];
    lines.extend(entries.iter().map(|e| format!("{}× {}", e.count, e.name)));
    Some(lines.join("\n"))
}

/// The deck list's message, or two when it's too long for one (the Side
/// Deck in the second); none without a list.
fn deck_messages(head: &Head, deck: &SnapshotList) -> Vec<(i64, String)> {
    let [main, extra, side] = DeckZone::ALL.map(|zone| zone_text(deck, zone));
    if main.is_none() && extra.is_none() && side.is_none() {
        return Vec::new();
    }
    let title = match &head.deck {
        Some(name) => format!("### 🃏 Liste jouée : **{name}**"),
        None => "### 🃏 Liste jouée".to_string(),
    };
    let first: Vec<String> = [Some(title.clone()), main, extra]
        .into_iter()
        .flatten()
        .collect();
    let first = first.join("\n\n");
    match side {
        None => vec![(DECK, first)],
        Some(side) => {
            let whole = format!("{first}\n\n{side}");
            if whole.chars().count() <= common::discord::MAX_CHARS {
                vec![(DECK, whole)]
            } else {
                vec![
                    (DECK, first),
                    (DECK_SIDE, format!("{title} (side)\n\n{side}")),
                ]
            }
        }
    }
}

/// `## 🏆 Locals`, then the date, place, deck played.
fn head_text(head: &Head) -> String {
    let mut details = vec![format!("📅 {}", french_date(&head.date))];
    if let Some(location) = &head.location {
        details.push(format!("📍 {location}"));
    }
    if head.is_team {
        details.push("👥 Par équipes".into());
    }
    let mut text = format!("## 🏆 {}\n{}", head.name, details.join(" · "));
    if let Some(deck) = &head.deck {
        text.push_str(&format!("\n🃏 Deck joué : **{deck}**"));
    }
    text
}

fn result_emoji(result: RoundResult) -> &'static str {
    match result {
        RoundResult::Win | RoundResult::ForfeitWin => "✅",
        RoundResult::Loss => "❌",
        RoundResult::DoubleLoss => "⏱️",
        RoundResult::Bye => "💤",
    }
}

fn result_word(result: RoundResult) -> &'static str {
    match result {
        RoundResult::Win => "Victoire",
        RoundResult::Loss => "Défaite",
        RoundResult::DoubleLoss => "Double défaite",
        RoundResult::Bye => "Exempt",
        RoundResult::ForfeitWin => "Victoire par forfait",
    }
}

fn game_word(result: GameResult) -> &'static str {
    match result {
        GameResult::Won => "Gagnée",
        GameResult::Lost => "Perdue",
        GameResult::Draw => "Nulle",
        GameResult::Unfinished => "Inachevée",
    }
}

/// "1er", "3e".
fn ordinal(n: i64) -> String {
    if n == 1 {
        "1er".into()
    } else {
        format!("{n}e")
    }
}

/// "3e sur 32", "3e", "32 joueurs", `None` when not noted.
fn placement_text(head: &Head) -> Option<String> {
    match (head.placement, head.players) {
        (Some(p), Some(n)) => Some(format!("{} sur {n}", ordinal(p))),
        (Some(p), None) => Some(ordinal(p)),
        (None, Some(n)) => Some(format!("{n} joueurs")),
        (None, None) => None,
    }
}

fn game_emoji(result: GameResult) -> &'static str {
    match result {
        GameResult::Won => "✅",
        GameResult::Lost => "❌",
        GameResult::Draw => "🤝",
        GameResult::Unfinished => "⏱️",
    }
}

/// "2-1", "2-1-1" with draws, `None` without games.
fn score(round: &RoundView) -> Option<String> {
    if round.games.is_empty() {
        return None;
    }
    let count = |r: GameResult| round.games.iter().filter(|g| g.result == r).count();
    let (won, lost, draw) = (
        count(GameResult::Won),
        count(GameResult::Lost),
        count(GameResult::Draw),
    );
    Some(if draw > 0 {
        format!("{won}-{lost}-{draw}")
    } else {
        format!("{won}-{lost}")
    })
}

/// The opponent's deck as written, else its labels.
fn opponent(round: &RoundView) -> Option<String> {
    round
        .opponent_deck
        .clone()
        .or_else(|| (!round.labels.is_empty()).then(|| round.labels.join(" / ")))
}

/// A text on one line (a list item ends at a line break).
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A round with everything noted: result and score, opponent, die, team
/// result, each game, the comment.
fn round_text(round: &RoundView) -> String {
    let mut title = format!(
        "### Ronde {} · {} {}",
        round.number,
        result_emoji(round.result),
        result_word(round.result)
    );
    if let Some(score) = score(round).filter(|_| round.result != RoundResult::Bye) {
        title.push_str(&format!(" {score}"));
    }
    let mut lines = vec![title];

    if round.result != RoundResult::Bye {
        lines.push(match opponent(round) {
            Some(deck) => format!("⚔️ contre **{deck}**"),
            None => "⚔️ contre ❔ *deck pas encore noté*".into(),
        });
    }

    let mut details: Vec<String> = Vec::new();
    match round.die {
        Some(Die::Won) => details.push("🎲 Dé gagné".into()),
        Some(Die::Lost) => details.push("🎲 Dé perdu".into()),
        None => {}
    }
    if let Some(team) = round.team_result {
        details.push(format!(
            "👥 Équipe : {} {}",
            result_emoji(team),
            result_word(team)
        ));
    }
    if !details.is_empty() {
        lines.push(details.join(" · "));
    }

    for game in &round.games {
        let mut text = format!(
            "- M{} {} {}",
            game.number,
            game_emoji(game.result),
            game_word(game.result)
        );
        match game.went_first {
            Some(true) => text.push_str(" · en premier"),
            Some(false) => text.push_str(" · en second"),
            None => {}
        }
        if let Some(comment) = &game.comment {
            text.push_str(&format!(" · {}", one_line(comment)));
        }
        lines.push(text);
    }

    if let Some(comment) = &round.comment {
        lines.push(format!("💬 {comment}"));
    }
    lines.join("\n")
}

/// 🥇🥈🥉 for the podium.
fn medal(placement: Option<i64>) -> &'static str {
    match placement {
        Some(1) => "🥇",
        Some(2) => "🥈",
        Some(3) => "🥉",
        _ => "🏅",
    }
}

/// "2-1", "2-1, 1 exempt" over the rounds, "" without any.
fn rounds_record(s: &yugioh_domain::tournament::Stats) -> String {
    if s.rounds.wins + s.rounds.losses + s.byes == 0 {
        return String::new();
    }
    let mut text = record_text(&s.rounds);
    match s.byes {
        0 => {}
        1 => text.push_str(", 1 exempt"),
        n => text.push_str(&format!(", {n} exempts")),
    }
    text
}

/// The day: placement, records, deck played, a line per round, conclusion.
fn summary_text(head: &Head, rounds: &[RoundView]) -> String {
    let records: Vec<RoundRecord> = rounds
        .iter()
        .map(|r| RoundRecord {
            version_id: None,
            own_labels: Vec::new(),
            opponent_labels: r.labels.clone(),
            die: r.die,
            result: r.result,
            games: r
                .games
                .iter()
                .map(|g| Game {
                    result: g.result,
                    went_first: g.went_first,
                })
                .collect(),
        })
        .collect();
    let s = stats(&records);

    let mut lines = vec![format!("## 🏁 {} : bilan", head.name)];
    let mut figures: Vec<String> = Vec::new();
    if let Some(placement) = placement_text(head) {
        figures.push(format!("{} **{placement}**", medal(head.placement)));
    }
    let record = rounds_record(&s);
    if !record.is_empty() {
        figures.push(format!("Rondes **{record}**"));
    }
    if s.games.wins + s.games.losses + s.games.draws > 0 {
        figures.push(format!("Manches {}", record_text(&s.games)));
    }
    if !figures.is_empty() {
        lines.push(figures.join(" · "));
    }
    if let Some(deck) = &head.deck {
        lines.push(format!("🃏 {deck}"));
    }

    if !rounds.is_empty() {
        lines.push(String::new());
    }
    for round in rounds {
        let mut text = format!("R{} {}", round.number, result_emoji(round.result));
        if round.result == RoundResult::Bye {
            text.push_str(" Exempt");
        } else {
            if let Some(score) = score(round) {
                text.push_str(&format!(" {score}"));
            }
            text.push_str(&format!(
                " contre {}",
                opponent(round).unwrap_or_else(|| "?".into())
            ));
        }
        lines.push(text);
    }

    if let Some(conclusion) = &head.conclusion {
        lines.push(String::new());
        lines.push(format!("📝 {conclusion}"));
    }
    if let Some(url) = &head.deck_url {
        lines.push(String::new());
        // `<…>`: a link without Discord's preview (the page isn't there yet)
        lines.push(format!("🔗 La liste sur le site : <{url}>"));
        lines.push("⏳ En ligne demain matin seulement (mise à jour du site la nuit)".into());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::tournaments::data::GameView;
    use yugioh_domain::deck_history::SnapshotEntry;

    fn head(placement: Option<i64>, players: Option<i64>, conclusion: Option<&str>) -> Head {
        Head {
            name: "Locals".into(),
            date: "2026-11-21".into(),
            location: Some("Paris".into()),
            is_team: false,
            deck: Some("Snake-Eye - v3".into()),
            placement,
            players,
            conclusion: conclusion.map(Into::into),
            deck_url: None,
        }
    }

    fn game(
        number: i64,
        result: GameResult,
        went_first: Option<bool>,
        comment: Option<&str>,
    ) -> GameView {
        GameView {
            number,
            result,
            went_first,
            comment: comment.map(Into::into),
        }
    }

    fn round(
        number: i64,
        result: RoundResult,
        deck: Option<&str>,
        games: Vec<GameView>,
    ) -> RoundView {
        RoundView {
            number,
            opponent_deck: deck.map(Into::into),
            die: None,
            result,
            team_result: None,
            comment: None,
            labels: Vec::new(),
            games,
        }
    }

    #[test]
    fn a_round_says_everything_noted() {
        let mut r = round(
            2,
            RoundResult::Win,
            Some("Mitsurugi"),
            vec![
                game(1, GameResult::Won, Some(true), Some("Ash on\nthe search")),
                game(2, GameResult::Lost, Some(false), None),
                game(3, GameResult::Won, None, None),
            ],
        );
        r.die = Some(Die::Won);
        r.comment = Some("Close one".into());
        assert_eq!(
            round_text(&r),
            "### Ronde 2 · ✅ Victoire 2-1\n\
             ⚔️ contre **Mitsurugi**\n\
             🎲 Dé gagné\n\
             - M1 ✅ Gagnée · en premier · Ash on the search\n\
             - M2 ❌ Perdue · en second\n\
             - M3 ✅ Gagnée\n\
             💬 Close one"
        );

        let mut r = round(1, RoundResult::Loss, None, Vec::new());
        assert_eq!(
            round_text(&r),
            "### Ronde 1 · ❌ Défaite\n⚔️ contre ❔ *deck pas encore noté*"
        );
        r.labels = vec!["Ryzeal".into(), "Fiendsmith".into()];
        assert!(round_text(&r).contains("⚔️ contre **Ryzeal / Fiendsmith**"));

        let bye = round(3, RoundResult::Bye, None, Vec::new());
        assert_eq!(round_text(&bye), "### Ronde 3 · 💤 Exempt");
    }

    #[test]
    fn the_first_round_carries_the_head_and_the_summary_waits_for_the_end() {
        let rounds = vec![
            round(
                1,
                RoundResult::Win,
                Some("Mitsurugi"),
                vec![game(1, GameResult::Won, None, None)],
            ),
            round(2, RoundResult::Bye, None, Vec::new()),
        ];
        let m = messages(&head(None, None, None), &SnapshotList::default(), &rounds);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].0, 1);
        assert!(
            m[0].1.starts_with(
                "## 🏆 Locals\n📅 21/11/2026 · 📍 Paris\n🃏 Deck joué : **Snake-Eye - v3**\n\n### Ronde 1"
            ),
            "{}",
            m[0].1
        );
        assert_eq!(m[1], (2, "### Ronde 2 · 💤 Exempt".to_string()));
        assert_eq!(
            placement_text(&head(Some(1), None, None)).as_deref(),
            Some("1er")
        );
        assert_eq!(
            placement_text(&head(None, Some(16), None)).as_deref(),
            Some("16 joueurs")
        );

        let m = messages(
            &head(Some(3), Some(32), Some("Bonne journée")),
            &SnapshotList::default(),
            &rounds,
        );
        assert_eq!(m.len(), 3);
        assert_eq!(
            m[2],
            (
                SUMMARY,
                "## 🏁 Locals : bilan\n\
                 🥉 **3e sur 32** · Rondes **1-0, 1 exempt** · Manches 1-0\n\
                 🃏 Snake-Eye - v3\n\
                 \n\
                 R1 ✅ 1-0 contre Mitsurugi\n\
                 R2 💤 Exempt\n\
                 \n\
                 📝 Bonne journée"
                    .to_string()
            )
        );

        let mut ended = head(Some(3), Some(32), None);
        ended.deck_url = Some("https://cards.example.org/deck-snapshots-35.html".into());
        assert!(summary_text(&ended, &rounds).ends_with(
            "\n\n🔗 La liste sur le site : <https://cards.example.org/deck-snapshots-35.html>\n\
             ⏳ En ligne demain matin seulement (mise à jour du site la nuit)"
        ));
    }

    fn entry(zone: DeckZone, name: &str, count: u32) -> SnapshotEntry {
        SnapshotEntry {
            zone,
            card_data_id: None,
            name: name.into(),
            count,
        }
    }

    #[test]
    fn the_deck_list_comes_first() {
        let deck = SnapshotList {
            entries: vec![
                entry(DeckZone::Main, "Snake-Eye Ash", 3),
                entry(DeckZone::Main, "Ash Blossom & Joyous Spring", 2),
                entry(DeckZone::Side, "Droll & Lock Bird", 3),
            ],
        };
        let rounds = vec![round(1, RoundResult::Bye, None, Vec::new())];
        let m = messages(&head(None, None, None), &deck, &rounds);
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[0],
            (
                DECK,
                "### 🃏 Liste jouée : **Snake-Eye - v3**\n\n\
                 **Main (5)**\n\
                 3× Snake-Eye Ash\n\
                 2× Ash Blossom & Joyous Spring\n\n\
                 **Side (3)**\n\
                 3× Droll & Lock Bird"
                    .to_string()
            )
        );
        assert_eq!(m[1].0, 1);

        // before the first round: the list alone; no list, no message
        assert_eq!(messages(&head(None, None, None), &deck, &[]).len(), 1);
        let none = messages(&head(None, None, None), &SnapshotList::default(), &[]);
        assert!(none.is_empty());
    }

    #[test]
    fn a_long_list_puts_its_side_in_a_second_message() {
        let long = "A card with a very long name, the kind Konami prints";
        let mut entries: Vec<SnapshotEntry> = (0..30)
            .map(|i| entry(DeckZone::Main, &format!("{long} {i}"), 1))
            .collect();
        entries.extend((0..15).map(|i| entry(DeckZone::Side, &format!("{long} s{i}"), 1)));
        let deck = SnapshotList { entries };
        let m = deck_messages(&head(None, None, None), &deck);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].0, DECK);
        assert!(!m[0].1.contains("**Side"));
        assert_eq!(m[1].0, DECK_SIDE);
        assert!(m[1]
            .1
            .starts_with("### 🃏 Liste jouée : **Snake-Eye - v3** (side)\n\n**Side (15)**"));
        assert!(m
            .iter()
            .all(|(_, text)| text.chars().count() <= common::discord::MAX_CHARS));
    }
}
