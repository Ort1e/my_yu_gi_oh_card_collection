-- Rarity of each physical card ("Ultra Rare"…), as the label of
-- domain::rarity::Rarity. No CHECK: the list lives in domain/ and the forms
-- validate against it. NULL when unknown (most older cards).
ALTER TABLE card ADD COLUMN rarity TEXT;
