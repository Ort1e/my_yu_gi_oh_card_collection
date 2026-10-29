-- A deck version shown on the public site (the button on its page): 1, else
-- 0. A version played in a tournament is shown anyway. It replaces
-- deck.is_public (0017, a whole deck): the versions of a deck ticked take it.
ALTER TABLE deck_version ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0;
UPDATE deck_version SET is_public = 1
    WHERE deck_id IN (SELECT id FROM deck WHERE is_public = 1);
ALTER TABLE deck DROP COLUMN is_public;
