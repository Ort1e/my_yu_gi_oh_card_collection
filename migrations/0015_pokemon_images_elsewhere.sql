-- The pictures borrowed from another print with the same name (0014) were
-- often another artwork (a Trainer Gallery's, a promo's): forget them. The
-- picture route now asks the Pokémon TCG API for the same print's
-- (store::pokemon::cards::find_image): `image_from` NULL = to look up.
UPDATE pokemon_card SET image = NULL, image_from = NULL WHERE image_from IS NOT NULL;
