-- A built deck is on the public site by default: building a version now
-- turns its `is_public` on; the versions built before get it here.
UPDATE deck_version SET is_public = 1 WHERE is_built = 1;
