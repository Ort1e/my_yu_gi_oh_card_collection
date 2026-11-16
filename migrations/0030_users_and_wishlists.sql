-- Users and their permissions in the database (they came from the
-- environment), and one Pokémon wishlist per person.

-- What a user can be granted: 'yugioh', 'books', 'pokemon' (the collection:
-- copies, openings), and 'pokemon_wishlist:<id>' (one per wishlist, added,
-- renamed and removed with it). `label` goes after "change": "the books".
CREATE TABLE app_permission (
    key      TEXT PRIMARY KEY,
    label    TEXT NOT NULL,
    position INTEGER NOT NULL
);

INSERT INTO app_permission (key, label, position) VALUES
    ('yugioh',  'the Yu-Gi-Oh! cards', 1),
    ('books',   'the books',           2),
    ('pokemon', 'the Pokémon cards',   3);

-- `name`: lower case letters, digits and _ (the session cookie splits on
-- dots). `password_hash`: an argon2 PHC string.
CREATE TABLE app_user (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    is_admin      INTEGER NOT NULL DEFAULT 0 CHECK (is_admin IN (0, 1)),
    created_on    TEXT NOT NULL
);

CREATE TABLE app_user_permission (
    user_id    INTEGER NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,
    permission TEXT NOT NULL REFERENCES app_permission(key)
                   ON DELETE CASCADE ON UPDATE CASCADE,
    PRIMARY KEY (user_id, permission)
);

CREATE TABLE pokemon_wishlist (
    id                    INTEGER PRIMARY KEY,
    name                  TEXT NOT NULL UNIQUE COLLATE NOCASE,
    -- 1: a copy added to the collection ticks off this list's matching wishes
    fills_from_collection INTEGER NOT NULL DEFAULT 0 CHECK (fills_from_collection IN (0, 1)),
    position              INTEGER NOT NULL DEFAULT 0
);

-- today's wishes: the owner's
INSERT INTO pokemon_wishlist (id, name, fills_from_collection, position)
    VALUES (1, 'Ortie', 1, 1);
INSERT INTO app_permission (key, label, position)
    VALUES ('pokemon_wishlist:1', 'the Pokémon wishlist “Ortie”', 101);

-- nullable in SQL (SQLite: a new REFERENCES column can't have a default),
-- always set by the code
ALTER TABLE pokemon_wish ADD COLUMN wishlist_id INTEGER
    REFERENCES pokemon_wishlist(id) ON DELETE CASCADE;
UPDATE pokemon_wish SET wishlist_id = 1;

-- got = got_on is set; the copy that fulfilled it is optional
ALTER TABLE pokemon_wish ADD COLUMN got_on TEXT;
UPDATE pokemon_wish SET got_on = COALESCE(
        (SELECT c.obtained_on FROM pokemon_copy c WHERE c.id = pokemon_wish.got_copy_id),
        (SELECT c.added_on FROM pokemon_copy c WHERE c.id = pokemon_wish.got_copy_id))
    WHERE got_copy_id IS NOT NULL;
CREATE INDEX pokemon_wish_list ON pokemon_wish(wishlist_id);
