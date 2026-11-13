-- Limitless TCG is now the first place a missing Pokémon picture is looked
-- for: the cards that were given up on ('' = nothing found, no picture at
-- all) are looked up again at their next view.
UPDATE pokemon_card SET image_from = NULL
WHERE image_from = '' AND image IS NULL AND image_fr IS NULL;
