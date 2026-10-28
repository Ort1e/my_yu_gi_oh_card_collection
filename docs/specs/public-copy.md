# Spec: a nicer public copy

Status: **decided, not built** (2026-10-28). The owner asked for the public copy
(`export-public`, on the OVH hosting) to look like the app, and to show
the Yu-Gi-Oh! collection's statistics; "all of them" for the statistics
proposed. Still public only: the cards and books owned, the Pokémon
wishlist, market values. Never lots, sellers, prices paid, gains, budget,
decks, tournaments, calendars, notes.

## Stays private (owner, 2026-10-28)

Nothing of these in any exported file, page or picture:

- **sellers** (names, sources) and **lots** (their ids, prices, shipping,
  refunds);
- **the shipment PDFs** and anything read from them;
- **prices paid**: lot prices, unit prices, the gain/loss, what was spent
  or sold (no "spent" line in the chart);
- **private information**: notes (cards, copies, wishes, books), the
  advent calendars and their photos, the budget, decks, tournaments, the
  wishes' maximum prices.

What the copy uses of a lot is only its **date** (a card's "added on", the
value over time), never the lot itself. The export test checks for the
sellers' names, the lot prices and the notes in every file.

## Today

`common::export::write_site` writes `index.html` and one page per section
(`yugioh`, `pokemon`, `wishlist`, `books`) with the app's stylesheets but
its own plain header ("My collection" + the sections). The Yu-Gi-Oh! page
is one grid of ~500 tiles (470 KB of HTML), tiles without links, no
search, no script.

## 1. The app's layout

- **Each collection with its own header**, as in the app: the Yu-Gi-Oh!
  bar (logo, menus), the Pokémon red header, the books' colours. Only the
  public pages in the menus; a switch between the three collections at the
  right, like the app's `section-switch` links.
  - Yu-Gi-Oh!: **Cards**, **Statistics**, **Rarities** (the rarity guide:
    static already, `static/js/rarities.js`).
  - Pokémon: **Cards**, **Wishlist**, **Rarities** (`pokemon_rarities.js`).
  - Books: **Books**, **Authors**, **Series** (with the missing volumes).
- **The list pages like `list_base.html`**: the title with its counts, the
  summary line, the grid, and **pages of 48 cards** (`yugioh.html`,
  `yugioh-2.html`…) with the app's pagination links, or one page with a
  filter script (see "To decide").
- **A table view** (`?view=table` in the app): a second static page per
  list (`yugioh-table.html`: name, code, rarity, condition, value), sortable
  with the app's `tables.js`, which also gives the search field.
- **A page per card** (`yugioh/<id>.html`): the picture large, its names,
  code, rarity, condition, the copies (× n), its market price and the price
  line of the last months (the app's `cards::sparkline` SVG). No lot, no
  price paid. Same for the Pokémon copies (variant, set, number).
- The shared templates (`base.html`, `list_base.html`, the nav partials)
  get an **export mode** rather than a copy: links relative (`yugioh.html`,
  not `/cards`), no login link, no app-only menu. `common::export` renders
  them with a `public: true` flag; the app keeps its pages as they are.

## 2. The card seen large on hover (app and copy)

Owner's addition. In every card grid (`partials/card_grid.html`: `/cards`,
`/pokemon`, a booster opening, the copy's pages), with a mouse, hovering a
card's picture shows it **large beside it** (about 80 % of the window's
height, up to 560 px), on the side with room, until the mouse leaves.
Phones and tablets (no hover) are unchanged.

- `static/js/card_preview.js` (+ `.card-preview` in `tiles.css`): one
  floating picture, a short delay so moving across the grid doesn't
  flicker, the card's proportions from its tile (Yu-Gi-Oh! 59 × 86,
  Pokémon 63 × 88), hidden on scroll.
- The app shows the full picture (the tile's own). The copy has only small
  copies: they become **360 px wide** (instead of 240), one copy for the
  grid, the preview and the card pages. About 40 MB on the hosting (of 100;
  22 MB today).
- `CardTile::preview`: a larger picture than the tile's when there is one
  (none in the app).

## 3. The Yu-Gi-Oh! statistics (`yugioh-stats.html`)

All from the copies owned today (no proxy, not sold) and the market prices
(the same estimate as the cards' tiles: Cardmarket product, else
ygoprodeck, × condition × language).

| Block | What |
|---|---|
| Totals | copies, distinct cards, market value, the date of the prices |
| Value over time | a line chart (Chart.js from its CDN, like the dashboard): the value of the cards owned at each weekly price refresh. **No "spent" line** (that's the price paid). It shows when the collection grew, not what it cost. |
| By rarity | copies and value per rarity, in the guide's order, a bar chart beside the table |
| By kind | monsters (main deck), Extra Deck (Fusion, Synchro, Xyz, Link), spells, traps: from ygoprodeck's `type` |
| By set | the 15 sets with most copies (code prefix: `LOB`, `MAMO`), copies and value |
| By language and condition | FR/EN, MT/NM/EX/G/P/unknown: copies and value |
| The most valuable | the 12 cards of highest value, as tiles |
| Recent additions | the 12 cards added last (their lot's date), as tiles |
| Price moves | the biggest rises and falls over 3 months (`valuation::biggest_moves`, already on `/collection/value`) |

The same page in the app, behind the login, is not part of this: the app
has `/collection/value` already (with the private parts).

## Who writes what

Claude: the templates' export mode, the pages and their files, pagination,
the per-card pages, the chart data, the stylesheets, the tests (that no
private data gets in: the existing `backend/src/tests/export.rs` extended
to the new pages; each page written).

Claude too, the owner decided (`yugioh-domain/src/stats.rs`):

- `kind(card_type: &str) -> CardKind`: ygoprodeck's `type` ("XYZ Monster",
  "Pendulum Effect Monster", "Spell Card"…) → Monster / Extra Deck / Spell
  / Trap;
- `breakdown<K>(copies, key: impl Fn(&Copy) -> K) -> Vec<Group<K>>`:
  copies and value per key, sorted by value;
- `set_prefix(code: &str) -> Option<&str>` (`MAMO-FR023` → `MAMO`), maybe
  from `card_set::code_without_language`.

## Decided (owner, 2026-10-28)

1. **Pages of 48 cards** like the app, and the table view (one sortable
   page with `tables.js`'s search) for finding a card.
2. **A page per card** (Yu-Gi-Oh! and Pokémon copies): tiles become links.
3. Statistics for the Pokémon cards and the books: **later**.
4. The stats functions (`kind`, `breakdown`, `set_prefix`): **Claude's**
   (simple grouping).

## Tests

- Export: every page written, the links between them all resolve (a test
  walks the written folder), nothing private in any file (seller names,
  lot prices, notes, deck names: the existing test's words).
- Domain: `kind`, `breakdown`, `set_prefix` (unit tests).
