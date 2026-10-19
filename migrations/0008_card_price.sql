-- Market price of one card (by name: ygoprodeck gives one price per card,
-- following its cheapest printing) on one refresh date (store/card_prices.rs,
-- weekly). Never pruned: the collection's value over time is computed from
-- these rows and the lot dates (domain::valuation).
CREATE TABLE card_price (
    card_data_id      INTEGER NOT NULL REFERENCES card_data(id) ON DELETE CASCADE,
    date              TEXT NOT NULL,   -- YYYY-MM-DD, the refresh day
    cardmarket_price  TEXT,            -- EUR, Money; NULL = ygoprodeck had none (or "0")
    tcgplayer_price   TEXT,            -- USD, Money; kept for the history, not shown yet
    PRIMARY KEY (card_data_id, date)
);
CREATE INDEX idx_card_price_date ON card_price(date);
