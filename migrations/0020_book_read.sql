-- Whether the book was read (the switches of the book tables and pages, the
-- edit form), for the random pick of the next book (`/books/random`).
-- Public: shown in the public copy, whose random pick uses it too.
ALTER TABLE book ADD COLUMN is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1));
