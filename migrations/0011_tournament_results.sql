-- Structured tournament results (docs/specs/tournaments.md): rounds, games,
-- deck labels. The version played stays deck_version.tournament_id.

ALTER TABLE tournament ADD COLUMN placement INTEGER CHECK (placement >= 1);
ALTER TABLE tournament ADD COLUMN players INTEGER CHECK (players >= 1);
ALTER TABLE tournament ADD COLUMN conclusion TEXT;
ALTER TABLE tournament ADD COLUMN is_team INTEGER NOT NULL DEFAULT 0
    CHECK (is_team IN (0, 1));

-- archetype/engine names, for your decks and the opponents' alike
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
    opponent_deck TEXT,
    die           TEXT CHECK (die IN ('won', 'lost')),
    result        TEXT NOT NULL CHECK (result IN
                      ('win', 'loss', 'double_loss', 'bye', 'forfeit_win')),
    team_result   TEXT CHECK (team_result IN ('win', 'loss', 'double_loss')),
    comment       TEXT,
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
    went_first INTEGER CHECK (went_first IN (0, 1)),
    comment    TEXT,
    UNIQUE (round_id, number)
);
