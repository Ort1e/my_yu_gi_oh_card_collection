-- Built decks (routes/decks/api/version.rs, domain::built_deck): a built
-- version locks its real cards, which then belong to it alone (out of the
-- collection for every other version). Free text for where the physical deck
-- lives; NULL when not noted.
ALTER TABLE deck_version ADD COLUMN is_built INTEGER NOT NULL DEFAULT 0
    CHECK (is_built IN (0, 1));
ALTER TABLE deck_version ADD COLUMN deck_box TEXT;   -- "red Boulder 100+"
ALTER TABLE deck_version ADD COLUMN sleeves TEXT;    -- "Dragon Shield matte black"
