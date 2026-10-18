-- Card names in other languages, from ygoprodeck, remembered so each one is
-- asked once (store/card_data.rs, `translated_name`). A name found is kept:
-- names don't change. NULL: ygoprodeck had no translation on `checked_at`
-- (recent sets); asked again after a week.
CREATE TABLE card_translation (
    ygopro_id  INTEGER NOT NULL,
    language   TEXT NOT NULL,       -- ygoprodeck's parameter: 'fr', 'de', 'it', 'pt'
    name       TEXT,
    checked_at TEXT NOT NULL,       -- YYYY-MM-DD
    PRIMARY KEY (ygopro_id, language)
);
