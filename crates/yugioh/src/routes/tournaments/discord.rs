//! The live tournament notes in a Discord channel (`DISCORD_WEBHOOK_URL`),
//! for a tournament opted in on the live start page
//! (`tournament.post_to_discord`).
//!
//! The messages are written here from the tournament **as saved**, after
//! every save of it (a round `PUT` or the end `PATCH` of the live page, the
//! edit form): one message per round, the first one under the tournament's
//! head, and the day's summary once its end is noted (placement, players or
//! conclusion). `store::discord::publish` then posts what is new, edits what
//! changed (the opponent's deck or a comment written after the round) and
//! deletes what is gone. In a task, one at a time, reading the tournament
//! inside its turn (the last one sees the last save); a failure is only a
//! `warn!` (the notes are saved anyway). Nothing under `cfg(test)`.

use common::web::filters::french_date;
use tokio::sync::Mutex;
use yugioh_domain::tournament::{stats, Die, Game, GameResult, RoundRecord, RoundResult};

use super::data::{self, RoundView, Which};
use super::{placement_text, record_text, tournament_record};
use crate::store::discord::{publish, SUMMARY};
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
    tokio::spawn(async move {
        let _turn = PUBLISHING.lock().await;
        let messages = match messages_of(&pool, id).await {
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
async fn messages_of(
    pool: &crate::store::db::Pool,
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
    let head = Head {
        name: t.name.clone(),
        date: t.date.clone(),
        location: t.location.clone(),
        is_team: t.is_team,
        deck: t.version_full_name(),
        placement: placement_text(t),
        conclusion: t.conclusion.clone(),
    };
    Ok(Some(messages(&head, loaded.rounds_of(id))))
}

/// What the messages say of the tournament itself.
struct Head {
    name: String,
    date: String,
    location: Option<String>,
    is_team: bool,
    /// "Deck - list" of the list played.
    deck: Option<String>,
    /// "3rd of 32", "" when not noted.
    placement: String,
    conclusion: Option<String>,
}

/// Every message (slot, content), in the order they are posted: the rounds,
/// then the summary when the end of the day is noted.
fn messages(head: &Head, rounds: &[RoundView]) -> Vec<(i64, String)> {
    let mut messages: Vec<(i64, String)> = rounds
        .iter()
        .map(|round| {
            let text = round_text(round);
            if round.number == 1 {
                (round.number, format!("{}\n\n{text}", head_text(head)))
            } else {
                (round.number, text)
            }
        })
        .collect();
    let ended = !head.placement.is_empty() || head.conclusion.is_some();
    if ended {
        messages.push((SUMMARY, summary_text(head, rounds)));
    }
    messages
}

/// `## 🏆 Locals`, then the date, place, deck played.
fn head_text(head: &Head) -> String {
    let mut details = vec![format!("📅 {}", french_date(&head.date))];
    if let Some(location) = &head.location {
        details.push(format!("📍 {location}"));
    }
    if head.is_team {
        details.push("👥 Team".into());
    }
    let mut text = format!("## 🏆 {}\n{}", head.name, details.join(" · "));
    if let Some(deck) = &head.deck {
        text.push_str(&format!("\n🃏 Playing **{deck}**"));
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
        "### Round {} · {} {}",
        round.number,
        result_emoji(round.result),
        round.result.label()
    );
    if let Some(score) = score(round).filter(|_| round.result != RoundResult::Bye) {
        title.push_str(&format!(" {score}"));
    }
    let mut lines = vec![title];

    if round.result != RoundResult::Bye {
        lines.push(match opponent(round) {
            Some(deck) => format!("⚔️ vs **{deck}**"),
            None => "⚔️ vs ❔ *deck not noted yet*".into(),
        });
    }

    let mut details: Vec<String> = Vec::new();
    match round.die {
        Some(Die::Won) => details.push("🎲 Die won".into()),
        Some(Die::Lost) => details.push("🎲 Die lost".into()),
        None => {}
    }
    if let Some(team) = round.team_result {
        details.push(format!("👥 Team: {} {}", result_emoji(team), team.label()));
    }
    if !details.is_empty() {
        lines.push(details.join(" · "));
    }

    for game in &round.games {
        let mut text = format!(
            "- G{} {} {}",
            game.number,
            game_emoji(game.result),
            game.result.label()
        );
        match game.went_first {
            Some(true) => text.push_str(" · went 1st"),
            Some(false) => text.push_str(" · went 2nd"),
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
fn medal(placement: &str) -> &'static str {
    match placement.split(' ').next() {
        Some("1st") => "🥇",
        Some("2nd") => "🥈",
        Some("3rd") => "🥉",
        _ => "🏅",
    }
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

    let mut lines = vec![format!("## 🏁 {}: the day", head.name)];
    let mut figures: Vec<String> = Vec::new();
    if !head.placement.is_empty() {
        figures.push(format!("{} **{}**", medal(&head.placement), head.placement));
    }
    let record = tournament_record(&s);
    if !record.is_empty() {
        figures.push(format!("Rounds **{record}**"));
    }
    if s.games.wins + s.games.losses + s.games.draws > 0 {
        figures.push(format!("Games {}", record_text(&s.games)));
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
            text.push_str(" Bye");
        } else {
            if let Some(score) = score(round) {
                text.push_str(&format!(" {score}"));
            }
            text.push_str(&format!(
                " vs {}",
                opponent(round).unwrap_or_else(|| "?".into())
            ));
        }
        lines.push(text);
    }

    if let Some(conclusion) = &head.conclusion {
        lines.push(String::new());
        lines.push(format!("📝 {conclusion}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::tournaments::data::GameView;

    fn head(placement: &str, conclusion: Option<&str>) -> Head {
        Head {
            name: "Locals".into(),
            date: "2026-11-21".into(),
            location: Some("Paris".into()),
            is_team: false,
            deck: Some("Snake-Eye - v3".into()),
            placement: placement.into(),
            conclusion: conclusion.map(Into::into),
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
            "### Round 2 · ✅ Win 2-1\n\
             ⚔️ vs **Mitsurugi**\n\
             🎲 Die won\n\
             - G1 ✅ Won · went 1st · Ash on the search\n\
             - G2 ❌ Lost · went 2nd\n\
             - G3 ✅ Won\n\
             💬 Close one"
        );

        let mut r = round(1, RoundResult::Loss, None, Vec::new());
        assert_eq!(
            round_text(&r),
            "### Round 1 · ❌ Loss\n⚔️ vs ❔ *deck not noted yet*"
        );
        r.labels = vec!["Ryzeal".into(), "Fiendsmith".into()];
        assert!(round_text(&r).contains("⚔️ vs **Ryzeal / Fiendsmith**"));

        let bye = round(3, RoundResult::Bye, None, Vec::new());
        assert_eq!(round_text(&bye), "### Round 3 · 💤 Bye");
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
        let m = messages(&head("", None), &rounds);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].0, 1);
        assert!(
            m[0].1.starts_with(
                "## 🏆 Locals\n📅 21/11/2026 · 📍 Paris\n🃏 Playing **Snake-Eye - v3**\n\n### Round 1"
            ),
            "{}",
            m[0].1
        );
        assert_eq!(m[1], (2, "### Round 2 · 💤 Bye".to_string()));

        let m = messages(&head("3rd of 32", Some("Good day")), &rounds);
        assert_eq!(m.len(), 3);
        assert_eq!(
            m[2],
            (
                SUMMARY,
                "## 🏁 Locals: the day\n\
                 🥉 **3rd of 32** · Rounds **1-0, 1 bye** · Games 1-0\n\
                 🃏 Snake-Eye - v3\n\
                 \n\
                 R1 ✅ 1-0 vs Mitsurugi\n\
                 R2 💤 Bye\n\
                 \n\
                 📝 Good day"
                    .to_string()
            )
        );
    }
}
