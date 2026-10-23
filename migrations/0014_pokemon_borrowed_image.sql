-- A Pokémon card TCGdex has no picture of yet (the Classic Collection
-- reprints of 30th Celebration) borrows the picture of another print with
-- the same name, the same illustrator's when possible
-- (store::pokemon::cards::borrow_image). NULL: not tried (or its own
-- picture); '' : tried, none found; else the TCGdex id of the print shown.
ALTER TABLE pokemon_card ADD COLUMN image_from TEXT;
