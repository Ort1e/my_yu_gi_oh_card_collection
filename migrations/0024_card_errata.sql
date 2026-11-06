-- Errata (Yugipedia's `Card Errata:<name>` page, English section): the
-- versions of a card's text, parsed (`yugioh_domain::errata::TextVersion`,
-- JSON), oldest first. NULL = no errata page (or no version in it); asked
-- again after 30 days, like `yugipedia_gallery`.
CREATE TABLE yugipedia_errata (
    card_data_id INTEGER PRIMARY KEY REFERENCES card_data(id) ON DELETE CASCADE,
    versions     TEXT,
    checked_at   TEXT NOT NULL
);
