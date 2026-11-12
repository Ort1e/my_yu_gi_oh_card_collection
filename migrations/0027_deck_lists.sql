-- Deck versions as lists (docs/specs/deck-lists.md, `yugioh_domain::ownership`):
-- a version holds lines (a card and a count), not `card` rows. Only a built
-- version holds copies (`deck_version_copy`); a line short of copies has
-- `card_id` NULL slots ("missing", to buy). No proxy `card` row any more.

-- one row per card and zone, like deck_snapshot_card; name = a copy's name
-- (shown when card_data_id is NULL: a card not linked yet)
CREATE TABLE deck_version_card (
    deck_version_id INTEGER NOT NULL REFERENCES deck_version(id) ON DELETE CASCADE,
    zone            TEXT    NOT NULL CHECK (zone IN ('main', 'extra', 'side')),
    card_data_id    INTEGER REFERENCES card_data(id) ON DELETE SET NULL,
    name            TEXT    NOT NULL,
    count           INTEGER NOT NULL CHECK (count > 0),
    position        INTEGER NOT NULL,
    PRIMARY KEY (deck_version_id, zone, position)
);
CREATE INDEX deck_version_card_data ON deck_version_card (card_data_id);

-- a built version's copies: a line of count n has n slots (0 .. n-1)
CREATE TABLE deck_version_copy (
    deck_version_id INTEGER NOT NULL,
    zone            TEXT    NOT NULL,
    position        INTEGER NOT NULL,
    slot            INTEGER NOT NULL,
    card_id         INTEGER REFERENCES card(id),   -- NULL: missing
    PRIMARY KEY (deck_version_id, zone, position, slot),
    FOREIGN KEY (deck_version_id, zone, position)
        REFERENCES deck_version_card (deck_version_id, zone, position) ON DELETE CASCADE
);
-- a copy is locked by one built version only
CREATE UNIQUE INDEX deck_version_copy_card ON deck_version_copy (card_id) WHERE card_id IS NOT NULL;

-- a plain version the owner means to build (its missing cards go on the
-- shopping list with the built ones')
ALTER TABLE deck_version ADD COLUMN is_planned INTEGER NOT NULL DEFAULT 0;

-- the lines: each zone's copies grouped by card data (by name when not
-- linked), in the order of their first copy
INSERT INTO deck_version_card (deck_version_id, zone, card_data_id, name, count, position)
SELECT z.deck_version_id, z.zone, c.card_data_id, MIN(c.name), COUNT(*),
       ROW_NUMBER() OVER (PARTITION BY z.deck_version_id, z.zone ORDER BY MIN(z.rowid))
FROM (SELECT rowid, deck_version_id, card_id, 'main' AS zone FROM deck_version_main_deck
      UNION ALL
      SELECT rowid, deck_version_id, card_id, 'extra' FROM deck_version_extra_deck
      UNION ALL
      SELECT rowid, deck_version_id, card_id, 'side' FROM deck_version_side_deck) z
JOIN card c ON c.id = z.card_id
GROUP BY z.deck_version_id, z.zone, COALESCE(c.card_data_id, -1),
         CASE WHEN c.card_data_id IS NULL THEN c.name END;

-- a built version's copies: its real cards keep their copy (the first built
-- version holding one keeps it), its proxies become missing slots
WITH z AS (
    SELECT rowid, deck_version_id, card_id, 'main' AS zone FROM deck_version_main_deck
    UNION ALL
    SELECT rowid, deck_version_id, card_id, 'extra' FROM deck_version_extra_deck
    UNION ALL
    SELECT rowid, deck_version_id, card_id, 'side' FROM deck_version_side_deck
),
held AS (
    SELECT z.deck_version_id, z.zone, z.rowid, c.id AS card_id, c.card_data_id, c.name, c.is_proxy,
           ROW_NUMBER() OVER (PARTITION BY c.id ORDER BY z.deck_version_id, z.zone, z.rowid) AS holder
    FROM z
    JOIN deck_version v ON v.id = z.deck_version_id AND v.is_built = 1
    JOIN card c ON c.id = z.card_id
)
INSERT INTO deck_version_copy (deck_version_id, zone, position, slot, card_id)
SELECT h.deck_version_id, h.zone, l.position,
       ROW_NUMBER() OVER (PARTITION BY h.deck_version_id, h.zone, l.position
                          ORDER BY h.is_proxy, h.rowid) - 1,
       CASE WHEN h.is_proxy = 0 AND h.holder = 1 THEN h.card_id END
FROM held h
JOIN deck_version_card l
  ON l.deck_version_id = h.deck_version_id AND l.zone = h.zone
 AND (l.card_data_id = h.card_data_id
      OR (l.card_data_id IS NULL AND h.card_data_id IS NULL AND l.name = h.name));

-- categories are given to a card within a version, not to a copy (a leaf
-- table: rebuilt, nothing cascades); a card not linked loses its categories
CREATE TABLE card_category_assignment_new (
    id           INTEGER PRIMARY KEY,
    category_id  INTEGER NOT NULL REFERENCES card_category(id) ON DELETE CASCADE,
    card_data_id INTEGER NOT NULL REFERENCES card_data(id) ON DELETE CASCADE,
    UNIQUE (category_id, card_data_id)
);
INSERT INTO card_category_assignment_new (category_id, card_data_id)
SELECT DISTINCT a.category_id, c.card_data_id
FROM card_category_assignment a JOIN card c ON c.id = a.card_id
WHERE c.card_data_id IS NOT NULL
ORDER BY a.id;
DROP TABLE card_category_assignment;
ALTER TABLE card_category_assignment_new RENAME TO card_category_assignment;
CREATE INDEX idx_card_category_assignment_card_data ON card_category_assignment (card_data_id);

DROP TABLE deck_version_main_deck;
DROP TABLE deck_version_extra_deck;
DROP TABLE deck_version_side_deck;

-- the proxies were only in decks: gone (card.is_proxy stays 0 one release,
-- then a migration drops it); one in a lot or a calendar is kept
DELETE FROM card WHERE is_proxy = 1
  AND id NOT IN (SELECT card_id FROM unite)
  AND id NOT IN (SELECT card_id FROM advent_calendar_entry WHERE card_id IS NOT NULL);
