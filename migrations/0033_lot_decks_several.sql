-- Several decks received in one lot (was one, 0031).
DROP INDEX deck_snapshot_lot;
CREATE INDEX deck_snapshot_lot ON deck_snapshot (lot_id) WHERE lot_id IS NOT NULL;
