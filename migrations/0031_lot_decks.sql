-- A deck received in a lot (the lot's `Add a deck` page): its snapshot is
-- linked to the lot, like a tournament's list is to its tournament. One lot
-- deck per lot; lots are never deleted.
ALTER TABLE deck_snapshot ADD COLUMN lot_id INTEGER REFERENCES lot (id);
CREATE UNIQUE INDEX deck_snapshot_lot ON deck_snapshot (lot_id) WHERE lot_id IS NOT NULL;
