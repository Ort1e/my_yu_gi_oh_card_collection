# Spec: the Pokémon card collection

A third collection in the app, next to the Yu-Gi-Oh! cards and the books: the
owner's Pokémon cards. A small collection, mostly full arts and Water types,
in French, English and Japanese, collected and not played: no decks, no ban
lists, no tournaments. Most cards come **from boosters** opened at home, few
are bought as singles. Two things to do with it: **list the cards owned** and
**keep a wishlist**, both with Cardmarket's market price.

**Status: designed (2026-10-23), not started.**

**Who writes what**: Claude writes all of it, logic included, like the books
(owner's decision, 2026-10-23; CLAUDE.md, "Division of work").

## Decisions

- **Its own section**, like the books: every route starts with `/pokemon`,
  its own tables (`pokemon_*`), navigation (`partials/pokemon_nav.html`) and
  colours (`body.pokemon`, `static/css/pokemon.css`). Nothing shared with the
  Yu-Gi-Oh! tables. The section switch in each header becomes two pills
  (Cards: "Books", "Pokémon"; Books: "Cards", "Pokémon"; Pokémon: "Cards",
  "Books").
- **Reads are public** (collection and wishlist), writes behind the login.
- **Card data from [TCGdex](https://tcgdex.dev)**: free, open source, no
  key. It gives names, set, number, rarity, types, picture, variants (normal,
  holo, reverse…) and **Cardmarket prices** (EUR, trend, averages, product
  id), updated daily. Checked on 2026-10-23. The Pokémon TCG API
  (pokemontcg.io) is being replaced by a paid service (Scrydex): not used.
  Never the Pokémon TCG Pocket cards (TCGdex series `tcgp`).
- **Two catalogues**: TCGdex has one catalogue for the international cards
  (ids like `sv06.5-067`, the same card in French, English, German…) and
  another for the **Japanese** cards (other sets, ids like `SV6a-010`, names
  in Japanese only, their own Cardmarket product and price). A Japanese card
  is shown with its Japanese name and the species' French name below, found
  by its Pokédex number (`dexId` → a French card of that Pokémon); a
  Japanese Trainer or Energy has no `dexId` and keeps its Japanese name only,
  with an optional name typed by hand.
- **A copy** is one physical card: which TCGdex card, language (`fr`, `en`,
  `ja`, any other code TCGdex knows), variant, condition, full art or not,
  **how it came** (booster, single, gift, trade, other) and **when**. A price
  paid only for a single, optional. Graded copies: grader, grade and
  certificate number, and an estimate typed by hand (Cardmarket's guide has
  no graded prices).
- **Never deleted**, like the Yu-Gi-Oh! cards (the collection's history): a
  copy is **marked sold** (date, optional price) and leaves the collection,
  shown in a "Sold" filter. A mistake is fixed on the edit page. Wishes are
  not history: they can be deleted.
- **Fast entry for boosters**: one form adds every pull of an opening: set
  (or its printed code), language, date, then the numbers (`67 45r 12x2`).
- **A wish** is a TCGdex card wanted, with the language and variant wanted
  (or any, within its catalogue), a maximum price, a priority (1 = most
  wanted) and a note. Adding a copy that fulfils a wish offers to mark it
  got.
- **Full art** is a flag on the copy, pre-filled from the rarity (TCGdex has
  no "full art" field, see 3.2), editable.
- **Prices**: weekly, for the cards owned or wished, history kept like
  `card_price`. No language factor: the international catalogue has one
  price for all its languages, the Japanese one its own.
- **No cost tracking**: boosters are not counted, so there is no gain/loss;
  the pages show the market value, and what was paid for singles when noted.

## 1. TCGdex

All calls in `backend/src/store/pokemon/tcgdex.rs`, through one client with
a throttle (100 ms between calls; no published limit, be polite). Shapes
reshaped in `domain::pokemon` (pure, unit-tested on small synthetic JSON).

| What | Call | Remembered in |
|---|---|---|
| an international card | `GET /v2/{fr,en}/cards/{id}` | `pokemon_card` (asked again after 30 days) |
| a Japanese card | `GET /v2/ja/cards/{id}`, then its species' names: `GET /v2/{fr,en}/cards?dexId=eq:{n}` (first answer) | `pokemon_card` |
| search by name | `GET /v2/{fr,en,ja}/cards?name={text}` | not stored (the finder's results) |
| sets: list, cards, printed codes (`FNE`, `SFA` → `sv06.5`; Japanese: the id is the code, `SV6a`) | `GET /v2/{lang}/sets`, `GET /v2/{lang}/sets/{id}` (`abbreviation.official`/`localized`) | memory, until a restart (like `SET_CACHE`) |
| its picture | `{image}/high.webp` (some Japanese cards have none) | `media/pokemon_card_images/{id}-{lang}.webp` |
| prices, weekly | the card call again: `variants_detailed[].pricing.cardmarket`, else `pricing.cardmarket` | `pokemon_price` |

Network calls happen **before** a write transaction (CLAUDE.md rule). Tests
never call TCGdex: `TestApp` pre-inserts `pokemon_card` rows.

## 2. Migration `0012_pokemon.sql`

```sql
-- TCGdex's cards: those owned or wished (a cache, not the whole catalogue)
CREATE TABLE pokemon_card (
    id           TEXT PRIMARY KEY,       -- TCGdex id: 'sv06.5-067', 'SV6a-010'
    catalogue    TEXT NOT NULL CHECK (catalogue IN ('intl', 'ja')),
    set_id       TEXT NOT NULL,          -- 'sv06.5', 'SV6a'
    local_id     TEXT NOT NULL,          -- '067' (may hold letters: 'TG05', 'SV107')
    name_fr      TEXT,                   -- Japanese card: its species' names
    name_en      TEXT,
    name_ja      TEXT,                   -- Japanese card only
    name_manual  TEXT,                   -- typed by hand (Japanese Trainer)
    set_name     TEXT NOT NULL,          -- French when known, else English/Japanese
    rarity       TEXT,                   -- TCGdex's label
    category     TEXT NOT NULL CHECK (category IN ('Pokemon', 'Trainer', 'Energy')),
    types        TEXT,                   -- English, comma separated: 'Water'
    dex_id       INTEGER,
    image        TEXT,                   -- TCGdex base URL, without quality/extension
    json         TEXT NOT NULL,          -- TCGdex's answer (French, else English; Japanese)
    fetched_at   TEXT NOT NULL           -- YYYY-MM-DD
);

CREATE TABLE pokemon_copy (
    id           INTEGER PRIMARY KEY,
    card_id      TEXT NOT NULL REFERENCES pokemon_card(id) ON DELETE RESTRICT,
    language     TEXT NOT NULL,          -- 'fr', 'en', 'ja', …
    variant      TEXT NOT NULL CHECK (variant IN
                     ('normal', 'holo', 'reverse', 'firstEdition', 'wPromo')),
    condition    TEXT CHECK (condition IN ('MT', 'NM', 'EX', 'G', 'P')),
    full_art     INTEGER NOT NULL DEFAULT 0 CHECK (full_art IN (0, 1)),
    source       TEXT NOT NULL CHECK (source IN
                     ('booster', 'single', 'gift', 'trade', 'other')),
    obtained_on  TEXT,                   -- YYYY-MM-DD, NULL: unknown (old cards)
    price        TEXT,                   -- Money: what a single cost, optional
    grader       TEXT,                   -- 'PSA', 'CGC', 'BGS', 'PCA'…; NULL: raw
    grade        TEXT,                   -- '10', '9.5'
    cert_number  TEXT,
    estimate     TEXT,                   -- Money, by hand: a graded copy's value
    sold_on      TEXT,
    sold_price   TEXT,
    note         TEXT,
    CHECK (grade IS NULL OR grader IS NOT NULL),
    CHECK (sold_price IS NULL OR sold_on IS NOT NULL)
);

CREATE TABLE pokemon_wish (
    id          INTEGER PRIMARY KEY,
    card_id     TEXT NOT NULL REFERENCES pokemon_card(id) ON DELETE RESTRICT,
    language    TEXT,                    -- NULL: any (within the card's catalogue)
    variant     TEXT,                    -- NULL: any
    max_price   TEXT,                    -- Money, NULL: no limit
    priority    INTEGER NOT NULL DEFAULT 2 CHECK (priority BETWEEN 1 AND 3),
    note        TEXT,
    added_on    TEXT NOT NULL,
    got_copy_id INTEGER REFERENCES pokemon_copy(id) ON DELETE SET NULL
                                         -- NULL: still wanted
);

-- Cardmarket prices from TCGdex, weekly, never pruned
CREATE TABLE pokemon_price (
    card_id  TEXT NOT NULL REFERENCES pokemon_card(id) ON DELETE CASCADE,
    date     TEXT NOT NULL,
    json     TEXT NOT NULL,              -- the `cardmarket` objects, per variant
    PRIMARY KEY (card_id, date)
);
```

Copies are never deleted, so a `pokemon_card` row stays once owned; a row
only wished is deleted with its last wish (like book authors), with its
prices. The form checks a copy's language against the card's catalogue
(`ja` ⇔ `ja`).

## 3. `domain/src/pokemon/` (Claude)

### 3.1 `card_ref::parse`: what's printed on a card

Typed in the finder to reach a card without searching by name:

```rust
/// `"FNE 067/064"`, `"fne 67"`, `"SV6a 010/064"`, `"067/064"`, `"TG05/TG30"`.
pub struct CardRef { pub set_code: Option<String>, pub number: String }

pub fn parse(text: &str) -> Result<CardRef, CardRefError>;
```

Spaces and case don't matter; the set code is the leading word (letters,
digits allowed after the first letter: `SV6a`); `/total` is dropped; `67`
matches `067` (numbers compared as numbers when both are digits); a number
may have a letter prefix (`TG`, `GG`, `SV`). Without a code, the set chosen
in the finder is used.

### 3.2 `pulls::parse`: a booster opening's numbers

```rust
/// `"67 45r 12x2, TG05"`: numbers, `r` = reverse, `h` = holo, `x2` = two copies.
pub struct Pull { pub number: String, pub variant: Option<Variant>, pub count: u32 }

pub fn parse(text: &str) -> Result<Vec<Pull>, PullError>;
```

Without a letter, the variant is the card's only one, else `normal`
(`default_variant(&CardInfo)`). Every number is checked against the set's
cards before anything is saved: the form shows the unknown ones.

### 3.3 Card info, full art, prices, wishes

- `CardInfo::from_tcgdex(json)`: TCGdex JSON → the `pokemon_card` columns.
- `is_full_art(rarity) -> bool`: the suggestion. `Illustration rare`,
  `Special illustration rare`, `Hyper rare`, `Full Art Trainer`,
  `Shiny Ultra Rare`, `Mega Hyper Rare`, and `Ultra Rare` from Sword &
  Shield on (Japanese labels: `SR`, `SAR`, `AR`, `UR`, `CHR`, `CSR` when
  TCGdex gives them). Unknown → false.
- `price::of_copy(variant, &CardmarketPrices) -> Option<Decimal>`: Cardmarket's
  `-holo` fields are the **reverse holo** prices; a holo-only card
  (illustration rares) uses the plain fields. The trend, else the 30-day
  average (as for Yu-Gi-Oh!).
- `price::value(&Copy, Option<Decimal>) -> Option<Decimal>`: a graded copy's
  `estimate` when set, else the market price × the condition factor of
  `domain::valuation` (EX 0.8, G 0.6, P 0.4); sold copies count for nothing.
- `wish::verdict(&Wish, Option<Decimal>) -> Verdict` (`UnderMax`, `OverMax`,
  `NoMax`, `NoPrice`) and `wish::fulfils(&Wish, &Copy) -> bool` (same card,
  language and variant equal or "any").

## 4. Pages and routes (`routes/pokemon/`, `templates/pokemon/`)

- `/pokemon`: the collection. A **grid of pictures** by default and a table
  view (the shared "Data tables" convention). Filters: name, type, set,
  rarity, full art, language, graded, source, sold (off by default). Sort:
  set and number, price, date. Totals at the top: copies, market value.
- `/pokemon/:id`: a copy: big picture, every field, price history
  (Chart.js), sold or not.
- `/pokemon/add`: one card (singles, gifts, graded): find the card (printed
  code, name in French/English/Japanese, or set + number), pick it among
  the pictures, then the copy fields; variant and full art pre-filled.
- `/pokemon/add/booster`: an opening: set (or printed code), language, date,
  the pulls (3.2). Shows the cards found with their pictures, then saves
  them all (`source = booster`). Full art pre-filled per card.
- After saving, the copies that fulfil a wish are listed with "Mark got".
- `/pokemon/:id/edit` (also "Sold": date, price). No delete route.
- `/pokemon/wishlist`: the wishes still wanted, by priority: picture, card,
  wanted language/variant, max price, today's price, the verdict (green
  "under your price"). Got wishes in a folded section.
- `/pokemon/wishlist/add` (same card finder), `…/:id/edit`,
  `POST …/:id/delete`.
- `GET /pokemon/api/search?q=&lang=` and `GET /pokemon/api/sets[/:id]`:
  JSON for the card finder (`static/js/pokemon_finder.js`), login required.
- `/pokemon/card-image/:card_id/:lang`: the picture, downloaded once.
- Dashboard: not in the first version.

Writes in `write_router()`, reads in `router()`.

## 5. Price refresh

`refresh-prices` gains a third step, and the weekly task runs it: every
`pokemon_card` owned (not sold) or wished, one TCGdex call each, the
`cardmarket` objects stored in `pokemon_price` for today. `--dry-run` as for
the others.

## 6. Tests

- Domain: unit tests for `card_ref::parse`, `pulls::parse`,
  `CardInfo::from_tcgdex` (synthetic JSON, international and Japanese),
  `is_full_art`, `price::of_copy`/`value`, `wish::verdict`/`fulfils`.
- Handlers (`tests/pokemon.rs`, `TestApp`, `pokemon_card` rows pre-inserted):
  list and filters, add a single, add a booster opening (unknown number
  refused, nothing saved), sell, no delete route, a wish marked got, the
  wishlist verdicts, language vs catalogue, the login on writes.

## 7. Docs

docs/tour.md gains "The Pokémon cards" (like "The books") and a TCGdex table
like "Talking to ygoprodeck"; CLAUDE.md the routes, tables, folders;
recipes.md "Add a booster opening".
