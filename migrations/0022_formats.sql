-- Formats (Edison, Goat…): a card pool (the cards released in the TCG up
-- to `cutoff`) and the format's own list of forbidden and limited cards,
-- copied from a Konami list (`based_on`) when created, then edited. A
-- change of the list on a format already played gets a new `version` (a
-- copy), the old one kept as it was. yugioh_domain::format.

-- Every set's TCG release date, from ygoprodeck's cardsets.php (refreshed
-- with the weekly prices): a card's first release = the earliest date of
-- the sets in its card data's `card_sets`. NULL: no TCG date.
CREATE TABLE ygo_set (
    name      TEXT PRIMARY KEY,
    code      TEXT,
    tcg_date  TEXT
);

CREATE TABLE deck_format (
    id        INTEGER PRIMARY KEY,
    name      TEXT NOT NULL,
    version   INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    cutoff    TEXT NOT NULL,
    based_on  INTEGER REFERENCES advanced_ban_list(id) ON DELETE SET NULL,
    notes     TEXT,
    UNIQUE (name, version)
);

CREATE TABLE deck_format_entry (
    format_id     INTEGER NOT NULL REFERENCES deck_format(id) ON DELETE CASCADE,
    card_data_id  INTEGER NOT NULL REFERENCES card_data(id) ON DELETE RESTRICT,
    status        TEXT NOT NULL CHECK (status IN ('Banned', 'Limited', 'Semi-Limited')),
    PRIMARY KEY (format_id, card_data_id)
);
CREATE INDEX idx_deck_format_entry_card_data ON deck_format_entry(card_data_id);

-- A version plays a ban list or a format (or neither), never both: the
-- deck builder's save refuses both.
ALTER TABLE deck_version ADD COLUMN format_id INTEGER REFERENCES deck_format(id) ON DELETE SET NULL;
