-- Books: a second collection (novels), with authors, series and an ISBN
-- lookup cache, and their own advent calendars. Nothing links them to the
-- cards. See domain::book, domain::isbn, backend routes/books/.

CREATE TABLE book_author (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    -- surname first, lower case (domain::book::author_sort_name)
    sort_name  TEXT NOT NULL
);

CREATE TABLE book_series (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
);

CREATE TABLE book (
    id              INTEGER PRIMARY KEY,
    -- ISBN-13, digits only (domain::isbn); NULL for a book without one.
    -- One ISBN per edition and format: a second copy of the same edition
    -- is refused.
    isbn            TEXT UNIQUE,
    title           TEXT NOT NULL,
    publisher       TEXT,
    published_year  INTEGER,
    language        TEXT,
    page_count      INTEGER,
    series_id       INTEGER REFERENCES book_series(id) ON DELETE SET NULL,
    volume          INTEGER,
    format          TEXT CHECK (format IN ('Paperback', 'Hardcover', 'Ebook', 'Audiobook')),
    notes           TEXT,
    added_on        TEXT NOT NULL
);

CREATE TABLE book_book_author (
    book_id    INTEGER NOT NULL REFERENCES book(id) ON DELETE CASCADE,
    author_id  INTEGER NOT NULL REFERENCES book_author(id) ON DELETE RESTRICT,
    -- 0 = first author
    position   INTEGER NOT NULL,
    PRIMARY KEY (book_id, author_id)
);

-- ISBN lookups (Open Library, Google Books), like card_data/card_translation:
-- json_data is the domain::book::BookInfo found; NULL = nothing found on
-- checked_at, asked again after 7 days.
CREATE TABLE book_data (
    isbn        TEXT PRIMARY KEY,
    source      TEXT,
    json_data   TEXT,
    checked_at  TEXT NOT NULL
);

CREATE TABLE book_advent_calendar (
    id    INTEGER PRIMARY KEY,
    year  INTEGER NOT NULL UNIQUE
);

CREATE TABLE book_advent_calendar_entry (
    id           INTEGER PRIMARY KEY,
    calendar_id  INTEGER NOT NULL REFERENCES book_advent_calendar(id) ON DELETE CASCADE,
    day          INTEGER NOT NULL CHECK (day BETWEEN 1 AND 25),
    book_id      INTEGER NOT NULL REFERENCES book(id) ON DELETE RESTRICT,
    photo        TEXT,   -- path relative to the media root
    UNIQUE (calendar_id, day)
);

CREATE INDEX idx_book_title ON book(title);
CREATE INDEX idx_book_series ON book(series_id);
CREATE INDEX idx_book_book_author_author ON book_book_author(author_id);
CREATE INDEX idx_book_advent_entry_book ON book_advent_calendar_entry(book_id);
