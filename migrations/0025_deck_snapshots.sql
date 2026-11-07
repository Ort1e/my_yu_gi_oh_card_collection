-- A deck's history: snapshots of what a version's zones held, by card (not by
-- copy: no proxies, no locks), written each time the zones are saved
-- (`yugioh_domain::deck_history`, `store/snapshots.rs`).
--   name           NULL = an automatic one; named ones are kept
--   version_id     the version it was taken from (NULL once that one is deleted)
--   tournament_id  the list played in that tournament (frozen; replaces
--                  deck_version.tournament_id, left NULL and unused from now on)
--   is_public      shown on the public site (a tournament's one is anyway)
CREATE TABLE deck_snapshot (
    id            INTEGER PRIMARY KEY,
    deck_id       INTEGER NOT NULL REFERENCES deck(id) ON DELETE CASCADE,
    version_id    INTEGER REFERENCES deck_version(id) ON DELETE SET NULL,
    created_at    TEXT NOT NULL,   -- YYYY-MM-DD HH:MM:SS, local time
    name          TEXT,
    tournament_id INTEGER UNIQUE REFERENCES tournament(id) ON DELETE SET NULL,
    is_public     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX deck_snapshot_deck ON deck_snapshot (deck_id, created_at);
CREATE INDEX deck_snapshot_version ON deck_snapshot (version_id, created_at);

-- one row per card and zone; name = the card's name when it was taken (also
-- for a card not linked to card data yet)
CREATE TABLE deck_snapshot_card (
    snapshot_id  INTEGER NOT NULL REFERENCES deck_snapshot(id) ON DELETE CASCADE,
    zone         TEXT NOT NULL CHECK (zone IN ('main', 'extra', 'side')),
    card_data_id INTEGER REFERENCES card_data(id) ON DELETE SET NULL,
    name         TEXT NOT NULL,
    count        INTEGER NOT NULL CHECK (count > 0),
    position     INTEGER NOT NULL
);
CREATE INDEX deck_snapshot_card_snapshot ON deck_snapshot_card (snapshot_id, zone, position);

-- Every existing version starts its history with what it holds now, named
-- after it; the one played in a tournament is that tournament's list (dated
-- that day) and stays public like before.
INSERT INTO deck_snapshot (deck_id, version_id, created_at, name, tournament_id, is_public)
SELECT v.deck_id, v.id,
       COALESCE((SELECT t.date || ' 00:00:00' FROM tournament t WHERE t.id = v.tournament_id),
                datetime('now', 'localtime')),
       v.version_name, v.tournament_id, v.is_public
FROM deck_version v
WHERE v.deck_id IS NOT NULL
ORDER BY v.id;

INSERT INTO deck_snapshot_card (snapshot_id, zone, card_data_id, name, count, position)
SELECT s.id, z.zone, c.card_data_id, MIN(c.name), COUNT(*),
       ROW_NUMBER() OVER (PARTITION BY s.id, z.zone ORDER BY MIN(z.rowid))
FROM (SELECT rowid, deck_version_id, card_id, 'main' AS zone FROM deck_version_main_deck
      UNION ALL
      SELECT rowid, deck_version_id, card_id, 'extra' FROM deck_version_extra_deck
      UNION ALL
      SELECT rowid, deck_version_id, card_id, 'side' FROM deck_version_side_deck) z
JOIN deck_snapshot s ON s.version_id = z.deck_version_id
JOIN card c ON c.id = z.card_id
GROUP BY s.id, z.zone, COALESCE(c.card_data_id, -1), CASE WHEN c.card_data_id IS NULL THEN c.name END;

UPDATE deck_version SET is_public = 1 WHERE tournament_id IS NOT NULL;
UPDATE deck_version SET tournament_id = NULL;
