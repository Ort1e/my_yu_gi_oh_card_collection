-- A copy's picture of its own printing, from Yugipedia's card galleries
-- (yugioh/src/store/yugipedia.rs, yugioh_domain::printing_picture): the
-- file name (`WitnessoftheAncient-CORI-FR-UR-1E-EA.png`), NULL = ygoprodeck's
-- picture of its card data. `picture_manual` = 1: chosen on the edit page
-- (ygoprodeck's one included), kept by the automatic pass (`find-pictures`).
ALTER TABLE card ADD COLUMN picture TEXT;
ALTER TABLE card ADD COLUMN picture_manual INTEGER NOT NULL DEFAULT 0
    CHECK (picture_manual IN (0, 1));

-- Yugipedia's card galleries, a cache: the file names of a card data's
-- gallery (JSON list), NULL = no gallery found on `checked_at`; asked again
-- after 30 days.
CREATE TABLE yugipedia_gallery (
    card_data_id INTEGER PRIMARY KEY REFERENCES card_data(id) ON DELETE CASCADE,
    files        TEXT,
    checked_at   TEXT NOT NULL
);
