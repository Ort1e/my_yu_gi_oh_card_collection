-- Pokémon booster openings (routes/pokemon/openings.rs): the booster form
-- saves one, its copies point to it, and cards can be added to it later.

CREATE TABLE pokemon_opening (
    id         INTEGER PRIMARY KEY,
    language   TEXT NOT NULL,            -- 'fr', 'en', 'ja'…
    set_code   TEXT NOT NULL,            -- as typed in the form ('30C'), or a TCGdex set id
    set_name   TEXT NOT NULL,            -- 'Fable Nébuleuse', '30ᵉ Anniversaire / …'
    opened_on  TEXT,                     -- YYYY-MM-DD
    note       TEXT,
    added_on   TEXT NOT NULL
);

-- NULL: not from an opening (a single, a gift…). An opening is never
-- deleted (the collection's history); SET NULL only for safety.
ALTER TABLE pokemon_copy ADD COLUMN opening_id INTEGER
    REFERENCES pokemon_opening(id) ON DELETE SET NULL;

CREATE INDEX pokemon_copy_opening ON pokemon_copy (opening_id);

-- The booster cards saved before openings existed: one opening per day and
-- language (a day's boosters of one language become one opening; edit it
-- or move cards on their edit page).
INSERT INTO pokemon_opening (language, set_code, set_name, opened_on, added_on)
SELECT pc.language, min(c.set_id), group_concat(DISTINCT c.set_name), pc.obtained_on,
       min(pc.added_on)
FROM pokemon_copy pc JOIN pokemon_card c ON c.id = pc.card_id
WHERE pc.source = 'booster'
GROUP BY pc.language, pc.obtained_on;

UPDATE pokemon_copy SET opening_id = (
    SELECT o.id FROM pokemon_opening o
    WHERE o.language = pokemon_copy.language AND o.opened_on IS pokemon_copy.obtained_on
)
WHERE source = 'booster';
