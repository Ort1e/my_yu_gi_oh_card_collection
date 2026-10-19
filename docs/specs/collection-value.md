# Spec: collection value (market prices and their history)

What the owned collection is worth at market prices, compared with what it
cost, and how that value moves over time.

**Status: designed (2026-10-19), all decisions settled. Backend for the owner
(Claude reviews and suggests), frontend for Claude once the queries exist.

## The price source

ygoprodeck, which already gives `card_prices[0].cardmarket_price` (EUR, TEXT)
in every `card_data.json_data` row.

- **One price per card name, not per printing.** It follows the cheapest
  printing: Blue-Eyes White Dragon is €0.09 (2026-10-19), whatever printing you
  own. Printing prices (`card_sets[].set_price`) exist, but they are TCGplayer
  prices in USD and are often `"0"`. So the value is an **estimate**, which is
  usually too low for old or high rarity copies. The page must say so.
- **The copy in `json_data` is stale**: it dates from when the card was
  linked, at an unknown date. Don't use it. The history starts at the first
  refresh.
- **Cardmarket** is not an option: its API no longer accepts new users, and
  scraping it is against its terms (and blocked by Cloudflare).

Fetching: `cardinfo.php?id=<id>,<id>,…` accepts several ids (checked
2026-10-19). 1,035 card data rows → about 11 requests of 100 ids, through the
existing `throttle()` in `store/card_data.rs`. Not the full dump
(`cardinfo.php` without params, every card, tens of MB) for so few cards.

## 1. The table (next free migration: `0008` is taken by built decks, so `0009`)

```sql
-- Market price of one card (by name) on one refresh date.
CREATE TABLE card_price (
    card_data_id      INTEGER NOT NULL REFERENCES card_data(id) ON DELETE CASCADE,
    date              TEXT NOT NULL,   -- YYYY-MM-DD, the refresh day
    cardmarket_price  TEXT,            -- EUR, Money; NULL = ygoprodeck had none
    tcgplayer_price   TEXT,            -- USD, Money; kept for the history, not shown yet
    PRIMARY KEY (card_data_id, date)
);
```

- Prices only, no totals. The collection value on a given date is computed
  from these rows and the lot dates (section 4). Totals stored at refresh time
  would go stale when a lot is entered late or the discounts (section 3)
  change.
- Keyed by `card_data_id`, our own key (`ygopro_id` can be NULL). Rows of
  card data without a `ygopro_id` are skipped.
- **Weekly** rows: prices don't move much day to day. That is ~54k rows a
  year (~1,000 cards × 52), small for SQLite. Rows are never pruned.
- A `"0"` price from ygoprodeck counts as no price (NULL).
- Both prices are stored, though only the Cardmarket one is used: past
  prices can't be fetched later, so the TCGplayer one is kept from the start.

## 2. The refresh (`store/card_prices.rs` + `commands/`)

- `refresh_prices(state) -> AppResult<Report>`: for every `card_data` with a
  `ygopro_id` and no row dated today, fetch in batches of 100, then insert in
  **one** write transaction opened **after** every request has finished (the
  `begin_write` rule). `INSERT OR REPLACE`, so running it twice in a day just
  rewrites the rows.
- Command `cargo run -p backend -- refresh-prices [--dry-run]`, like
  `clean-cards`.
- In the background: weekly, started like `clean_cards::spawn_daily`. It runs
  only if the latest `card_price.date` is at least 7 days old, so restarting
  the server doesn't refresh again. New card data (a new lot) gets its first
  price at the next refresh. Also fetching it when it is linked would be nice,
  not needed.
- Ids that ygoprodeck doesn't return (a removed or renamed card): log their
  number, not the cards, and insert nothing.

## 3. Valuing one copy (`domain::valuation`, pure, unit-tested)

```rust
/// Market value of one owned copy: card price × condition × language.
pub fn copy_value(price: Decimal, status: Option<CardStatus>, language: Language) -> Decimal
```

Factors (settled 2026-10-19), as `const`s in the module:

| | factor |
|---|---|
| MT, NM, unknown | 1.0 |
| EX | 0.8 |
| G | 0.6 |
| P | 0.4 |
| English | 1.0 |
| French (`Language::of_code`, `-FR` code) | 0.9 |

Round to the cent at the end (`round_dp(2)`), on the total, not per copy.

**Cost of a copy**: its `unite.price` in its purchase lot (not cancelled), plus
its share of the lot's `domain::lot::shipping_cost`, split evenly across the
lot's cards. `no_card_price` (boosters, sleeves…) is not part of what the
cards cost. Unpriced unites: cost unknown. They are left out of the gain/loss
and counted ("12 copies without a price").

## 4. Who is owned when

A copy counts on date D if it is not a proxy, it has a purchase lot (not
cancelled) with `buy_date ≤ D`, and no sale lot (not cancelled) with
`buy_date ≤ D`. Today, that is `is_proxy = 0 AND NOT SOLD_EXISTS`, plus the
date conditions.

The history: for every refresh date D,
`value(D) = Σ copy_value(price on D, …)` over the copies owned on D. For the
chart, also compute the cost of those same copies, so the gap is the
unrealised gain. A pure `domain` function that takes rows (copy: bought,
sold, status, language, card_data_id) and prices by date is simple to unit
test. The SQL only loads rows, and never `SUM()`s money.

## 5. The pages (frontend: Claude)

**`/collection/value`** (`routes/collection/mod.rs`,
`templates/collection/value.html`, read-only, in `routes::router()`):

- Top: value today, cost, gain/loss (€ and %), number of copies, date of the
  last refresh, and the estimate warning from section 1.
- **Chart**: value and cost over time (Chart.js, like the dashboard), one
  point per refresh.
- **Most valuable copies**: top 20 (card, code, condition, value, cost),
  linking to `/cards/:id`.
- **Biggest moves**: cards whose price moved most since the previous refresh
  and over 3 months (in % and €, with a € floor so €0.02 → €0.05 doesn't
  show up as +150%).
- **By deck**: value of each deck's latest version (its real cards; proxies
  separately, as "what's missing would cost").
- Later, maybe: by set or by rarity.

**Card detail** `/cards/:id`: current price and a small price-history line.

**Dashboard**: one tile (value, change over the last 4 weeks) linking to
the page.

## Tests

- `domain::valuation`: factors, rounding, cost sharing, ownership by date
  (bought after D, sold before D, cancelled lots, proxies).
- `TestApp`: the page with pre-inserted `card_price` rows, and no prices yet
  (empty state: "no price yet, the first refresh runs within a day").
- `refresh_prices`: building the requests and parsing a canned JSON answer.
  No network in tests: split the fetch from the parsing.
