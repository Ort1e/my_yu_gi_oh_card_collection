-- ygoprodeck's archetype names (archetypes.php), refreshed with the weekly
-- prices: the names offered for the opponents' decks. `key` = the name's
-- letters and digits in lower case (yugioh_domain::archetype::key), what a
-- typed label must share with an archetype to take its spelling.
CREATE TABLE ygo_archetype (
    name TEXT NOT NULL PRIMARY KEY COLLATE NOCASE,
    key TEXT NOT NULL
);
CREATE INDEX ygo_archetype_key ON ygo_archetype (key);
