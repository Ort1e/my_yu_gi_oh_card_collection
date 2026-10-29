-- A deck shown on the public site with all its versions (ticked on its edit
-- page): 1, else 0. A deck played in a tournament is shown anyway, by the
-- versions played (yugioh::routes::decks, PUBLIC_DECK / PUBLIC_VERSION).
ALTER TABLE deck ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0;
