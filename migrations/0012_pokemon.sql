-- The Pokémon card collection (routes/pokemon/, docs/tour.md "The Pokémon
-- cards"): its own tables, nothing shared with the Yu-Gi-Oh! ones. Card data
-- from TCGdex (store/pokemon/tcgdex.rs).

-- TCGdex's cards: those owned or wished (a cache, not the whole catalogue).
-- Two catalogues: the international cards (one card in French, English,
-- German…) and the Japanese ones (their own sets and ids).
CREATE TABLE pokemon_card (
    id           INTEGER PRIMARY KEY,
    catalogue    TEXT NOT NULL CHECK (catalogue IN ('intl', 'ja')),
    tcgdex_id    TEXT NOT NULL,          -- 'sv06.5-067', 'SV6a-010'
    set_id       TEXT NOT NULL,          -- 'sv06.5', 'SV6a'
    set_name     TEXT NOT NULL,          -- French when known, else English / Japanese
    local_id     TEXT NOT NULL,          -- '067' (may hold letters: 'TG05', 'SV107')
    name_fr      TEXT,                   -- Japanese card: its species' names
    name_en      TEXT,
    name_ja      TEXT,                   -- Japanese card only
    name_manual  TEXT,                   -- typed by hand (a Japanese Trainer)
    rarity       TEXT,                   -- TCGdex's English label
    category     TEXT NOT NULL CHECK (category IN ('Pokemon', 'Trainer', 'Energy')),
    types        TEXT,                   -- English, comma separated: 'Water'
    dex_id       INTEGER,
    variants     TEXT NOT NULL DEFAULT '', -- the printed variants: 'normal,reverse'
    image        TEXT,                   -- TCGdex base URL (English or Japanese), no extension
    image_fr     TEXT,                   -- the French print's
    json         TEXT NOT NULL,          -- TCGdex's answer (English, else French; Japanese)
    fetched_at   TEXT NOT NULL,          -- YYYY-MM-DD
    UNIQUE (catalogue, tcgdex_id)
);

-- One physical card. Never deleted (the collection's history): sold instead.
CREATE TABLE pokemon_copy (
    id           INTEGER PRIMARY KEY,
    card_id      INTEGER NOT NULL REFERENCES pokemon_card(id) ON DELETE RESTRICT,
    language     TEXT NOT NULL,          -- 'fr', 'en', 'ja', … ('ja' ⇔ the Japanese catalogue)
    variant      TEXT NOT NULL CHECK (variant IN
                     ('normal', 'holo', 'reverse', 'firstEdition', 'wPromo')),
    condition    TEXT CHECK (condition IN ('MT', 'NM', 'EX', 'G', 'P')),
    full_art     INTEGER NOT NULL DEFAULT 0 CHECK (full_art IN (0, 1)),
    source       TEXT NOT NULL CHECK (source IN
                     ('booster', 'single', 'gift', 'trade', 'other')),
    obtained_on  TEXT,                   -- YYYY-MM-DD, NULL: unknown
    price        TEXT,                   -- money: what a single cost
    grader       TEXT,                   -- 'PSA', 'CGC'…; NULL: not graded
    grade        TEXT,                   -- '10', '9.5'
    cert_number  TEXT,
    estimate     TEXT,                   -- money, by hand: a graded copy's value
    sold_on      TEXT,
    sold_price   TEXT,
    note         TEXT,
    added_on     TEXT NOT NULL,
    CHECK (grade IS NULL OR grader IS NOT NULL),
    CHECK (sold_price IS NULL OR sold_on IS NOT NULL)
);

CREATE INDEX pokemon_copy_card ON pokemon_copy (card_id);

-- A card wanted. Not history: wishes can be deleted.
CREATE TABLE pokemon_wish (
    id          INTEGER PRIMARY KEY,
    card_id     INTEGER NOT NULL REFERENCES pokemon_card(id) ON DELETE RESTRICT,
    language    TEXT,                    -- NULL: any (within the card's catalogue)
    variant     TEXT CHECK (variant IN
                    ('normal', 'holo', 'reverse', 'firstEdition', 'wPromo')),
                                         -- NULL: any
    max_price   TEXT,                    -- money, NULL: no limit
    priority    INTEGER NOT NULL DEFAULT 2 CHECK (priority BETWEEN 1 AND 3),
    note        TEXT,
    added_on    TEXT NOT NULL,
    got_copy_id INTEGER REFERENCES pokemon_copy(id) ON DELETE SET NULL
                                         -- NULL: still wanted
);

-- Cardmarket prices (EUR) from TCGdex, one row per card and refresh date,
-- never pruned. `prices`: JSON {"normal": "0.03", "reverse": "0.10", …},
-- the trend (else the 30-day average) of each variant
-- (domain::pokemon::price::variant_prices).
CREATE TABLE pokemon_price (
    card_id  INTEGER NOT NULL REFERENCES pokemon_card(id) ON DELETE CASCADE,
    date     TEXT NOT NULL,
    prices   TEXT NOT NULL,
    PRIMARY KEY (card_id, date)
);

-- TCGdex's sets, asked for the printed codes ('FNE', 'SFA', 'SV6a') and
-- their card lists (the booster form, the finder). A cache, asked again
-- after 7 days (promo sets grow).
CREATE TABLE pokemon_set (
    language     TEXT NOT NULL,          -- the language asked: 'fr', 'en', 'ja'
    id           TEXT NOT NULL,          -- 'sv06.5'
    name         TEXT NOT NULL,
    code         TEXT,                   -- the printed code in that language: 'FNE'
    official     INTEGER,                -- the set size printed after the number: 064
    cards        TEXT NOT NULL,          -- JSON [{"id", "localId", "name", "image"}]
    fetched_at   TEXT NOT NULL,
    PRIMARY KEY (language, id)
);
