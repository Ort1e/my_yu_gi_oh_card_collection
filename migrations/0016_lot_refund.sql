-- What the seller gave back on a lot (Dateacard refunds the shipping):
-- money, '0.00' when nothing. A lot's cost is `price - refund`, counted in
-- the lot's month; `price` stays what was paid.
ALTER TABLE lot ADD COLUMN refund TEXT NOT NULL DEFAULT '0.00';
