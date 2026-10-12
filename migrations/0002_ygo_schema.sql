-- Baseline schema, mirroring the Django models of `main` (my_ygo_cards/models/).
-- Until the Rust app replaces Django, app.db is rebuilt from the Django dump
-- (`cargo run -p importer -- data/data.json`), so this file may still be edited.
--
-- Conventions:
--   * money (price, amount, ...) is TEXT, not REAL: Django DecimalFields have
--     exact precision and floats would silently round. Parse with rust_decimal.
--   * dates are ISO TEXT (YYYY-MM-DD), booleans are INTEGER 0/1.
--   * ON DELETE mirrors Django's on_delete (PROTECT -> RESTRICT).

-- Card catalog data pulled from the ygoprodeck API (one row per distinct card)
CREATE TABLE card_data (
    id          INTEGER PRIMARY KEY,
    en_name     TEXT NOT NULL UNIQUE,
    ygopro_id   INTEGER UNIQUE,
    card_type   TEXT,
    json_data   TEXT NOT NULL   -- raw JSON blob from the API, stored as text
);

-- A physical copy you own of a card_data entry
CREATE TABLE card (
    id                  INTEGER PRIMARY KEY,
    name                TEXT NOT NULL,
    en_name             TEXT,
    card_data_id        INTEGER REFERENCES card_data(id) ON DELETE SET NULL,
    code                TEXT,
    last_known_status   TEXT CHECK (last_known_status IN ('MT', 'NM', 'EX', 'G', 'P')),
    is_proxy            INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tournament (
    id        INTEGER PRIMARY KEY,
    name      TEXT NOT NULL,
    date      TEXT NOT NULL,
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
    date  TEXT NOT NULL UNIQUE
);

CREATE TABLE deck_version (
    id            INTEGER PRIMARY KEY,
    deck_id       INTEGER REFERENCES deck(id) ON DELETE CASCADE,
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
    name  TEXT NOT NULL UNIQUE,
    url   TEXT
);

CREATE TABLE seller (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    is_person  INTEGER NOT NULL DEFAULT 1,
    source_id  INTEGER REFERENCES seller_source(id) ON DELETE SET NULL
);

CREATE TABLE lot (
    id             INTEGER PRIMARY KEY,
    lot_type       TEXT NOT NULL DEFAULT 'purchase' CHECK (lot_type IN ('purchase', 'sale')),
    price          TEXT NOT NULL,
    buy_date       TEXT NOT NULL,
    received_date  TEXT,
    seller_id      INTEGER REFERENCES seller(id) ON DELETE SET NULL,
    is_cancelled   INTEGER NOT NULL DEFAULT 0,
    no_card_price  TEXT NOT NULL DEFAULT '0.00',
    shipment_file  TEXT   -- path relative to the media root, e.g. shipment_files/shipment_1.pdf
);

-- "unite" = one card unit within a lot, individually priced
CREATE TABLE unite (
    id       INTEGER PRIMARY KEY,
    price    TEXT,
    lot_id   INTEGER NOT NULL REFERENCES lot(id) ON DELETE CASCADE,
    card_id  INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE,
    UNIQUE (lot_id, card_id)
);

CREATE TABLE monthly_budget (
    id      INTEGER PRIMARY KEY,
    month   TEXT NOT NULL UNIQUE,   -- first day of the month
    amount  TEXT NOT NULL
);

-- one-off budget contribution
CREATE TABLE apport (
    id           INTEGER PRIMARY KEY,
    budget_id    INTEGER NOT NULL REFERENCES monthly_budget(id) ON DELETE CASCADE,
    description  TEXT NOT NULL DEFAULT '',
    amount       TEXT NOT NULL,
    date         TEXT NOT NULL
);

-- CardCategory / CardConditionalCategory were Django multi-table
-- inheritance (CardConditionalCategory extends CardCategory, sharing its
-- primary key via cardcategory_ptr). Flattened here into one table: a row
-- is "conditional" iff condition_description IS NOT NULL.
CREATE TABLE card_category (
    id                      INTEGER PRIMARY KEY,
    name                    TEXT NOT NULL,
    deck_version_id         INTEGER NOT NULL REFERENCES deck_version(id) ON DELETE CASCADE,
    condition_description   TEXT,
    categorie_true_id       INTEGER REFERENCES card_category(id) ON DELETE CASCADE,
    categorie_false_id      INTEGER REFERENCES card_category(id) ON DELETE CASCADE,
    CHECK (condition_description IS NULL OR categorie_true_id IS NOT NULL)
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
    card_id      INTEGER NOT NULL REFERENCES card(id) ON DELETE CASCADE,
    UNIQUE (category_id, card_id)
);

CREATE TABLE ban_list_entry (
    id            INTEGER PRIMARY KEY,
    ban_list_id   INTEGER NOT NULL REFERENCES advanced_ban_list(id) ON DELETE CASCADE,
    card_data_id  INTEGER NOT NULL REFERENCES card_data(id) ON DELETE RESTRICT,
    status        TEXT NOT NULL CHECK (status IN ('Banned', 'Limited', 'Semi-Limited'))
);

CREATE TABLE advent_calendar (
    id    INTEGER PRIMARY KEY,
    year  INTEGER NOT NULL UNIQUE
);

CREATE TABLE advent_calendar_entry (
    id           INTEGER PRIMARY KEY,
    calendar_id  INTEGER NOT NULL REFERENCES advent_calendar(id) ON DELETE CASCADE,
    day          INTEGER NOT NULL CHECK (day BETWEEN 1 AND 25),
    card_id      INTEGER NOT NULL REFERENCES card(id) ON DELETE RESTRICT,
    photo        TEXT,   -- path relative to the media root
    UNIQUE (calendar_id, day)
);

CREATE INDEX idx_card_card_data ON card(card_data_id);
CREATE INDEX idx_card_en_name ON card(en_name);
CREATE INDEX idx_deck_version_deck ON deck_version(deck_id);
CREATE INDEX idx_deck_version_main_deck_card ON deck_version_main_deck(card_id);
CREATE INDEX idx_deck_version_extra_deck_card ON deck_version_extra_deck(card_id);
CREATE INDEX idx_deck_version_side_deck_card ON deck_version_side_deck(card_id);
CREATE INDEX idx_lot_seller ON lot(seller_id);
CREATE INDEX idx_unite_card ON unite(card_id);
CREATE INDEX idx_apport_budget ON apport(budget_id);
CREATE INDEX idx_card_category_deck_version ON card_category(deck_version_id);
CREATE INDEX idx_card_category_assignment_card ON card_category_assignment(card_id);
CREATE INDEX idx_ban_list_entry_ban_list ON ban_list_entry(ban_list_id);
CREATE INDEX idx_ban_list_entry_card_data ON ban_list_entry(card_data_id);
