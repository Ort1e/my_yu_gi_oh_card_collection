-- card.is_proxy: always 0 since 0027 (no proxy card rows any more); nothing
-- indexes, checks or references it, so SQLite drops it in place.
ALTER TABLE card DROP COLUMN is_proxy;
