-- The ISBN lookups ask Google Books with a key and the BnF too now: forget
-- the "no catalogue knows it" answers (cached for 7 days), so these books
-- are looked up again at once. The books found stay.
DELETE FROM book_data WHERE json_data IS NULL;
