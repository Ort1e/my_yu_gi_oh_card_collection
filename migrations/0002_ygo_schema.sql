-- Card catalog data pulled from the ygoprodeck API (one row per distinct card)
CREATE TABLE card_data (
    id          INTEGER PRIMARY KEY,
    en_name     TEXT NOT NULL,
    ygopro_id   INTEGER NOT NULL,
    card_type   TEXT NOT NULL,
    json_data   TEXT NOT NULL   -- raw JSON blob from the API, stored as text
);

-- A physical copy you own of a card_data entry
CREATE TABLE card (
    id                  INTEGER PRIMARY KEY,
    name                TEXT NOT NULL,
    en_name             TEXT NOT NULL,
    card_data_id        INTEGER REFERENCES card_data(id),
    code                TEXT,
    last_known_status   TEXT,
    is_proxy            INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tournament (
    id        INTEGER PRIMARY KEY,
    name      TEXT NOT NULL,
    date      TEXT NOT NULL,   -- ISO date (YYYY-MM-DD)
    location  TEXT,
    notes     TEXT
);

CREATE TABLE deck (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT
);

CREATE TABLE advanced_ban_list (
    id    INTEGER PRIMARY KEY,
    date  TEXT NOT NULL
);

CREATE TABLE deck_version (
    id            INTEGER PRIMARY KEY,
    deck_id       INTEGER NOT NULL REFERENCES deck(id) ON DELETE CASCADE,
    version_name  TEXT NOT NULL,
    tournament_id INTEGER REFERENCES tournament(id) ON DELETE SET NULL,
    ban_list_id   INTEGER REFERENCES advanced_ban_list(id) ON DELETE SET NULL
);

-- main_deck / extra_deck / side_deck were Django ManyToManyFields to Card
CREATE TABLE deck_version_main_deck (
    deck_version_id INTEGER NOT NULL REFERENCES deck_version(id) ON DELETE CASCADE,
    card_id         INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE,
    PRIMARY KEY (deck_version_id, card_id)
);
CREATE TABLE deck_version_extra_deck (
    deck_version_id INTEGER NOT NULL REFERENCES deck_version(id) ON DELETE CASCADE,
    card_id         INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE,
    PRIMARY KEY (deck_version_id, card_id)
);
CREATE TABLE deck_version_side_deck (
    deck_version_id INTEGER NOT NULL REFERENCES deck_version(id) ON DELETE CASCADE,
    card_id         INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE,
    PRIMARY KEY (deck_version_id, card_id)
);

CREATE TABLE seller_source (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL,
    url   TEXT
);

CREATE TABLE seller (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    is_person  INTEGER NOT NULL DEFAULT 0,
    source_id  INTEGER REFERENCES seller_source(id)
);

-- price / no_card_price / amount fields are stored as TEXT, not REAL:
-- Django DecimalFields have exact precision, and floats would silently
-- round money values. Read/write them as strings from Rust too, or
-- parse with a decimal crate (e.g. rust_decimal) if you need arithmetic.
CREATE TABLE lot (
    id             INTEGER PRIMARY KEY,
    lot_type       TEXT NOT NULL,
    price          TEXT NOT NULL,
    buy_date       TEXT,
    received_date  TEXT,
    seller_id      INTEGER REFERENCES seller(id),
    is_cancelled   INTEGER NOT NULL DEFAULT 0,
    no_card_price  TEXT NOT NULL DEFAULT '0.00',
    shipment_file  TEXT
);

-- "unite" = one card unit within a lot, individually priced
CREATE TABLE unite (
    id       INTEGER PRIMARY KEY,
    price    TEXT,
    lot_id   INTEGER NOT NULL REFERENCES lot(id) ON DELETE CASCADE,
    card_id  INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE
);

CREATE TABLE monthly_budget (
    id      INTEGER PRIMARY KEY,
    month   TEXT NOT NULL,
    amount  TEXT NOT NULL
);

CREATE TABLE apport (
    id           INTEGER PRIMARY KEY,
    budget_id    INTEGER NOT NULL REFERENCES monthly_budget(id) ON DELETE CASCADE,
    description  TEXT,
    amount       TEXT NOT NULL,
    date         TEXT NOT NULL
);

-- CardCategory / CardConditionalCategory were Django multi-table
-- inheritance (CardConditionalCategory extends CardCategory, sharing its
-- primary key via cardcategory_ptr). Flattened here into one table with
-- nullable columns for the "conditional" subset instead of replicating
-- the two-table split — simpler for a rewrite, same information.
CREATE TABLE card_category (
    id                      INTEGER PRIMARY KEY,
    name                    TEXT NOT NULL,
    deck_version_id         INTEGER REFERENCES deck_version(id) ON DELETE CASCADE,
    -- NULL unless this row is also a "conditional" category:
    condition_description   TEXT,
    categorie_true_id       INTEGER REFERENCES card_category(id),
    categorie_false_id      INTEGER REFERENCES card_category(id)
);

-- categories_or_conditions M2M (self-referential, conditional -> category)
CREATE TABLE card_category_condition (
    category_id           INTEGER NOT NULL REFERENCES card_category(id) ON DELETE CASCADE,
    condition_category_id INTEGER NOT NULL REFERENCES card_category(id) ON DELETE CASCADE,
    PRIMARY KEY (category_id, condition_category_id)
);

CREATE TABLE card_category_assignment (
    id           INTEGER PRIMARY KEY,
    category_id  INTEGER NOT NULL REFERENCES card_category(id) ON DELETE CASCADE,
    card_id      INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE
);

CREATE TABLE ban_list_entry (
    id            INTEGER PRIMARY KEY,
    ban_list_id   INTEGER NOT NULL REFERENCES advanced_ban_list(id) ON DELETE CASCADE,
    card_data_id  INTEGER NOT NULL REFERENCES card_data(id) ON DELETE CASCADE,
    status        TEXT NOT NULL
);

CREATE TABLE advent_calendar (
    id    INTEGER PRIMARY KEY,
    year  INTEGER NOT NULL
);

CREATE INDEX idx_card_card_data ON card(card_data_id);
CREATE INDEX idx_unite_lot ON unite(lot_id);
CREATE INDEX idx_unite_card ON unite(card_id);
CREATE INDEX idx_ban_list_entry_ban_list ON ban_list_entry(ban_list_id);
CREATE INDEX idx_ban_list_entry_card_data ON ban_list_entry(card_data_id);
