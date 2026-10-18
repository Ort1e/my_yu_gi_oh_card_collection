-- The lot a calendar's cards were added to (a gift lot created from the
-- calendar's edit page, or an existing one). NULL: no dedicated lot.
ALTER TABLE advent_calendar ADD COLUMN lot_id INTEGER REFERENCES lot(id) ON DELETE SET NULL;
