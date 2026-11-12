-- The wishlist's booster suggestions (pokemon::store::boosters), refreshed
-- weekly with the prices from Cardmarket's catalogue files (game 6), which
-- are never stored.

-- The Cardmarket expansion of a card's product (TCGdex's
-- pricing.cardmarket.idProduct), NULL: not known (no product, or not
-- refreshed since the card came).
ALTER TABLE pokemon_card ADD COLUMN cardmarket_expansion INTEGER;

-- The booster of each expansion of our cards, with its latest price (EUR,
-- over every language). No row: the expansion has no booster (promos).
CREATE TABLE pokemon_booster (
    id_expansion INTEGER PRIMARY KEY,
    id_product   INTEGER NOT NULL,
    name         TEXT NOT NULL,         -- Cardmarket's, English: 'Surging Sparks Booster'
    price        TEXT,                  -- money, NULL: no price
    date         TEXT NOT NULL          -- YYYY-MM-DD, the refresh
);
