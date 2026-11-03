-- A book kept off the public copy (the "Private" switch of the book form):
-- the app shows it as usual, the public copy's lists, authors, series and
-- random pick leave it out, and its page isn't copied.
ALTER TABLE book ADD COLUMN is_private INTEGER NOT NULL DEFAULT 0 CHECK (is_private IN (0, 1));
