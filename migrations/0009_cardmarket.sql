-- Cardmarket's price guide (store/cardmarket.rs, weekly; domain::cardmarket_catalog):
-- a price per printing, kept only for what the collection needs. The
-- downloaded files themselves are never stored.

-- A set code (ygoprodeck, `SUDA`) and its Cardmarket expansion, found from
-- the card names. id_expansion NULL: not found on checked_at (asked again
-- after 30 days).
CREATE TABLE cardmarket_expansion (
    set_code      TEXT PRIMARY KEY,
    id_expansion  INTEGER,
    checked_at    TEXT NOT NULL     -- YYYY-MM-DD
);

-- The Cardmarket products named like one of our card data: the candidates
-- for the copies' links, with their latest prices (EUR, Money; NULL = none).
CREATE TABLE cardmarket_product (
    id            INTEGER PRIMARY KEY,   -- Cardmarket's idProduct
    name          TEXT NOT NULL,         -- as Cardmarket writes it
    name_key      TEXT NOT NULL,         -- clean_card_name(name), like card_data.en_name
    id_expansion  INTEGER NOT NULL,
    added         TEXT NOT NULL,         -- YYYY-MM-DD, its dateAdded
    trend         TEXT,
    avg30         TEXT,
    low           TEXT,
    prices_date   TEXT                   -- refresh day of these prices
);
CREATE INDEX idx_cardmarket_product_name_key ON cardmarket_product(name_key);

-- Price history of the products some card is linked to: trend, else avg30.
CREATE TABLE cardmarket_price (
    product_id  INTEGER NOT NULL REFERENCES cardmarket_product(id) ON DELETE CASCADE,
    date        TEXT NOT NULL,           -- YYYY-MM-DD, the refresh day
    price       TEXT NOT NULL,
    PRIMARY KEY (product_id, date)
);

-- A copy's printing on Cardmarket. Guessed by the refresh unless chosen on
-- the card's edit page (cardmarket_product_manual = 1, then kept).
ALTER TABLE card ADD COLUMN cardmarket_product_id INTEGER
    REFERENCES cardmarket_product(id) ON DELETE SET NULL;
ALTER TABLE card ADD COLUMN cardmarket_product_manual INTEGER NOT NULL DEFAULT 0;
