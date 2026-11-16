-- A copy flagged "to sell" (the card page, the lot page): it still counts as
-- owned, a built deck takes it only when no other free copy exists, and
-- `/cards/to-sell` lists the flagged ones. Cleared when the copy is sold.
ALTER TABLE card ADD COLUMN for_sale INTEGER NOT NULL DEFAULT 0;
