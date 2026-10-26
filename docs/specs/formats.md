# Spec: formats in the deck builder (Edison…)

Status: **designed, to review** (2026-10-26). Owner's answers: a deck
version uses a ban list **or** a format; a format whose list is changed
gets a new version; the cutoff is a date (a set's date is the same thing).

## What a format is

A named way to play: a **card pool** (the cards released in the TCG on or
before a **cutoff date**) and **its own list** of forbidden and limited
cards. Example: Edison = Konami's list of 2010-03-01 (already imported:
135 cards) and the cards released up to March 2010; the community's later
changes to that list make **Edison v2**, v1 kept as it was (decks saved
with v1 still say v1).

## Data (migration 0017)

- `deck_format` (`id`, `name`, `version` INTEGER, `cutoff` DATE TEXT,
  `based_on` → `advanced_ban_list` NULL, `notes`), UNIQUE (`name`,
  `version`).
- `deck_format_entry` (`format_id`, `card_data_id`, `status` Banned /
  Limited / Semi-Limited): the format's list, copied from a Konami list
  when the format is created, then edited. A new version copies the
  previous one.
- `deck_version.format_id` → `deck_format` NULL. A version has a ban list
  or a format or neither, never both (the save refuses both).
- `ygo_set` (`name`, `code`, `tcg_date`): every set with its TCG release
  date, from ygoprodeck's `cardsets.php` (one request, refreshed with the
  weekly prices). A card's **first TCG release** is the earliest date of
  the sets in its stored `card_sets`; no request per card. A card with no
  TCG set (OCG only, 9 card data today) has no date: out of every pool.

## Logic (the owner's part: stubs and tests left by Claude)

In `yugioh-domain/src/format.rs`:

- `first_release(card_sets: &[&str], set_dates: &HashMap<&str, NaiveDate>) -> Option<NaiveDate>`
  (set names → the earliest date);
- `check(deck: &[DeckCard], format: &FormatRules) -> Vec<Problem>`: a card
  released after the cutoff, or without a date; more copies than its
  status allows (forbidden 0, limited 1, semi-limited 2, else 3, counted
  across main + extra + side by card name). `Problem` says which card and
  why, for the deck builder to show.

Claude writes everything around: migration, `ygo_set` refresh, the pages,
the API, the deck builder.

## Pages

- `/formats` (Decks menu): the formats, newest version first.
- `/formats/add`: name, cutoff (a date, or "the date of set …" picked from
  `ygo_set`), the Konami list it starts from.
- `/formats/:id`: its list (sortable table, search), add a card (by name,
  ygoprodeck fetched if unknown), change a status, remove; "New version"
  (a copy, version + 1, the next edits go there); edit name, cutoff,
  notes. A version used by a deck can still be edited (the owner decides:
  change it, or make a new version).
- Write routes behind the login like every write.

## Deck builder

- The "Ban list" selector becomes **"Ban list or format"**: one list in two
  groups (formats, then Konami lists by date), saved as `ban_list_id` or
  `format_id` in the `PATCH`.
- Each card's statuses (`ban`) also get the formats' (`"f<id>"` keys), and
  its `first_release` date: the tiles show the limit and "not released in
  this format" like the limits today.
- The version panel shows the problems of `check` (an API call, `GET
  /api/deck-versions/:id/check`), refreshed after each save.
- The reserve gets a "Legal in this format only" switch.
- YDKE export unchanged.

## Tests

- Domain: the owner's unit tests (dates, counts by name across zones,
  the three statuses, a card without a date).
- Handlers: create a format from a Konami list (entries copied), new
  version (copied, v1 unchanged), save a version with a format, refusal of
  both, the `check` answer, the format pages behind the login for writes.

## To decide when building

- Edison's exact cutoff date (the community's definition; checked when
  creating it).
- Whether Goat or other formats get created right away (the owner can add
  them from `/formats/add`).
