# Spec: structured tournament results

TODO.md, "Structured tournament results". Today a tournament is a name, a
date, a location and free `notes`, edited in the deck builder's tournament
panel (`decks/api/tournament.rs`, `frontend/src/lib/Tournament.svelte`) and
linked to the version played by `deck_version.tournament_id`. The 7 existing
tournaments' notes all tell the same story (rounds, opponent's deck, die,
games with a comment, placement, conclusion) but in two different writing
styles. Goal: store that story as data, show every tournament on one page,
and compute stats from it.

**Status: done by Claude on branch `tournaments` (2026-10-22), except the
owner's `domain::tournament::stats` and `Record::rate` (stubs + tests).** Delete
this spec once they are written (what's useful is in docs/tour.md,
"Tournaments").

**Who writes what** (CLAUDE.md, "Division of work"):
- **the owner**: `domain::tournament`'s stats (section 3): the round and game
  counting rules, win rates, the groupings (per deck version, per label,
  matchups, die, first/second). Claude leaves the types, the stubs, their doc
  comments and their unit tests;
- **Claude**: everything else: the migration, the label writes, the forms and
  their validation, the pages, the deck builder panel, moving the old notes
  (section 6: transcribed by Claude, loaded by a one-off command),
  docs, handler tests.

## Decisions

- **Rounds, then games.** A tournament has numbered rounds; a round has any
  number of games (usually up to 3, more when a game is a draw). The round result is chosen, not derived from the games: a double
  loss or a forfeit can't be read from them.
- **Round results**: `win`, `loss`, `double_loss` (time ran out unfinished:
  both players lose; there are no draws), `bye`, `forfeit_win`.
- **Game results**: `won`, `lost`, `draw` (a drawn game is replayed, so a
  round can have more than 3 games), `unfinished` (cut by time: the game of
  a double loss). Rounds have no draws (time out = double loss), games do.
- **Die**: won or lost, per round (NULL for a bye). **First/second** per game.
- **Labels** (archetype/engine names) tag decks: an opponent's deck has
  several (`Mitsurugi` + `K9` for "mitsu/K9"), and so does each of your own
  decks (on the `deck`, shared by its versions). One table for both, one
  label per strategy whatever the spelling: the forms suggest the existing
  labels and create new ones on the way; a labels page renames and merges.
- **Team events**: a flag on the tournament, and a team result per round.
- **The old notes are moved by Claude, not by hand** (7 tournaments, section
  6): the two writing styles make a parser fragile for so few, so Claude
  transcribes them once into a data file loaded by a one-off command, and
  the owner checks the result on the pages. `tournament.notes` is kept
  (shown as "Old notes" on the detail page while not empty).
- **Reads are public** like the decks; writes in `write_router()`.

## 1. Migration `0011_tournament_results.sql`

```sql
ALTER TABLE tournament ADD COLUMN placement INTEGER CHECK (placement >= 1);
ALTER TABLE tournament ADD COLUMN players INTEGER CHECK (players >= 1);
ALTER TABLE tournament ADD COLUMN conclusion TEXT;
ALTER TABLE tournament ADD COLUMN is_team INTEGER NOT NULL DEFAULT 0
    CHECK (is_team IN (0, 1));

CREATE TABLE deck_label (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

-- your own decks' labels
CREATE TABLE deck_deck_label (
    deck_id  INTEGER NOT NULL REFERENCES deck(id) ON DELETE CASCADE,
    label_id INTEGER NOT NULL REFERENCES deck_label(id) ON DELETE CASCADE,
    PRIMARY KEY (deck_id, label_id)
);

CREATE TABLE tournament_round (
    id            INTEGER PRIMARY KEY,
    tournament_id INTEGER NOT NULL REFERENCES tournament(id) ON DELETE CASCADE,
    number        INTEGER NOT NULL CHECK (number >= 1),
    opponent_deck TEXT,              -- as written: "clown crew/mitsu"; NULL for a bye
    die           TEXT CHECK (die IN ('won', 'lost')),
    result        TEXT NOT NULL CHECK (result IN
                      ('win', 'loss', 'double_loss', 'bye', 'forfeit_win')),
    team_result   TEXT CHECK (team_result IN ('win', 'loss', 'double_loss')),
    comment       TEXT,              -- the round's own line ("pas eu de chance")
    UNIQUE (tournament_id, number)
);

-- the opponent's deck labels
CREATE TABLE tournament_round_label (
    round_id INTEGER NOT NULL REFERENCES tournament_round(id) ON DELETE CASCADE,
    label_id INTEGER NOT NULL REFERENCES deck_label(id) ON DELETE CASCADE,
    PRIMARY KEY (round_id, label_id)
);

CREATE TABLE tournament_game (
    id         INTEGER PRIMARY KEY,
    round_id   INTEGER NOT NULL REFERENCES tournament_round(id) ON DELETE CASCADE,
    number     INTEGER NOT NULL CHECK (number >= 1),
    result     TEXT NOT NULL CHECK (result IN ('won', 'lost', 'draw', 'unfinished')),
    went_first INTEGER CHECK (went_first IN (0, 1)),   -- NULL: not noted
    comment    TEXT,
    UNIQUE (round_id, number)
);
```

The version played stays `deck_version.tournament_id`: at most one version
per tournament (the form clears it on the others), none when unknown. A
copy of a version (`clone`) no longer copies its `tournament_id`: the copy
wasn't played there. Tournaments can be deleted (rounds and games cascade; the
version's `tournament_id` goes NULL); labels without any use are deleted
with them, like book authors.

## 2. Validation (Claude, in the form)

- a bye has no opponent, no die, no games, no labels;
- the opponent is optional (not always noted);
- games are numbered 1..n in order, no maximum (draws add games); the round
  result is not checked against the games (a game can be scooped);
- `placement <= players` when both are set; `team_result` only when `is_team`.

## 3. Stats (`domain/src/tournament.rs`, **the owner's**)

Types (Claude writes them):

```rust
pub enum RoundResult { Win, Loss, DoubleLoss, Bye, ForfeitWin }
pub enum Die { Won, Lost }

pub enum GameResult { Won, Lost, Draw, Unfinished }
pub struct Game { pub result: GameResult, pub went_first: Option<bool> }

/// One round, with what the stats group by.
pub struct RoundRecord {
    pub version_id: Option<i64>,     // the deck version played, if known
    pub own_labels: Vec<String>,     // the deck's labels
    pub opponent_labels: Vec<String>,
    pub die: Option<Die>,
    pub result: RoundResult,
    pub games: Vec<Game>,
}

/// Wins, losses and draws of rounds or games (rounds never draw);
/// `rate()` = wins / (wins + losses), draws left out, `None` when there are
/// no wins nor losses.
pub struct Record { pub wins: u32, pub losses: u32, pub draws: u32 }

pub struct Stats {
    pub rounds: Record,
    pub games: Record,
    pub byes: u32,
    pub per_version: Vec<(i64, Record)>,            // rounds, by version id
    pub per_own_label: Vec<(String, Record)>,       // rounds, by label name
    pub matchups: Vec<(String, Record)>,            // rounds, by opponent label
    pub die_won: Record,                            // rounds where the die was won
    pub die_lost: Record,
    pub going_first: Record,                        // games
    pub going_second: Record,
}

pub fn stats(rounds: &[RoundRecord]) -> Stats { todo!("owner") }
```

The rules the tests pin down:
- **rounds**: `Win` and `ForfeitWin` are wins, `Loss` and `DoubleLoss` are
  losses, a `Bye` is neither (counted in `byes`);
- **games**: every noted game counts, whatever the round result (a forfeit
  win has none); a drawn or unfinished game is in `draws`, out of the rate;
- `per_version` leaves out the rounds without a version;
- a round with two opponent labels counts once in **each** label's matchup
  (same for own labels);
- groupings sorted by number of rounds, most played first, then by name/id.

Tests (Claude writes them with the stub): the rules above, an empty input,
a bye-only tournament, a round with a drawn game (4 games), the die and
first/second splits.

## 4. Pages (Claude, `routes/tournaments/`, `templates/tournaments/`)

- **`/tournaments`**: every tournament, latest first: date, name, location,
  deck version played (link to the deck builder), record ("2-1, 1 bye"),
  placement ("7th of 11"). Below, **the stats** (section 3) as tables:
  overall, by deck version, by own label, matchups, die, first/second.
  Filters `?since=YYYY-MM-DD` and `?deck=<id>` (the meta moves: stats since
  a date are the useful ones), applied to the list and the stats alike.
- **`/tournaments/:id`**: the header (placement, team event), each round
  (opponent, labels, die, result, team result, its games with first/second
  and comment), the conclusion, "Old notes" while not empty. Edit and delete
  buttons.
- **`/tournaments/add`** (`?version=<id>` pre-selects the version played)
  and **`/tournaments/:id/edit`**: the tournament fields, the version played
  (a select of all versions, "Deck - version"), then repeating round rows
  (`rounds-<i>-<field>`) with their game rows (`rounds-<i>-games-<j>-<field>`),
  added/removed by `static/js/tournament_form.js`. Labels are a
  comma-separated field with a `<datalist>` of the existing labels. The edit
  page shows the old notes beside the form (side by side on desktop, above
  on phones). `POST /tournaments/:id/delete`.
- **`/tournaments/labels`**: each label with its use counts (own decks,
  opponent rounds); rename, and merge into another label (moves every use).
- **Deck page / deck edit form**: the deck's own labels (same field).
- **Deck builder**: the tournament panel becomes a summary (name, date,
  record, placement) with links to the tournament page, its edit page, or
  "Add a tournament" (`/tournaments/add?version=<id>`). `POST
  /api/deck-versions/:id/tournament` and the notes editing go away; the
  version's `GET` keeps a `tournament` summary.
- **Nav**: a "Tournaments" link (see TODO.md, "tidy the navigation bar").
- Dashboard: nothing for now.

## 5. Tests (`backend/src/tests/tournaments.rs`, `TestApp`)

- create a tournament with rounds, games and labels; new labels created, the
  version's `tournament_id` set; edit it (rounds removed, renumbered);
- validation errors re-render the form with the values kept;
- delete: rounds and games gone, unused labels deleted, the version kept;
- labels: rename; merge moves the uses and deletes the merged label; names
  are case-insensitive (`k9` = `K9`);
- `/tournaments` and its filters; the detail page shows the old notes only
  while not empty;
- the deck builder's version `GET` carries the summary.

## 6. Moving the 7 tournaments (Claude)

1. Claude reads the 7 notes and transcribes each into
   `data/tournaments_from_notes.json` (the **private** data submodule, never
   the main repo: the notes are personal): placement, players, conclusion,
   and per round the opponent deck as written, its labels, die, result,
   comment, and per game result, first/second and comment (the notes'
   wording kept). Where a note is unclear (a missing die, a game order),
   Claude leaves the field empty and lists it for the owner.
2. A one-off command `cargo run -p backend -- import-tournament-notes
   <file>` (`commands/`) loads it in one write transaction: labels created
   on the way, own deck labels set, and it refuses a tournament that already
   has rounds (safe to run twice). `--dry-run` prints counts only.
3. Run on a scratch copy of `data/app.db` first, the owner checks the 7
   detail pages, then on the live DB (server stopped), committed in the
   data submodule. The JSON file is deleted afterwards (the DB holds it).
4. `tournament.notes` stays until the owner clears it from the edit page (a
   "Clear the old notes" checkbox), never automatically.

Test: the command on a synthetic JSON (2 tournaments, a bye, a drawn game,
a new and an existing label), and the refusal on a second run.
