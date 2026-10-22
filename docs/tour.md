# A tour of the code

This is the reading guide for the Rust version of the app. It assumes you
know the Django version (and the collection itself) well, and shows where
everything went. For step-by-step changes, see [recipes.md](recipes.md).

Suggested reading order, about an hour with the code open next to it:

1. [The big picture](#the-big-picture) (this file)
2. [Following a page request](#following-a-page-request): `backend/src/routes/sellers/mod.rs`
3. [Following a form](#following-a-form): `backend/src/routes/sellers/form.rs`, then `lots/form.rs::create`
4. [The database](#the-database): `migrations/0002_ygo_schema.sql`, `backend/src/store/db.rs`
5. [The domain crate](#the-domain-crate): `domain/src/lib.rs` and one module, e.g. `ydke.rs`
6. [The deck builder](#the-deck-builder): `frontend/src/App.svelte` and `routes/decks/api/`
7. [Talking to ygoprodeck](#talking-to-ygoprodeck): `backend/src/store/card_data/`
8. [Tests](#tests): `backend/src/tests/`
9. [The books](#the-books): `backend/src/routes/books/`, a second, smaller collection

## The big picture

```
                    browser
          ┌───────────┴─────────────┐
     HTML pages                 deck builder (Svelte app, static/deck-builder/)
          │                         │  JSON over /api/*
          ▼                         ▼
 ┌──────────────────── backend (axum) ─────────────────────┐
 │ main.rs   AppState, app(): routers + login layer        │
 │ routes/   one module per page group: SQL + handler +    │
 │           template struct                               │
 │ templates/  askama HTML (compiled into the binary)      │
 │ forms.rs, cards.rs, card_data/, auth.rs, db.rs …        │
 └───────┬───────────────────────────┬─────────────────────┘
         │ pure functions            │ sqlx
         ▼                           ▼
   domain/ (no DB, no HTTP)     data/app.db (SQLite, tracked in the data submodule)
   budget, ydke, monte carlo,        ▲
   shipment parsing, ban lists       │ migrations/ applied at startup
```

The Cargo workspace has three crates:

| Crate | What it is | Django equivalent |
|---|---|---|
| `backend/` | The web server **and** the maintenance commands (one binary) | the project + `views/`, `forms`, `rest_api/`, `management/commands/` |
| `domain/` | Pure business logic, unit-tested | model methods, `monte_carlos/`, `shipment_reader/` |
| `importer/` | One-off Django `dumpdata` loader, used for the cutover | `loaddata` |

Plus `frontend/` (the Svelte deck builder, built by npm, not Cargo) and
`migrations/` (SQL files shared by both binaries).

### What replaced what

| Django | Rust |
|---|---|
| `models/*.py` (schema) | `migrations/0002_ygo_schema.sql`, plain SQL |
| `models/*.py` (methods) | `domain/` for pure logic, SQL in `routes/` or `store/cards.rs` |
| ORM queries | raw SQL strings run by `sqlx` (`query_as`, `query_scalar`) |
| `urls.py` | `routes/mod.rs`: `router()` (reads) and `write_router()` (writes) |
| class-based views | `async fn` handlers in `routes/<area>/` (`mod.rs` pages, `form.rs` forms) |
| Django templates | askama templates in `backend/templates/<area>/`, nearly identical syntax |
| `forms.py` | `web/forms.rs` (`FormData`) + validation in each `routes/<area>/form.rs` |
| DRF API + serializers | `routes/decks/api/` + `#[derive(Serialize)]` structs |
| `signals.py` | explicit code in the handler (e.g. `delete_orphan_proxies`) |
| `manage.py <command>` | `cargo run -p backend -- <command>` (`commands/`) |
| admin | edit pages (`/cards/:id/edit`, …) |
| Django auth | `web/auth.rs`: one password (`APP_PASSWORD`), signed cookie |
| `runserver` | `cargo run -p backend` |

### Startup (`backend/src/main.rs`)

`main()`:

1. reads `.env` (`dotenvy`), sets up the logs (`tracing`, level from
   `RUST_LOG`, `info` by default), parses the command line (`commands::Command`);
2. opens the SQLite pool and runs pending migrations (`db::init_pool`);
3. builds `AppState`, the struct every handler receives: the DB pool, an
   HTTP client (for ygoprodeck), the media/uploads directories, your
   Cardmarket account name, and the login settings;
4. either runs a maintenance command, or serves `app(state)` until Ctrl-C;
5. closes the pool, which folds the SQLite WAL back into `data/app.db`.

`app()` is the whole URL map: the read router, the write router behind
the login middleware, `/login`, and the static/media file servers. Its
`TraceLayer` logs one line per page or API call; it sits before the file
routes (card images, `/static`, `/media`), which are therefore not logged,
because a `.layer()` only wraps the routes added before it.

Changes to the collection are logged by the handlers themselves, after the
commit, with named fields: `tracing::info!(lot_id, cards = n, "lot created")`.

## Following a page request

`GET /sellers?page=2`, in `backend/src/routes/sellers/mod.rs`:

```rust
pub async fn list(
    State(state): State<AppState>,      // the shared AppState
    Query(param): Query<PageParam>,     // ?page=2, parsed by serde
) -> AppResult<SellersList> {           // a template struct, or an error
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM seller")
        .fetch_one(&state.pool)
        .await?;                        // `?`: a DB error becomes a 500
    …
    let sellers = sqlx::query_as(…).bind(PAGE_SIZE).bind(page.offset())
        .fetch_all(pool).await?;        // rows → Vec<SellerRow> via #[derive(FromRow)]
    Ok(SellersList { page_title: "All Sellers", page, total, sellers })
}
```

Things to notice:

- **Extractors**: a handler's arguments say what it needs from the
  request: `State`, `Path(id)` for `/sellers/:id`, `Query`, `Form`, `Json`,
  `Multipart`. axum fills them in, and rejects the request with a 4xx if
  one can't be parsed (e.g. `/sellers/abc`).
- **The response type is the template**. `SellersList` has
  `#[derive(Template)] #[template(path = "sellers/list.html")]`; returning
  it renders the HTML. The template's variables are the struct's fields, and
  it can call the struct's methods.
- **Templates are compiled.** askama turns each template into Rust code at
  build time, so a typo in a variable name is a compile error, not a blank
  on the page. The price: `cargo build` after each template edit (there is
  no hot reload of templates).
- **Row structs** (`SellerRow`) are filled by name from the SQL columns
  (`#[derive(sqlx::FromRow)]`). Rows used by several pages live in
  `store/models.rs` (`CardRow`, `LotRow`, `DeckVersionRow`), with their `SELECT`
  as a constant next to them.
- **Errors** (`web/error.rs`): handlers return `AppResult<T>`. `?` turns a
  `sqlx::Error` into a 500 (logged), `.or_404()` turns a missing row
  (`Option::None`) into a 404, `bad_request("…")` gives a 400 with a JSON
  `{"error": …}` for the API.
- **Pagination** (`web/pagination.rs`): `Page::new(requested, count, querystring)`
  and `page.offset()`; `partials/pagination.html` renders the links. Filter pages
  (`/cards`, `/lots`) keep their filters in the page links with
  `querystring(…)`.
- **Template inheritance** works like Django's: `base.html` → `list_base.html`
  (blocks `items`, `filter`, `add_button`) → `sellers/list.html`. Partials
  live in `partials/` and are pulled in with `{% include %}`. Filters such as
  `|money` or `|yes_no` are plain functions in `web/filters.rs`.

## Following a form

`POST /sellers/:id/edit`, in `backend/src/routes/sellers/form.rs`:

1. The body arrives as raw `(name, value)` pairs (`Form<Pairs>`) and is
   wrapped in `FormData`, which collects validation errors as it reads:
   `form.required("name", "Name")`, `form.id(…)`, `form.decimal(…)`,
   `form.date(…)` each return `Option<value>` and push a message to
   `form.errors` when invalid.
2. Extra checks (e.g. "name already taken") push to `form.errors` too.
3. If anything is wrong, the same page struct is rebuilt with the values
   **as typed** (`form.text(…)`) and the errors, and rendered again. That's
   Django's "form invalid → re-render" pattern.
4. Otherwise: `begin_write(&pool)` opens a write transaction, the
   `UPDATE`/`INSERT`s run on it, `tx.commit()`, then `Redirect::to(…)`.

`lots/form.rs::create` is the same pattern at full size:

- a lot plus repeated card rows (`cards-0-name`, `cards-0-code`, …, read by
  `form.card_rows()` / `form.new_cards()`);
- an existing seller **or** a new one created on the fly (`SellerChoice`);
- `cards::insert_lot_cards` creates each card `quantity` times with its
  `unite`;
- an optional shipment PDF (see below) moved into
  `media/shipment_files/` right before the commit, and moved back if the
  commit fails;
- after the commit, `spawn_link_cards` starts a **background task** that
  links the new cards to their ygoprodeck `card_data` (fetching unknown
  names from the API), puts their names in the language of their code, then
  swaps deck proxies for the new owned copies. The redirect doesn't wait
  for it (see [Talking to ygoprodeck](#talking-to-ygoprodeck)).

### The shipment PDF flow

`POST /lots/add/shipment` (`lot_form::upload`): the PDF is parsed **in
memory** (`domain::cardmarket::parse_shipment_pdf`), and the form is
rendered pre-filled with the dates, total, seller and card lines. The file
waits in `uploads/pending-<token>.pdf` (deleted after 24 h), and the form
carries the token in a hidden `pending` field. On save, `create` moves it to
`media/shipment_files/shipment_<lot id>.pdf`. The file moves are in
`backend/src/store/shipment_files.rs`.

The PDFs contain names and addresses: their text is never stored, logged or
printed, only the parsed fields. Keep it that way (CLAUDE.md, "Privacy
rule").

## The database

**Schema.** `migrations/0002_ygo_schema.sql` is the Django schema written
by hand in SQL, with Django's constraints kept (UNIQUE, CHECK on choice
fields, ON DELETE). The ids are the Django ids. Tables you'll meet most:

```
card_data ◄── card ──► unite ──► lot ──► seller ──► seller_source
 (ygoprodeck)  │                (purchase/sale)
               │
               ├── deck_version_{main,extra,side}_deck ──► deck_version ──► deck
               └── card_category_assignment ──► card_category ──► deck_version
monthly_budget ◄── apport           advanced_ban_list ◄── ban_list_entry ──► card_data
```

Differences from the Django models to keep in mind:

- **Money is TEXT** (`"12.50"`), read and written as `db::Money` (a wrapper
  around `rust_decimal::Decimal`). Never sum money in SQL (SQLite would
  convert it to floats): load the rows and add in Rust.
- Dates are `YYYY-MM-DD` text, booleans are `0/1`.
- `CardConditionalCategory` is folded into `card_category`: a category is
  conditional when `condition_description IS NOT NULL`.
- Django's many-to-many fields are explicit join tables (`cards::Zone::table()`
  gives the one for a deck zone).

**Migrations.** `sqlx::migrate!` embeds `migrations/*.sql` into the binary
and applies the new ones at startup, recording a checksum of each. So an
applied migration must **never be edited** (startup would fail on the
checksum): changes are new files, `0004_….sql`, and so on.

**Writing safely** (`store/db.rs`): SQLite allows one writer at a time. Every
transaction that writes starts with `db::begin_write(&pool)` (not
`pool.begin()`), which takes the write lock up front so concurrent writers
wait instead of failing with "database is locked". Get new ids with
`.execute(…)?.last_insert_rowid()`. Do ygoprodeck calls *before* opening a
write transaction, because they write `card_data` through the pool themselves.

**The data lives in git.** `data/app.db` is tracked in the private `data`
submodule. Ctrl-C stops the server cleanly, and after that `app.db` alone
holds everything. See [recipes.md](recipes.md#save-your-data-to-git).

## The domain crate

`domain/` holds the logic worth testing on its own, with no database and no
HTTP. Each function gets plain values and returns plain values:

| Module | What |
|---|---|
| `card` | name normalisation (`normalize_card_name`, `clean_card_name`), `CardStatus` |
| `lot` | `LotType`, shipping cost |
| `budget` | monthly recap (`recap`, `BudgetTotals`, `available_debt`) for `/budget` and the dashboard |
| `ydke` | `YdkeDeck::parse` / `to_url` |
| `monte_carlo` | opening-hand simulation, conditional categories included |
| `cardmarket` | shipment text → `Shipment` (dates, total, seller, card lines) |
| `ban_list` | Konami's ban-list JSON and US pages → entries |
| `cardmarket_catalog` | Cardmarket's price guide files, set code → expansion, copy → product |
| `valuation` | market value of the copies (condition/language factors), ownership by date, gain from lot prices, price moves |

Each module's doc comment (`//!` at the top) says what it ports and lists
the **deliberate differences from Django** (bugs fixed, crashes turned into
errors). That is the place to look when the Rust version behaves
differently from what you remember. Unit tests sit at the bottom of each
file (`#[cfg(test)] mod tests`).

## The deck builder

A Svelte 4 app in `frontend/`, built by Vite into `static/deck-builder/`
(`bundle.js`, `bundle.css`). The page `/deck-builder/:id`
(`templates/decks/builder.html`) is just a `<div id="app">` that loads the
bundle. The app then only talks JSON to `/api/*`:

| Frontend | Backend |
|---|---|
| `src/main.js`: mounts `App` with the version id | |
| `src/App.svelte`: state of the zones, drag & drop, save | `decks/api/version.rs`: load, save, delete, copy |
| `src/api.js`: one function per endpoint | `routes/decks/api/`: one handler per endpoint, one file per group (`ApiCard` in `mod.rs`) |
| `src/deck.js`: pure helpers (sorting, prices, YDKE URL) | |
| `src/lib/*.svelte`: zone, card tile, card overview, reserve search, YDKE toolbar, version panel, simulation, tournament; each with its CSS | `reserve.rs`, `proxies.rs`, `categories.rs`, `simulation.rs`, `tournament.rs`, `ydke_import.rs` |

Moving cards between zones only changes the app's local state until
**Save** (`PATCH /api/deck-versions/:id` with the three zones). Proxies,
categories and the tournament are saved immediately. Errors come back as
`{"error": "…"}` with a 4xx status, and `api.js` turns them into exceptions
the app shows to you.

During frontend work, `npm run dev` serves the app with hot reload on
`:5173` and forwards `/api` to the backend on `:3000`.

## Built decks

A deck version can be **built** (`deck_version.is_built`, migration 0010):
the physical deck exists, so its real (non-proxy) cards are **locked**, out
of the collection for every other version. In this app a proxy already means
"a card I don't have for this deck", so nothing else is needed to see what is
missing.

- **Building** (`PATCH /api/deck-versions/:id` with `{"is_built": true}`,
  `decks/api/version.rs`): `domain::built_deck::plan_build` says which cards
  change; each of the version's cards shared with other versions becomes a
  proxy there (`store::cards::proxy_copy` + `swap_card`, categories moved),
  except the cards another built version already holds: first built, first
  served, those become proxies here. The answer carries a `build_report`
  the deck builder shows.
- **Unbuilding** only clears the flag. The proxies made in other versions
  stay; `clean-cards` puts the owned cards back (`replace_proxies`), in
  those versions and in the unbuilt one.
- **Locked cards** (`store::cards::locked_elsewhere(version_id)`, a subquery)
  are skipped by the reserve, the YDKE import and `clean-cards`; saving zones
  with one is refused with a 400 naming the card and the built deck (only a
  stale page can send one). A copy of a built version gets proxies for them.
- **A built version's proxies are swapped by hand only.** The automatic
  swap (`replace_proxies`, after a new lot and in `clean-cards`) skips built
  versions: the physical deck changes only when you decide. In the deck
  builder, a selected proxy has a "Use my copy" button (`POST
  /api/deck-versions/:id/proxies/:card_id/replace`, `decks/api/proxies.rs`):
  it takes a `store::cards::free_copy` (a copy in no deck first), and if that
  copy was in other versions, they get a proxy of it, as building does.
- Each version can note its **deck box** and **sleeves** (free text).
- `/decks/built` (`routes/decks/built.rs`): each built version, what it still
  misses (its proxies), and the shopping list of all of them.

## The advent calendar

`/advent-calendar/:year` (`routes/advent/`, `templates/advent/calendar.html`,
`static/js/advent_calendar.js`, `static/css/advent.css`): 24 doors that open
on the photo and the card of the day. The tables `advent_calendar` (one per
year) and `advent_calendar_entry` (day, card, photo) come from Django, which
had no page for them. The photos are in `media/advent_calendar_photos/<year>/`
(upright, 1600 px, metadata stripped; the full-size originals in
`originals/`). They show handwritten notes, so the page and the photos are
behind the login (`routes::private_router`, `private_files` in `main.rs`).

Calendars are listed at `/advent-calendar`, added at `/advent-calendar/add`
like the other lists, and filled on their edit page (`routes/advent/form.rs`): for each day, a card of the collection (by code
or `#id`) and a photo. `store/photos.rs` makes the copy shown on the page
(upright from the EXIF orientation, 1600 px, re-encoded without metadata)
and keeps the original in `originals/`; ↺ ↻ turn a photo that came out
sideways. Each save writes new file names, so browsers never show an old
photo, and deletes the previous files (`store/calendar_photos.rs`, shared
with the book calendars). A calendar can have its own lot
(`advent_calendar.lot_id`, migration 0006, "Create its lot" on the edit
page): its cards are added on the usual lot page, then suggested in the
day fields; the lot page links back to the calendar.

## The books

A second collection in the same app and database, with nothing in common
with the cards: the novels at home. Every route starts with `/books`
(`routes/books/`, `templates/books/`); the book templates swap the
navigation bar (`{% block nav %}` → `partials/books_nav.html`) and the page
background (`body.books`, `static/css/books.css`). No prices, lots or
pictures.

| Piece | Where |
|---|---|
| tables (`book`, `book_author`, `book_book_author`, `book_series`, `book_data`, `book_advent_calendar[_entry]`) | `migrations/0007_books.sql` |
| ISBN parsing, check digits, ISBN-10 → 13 | `domain/src/isbn.rs` |
| lookup answers → `BookInfo`, series text → name + volume, author sort names, missing volumes | `domain/src/book.rs` |
| writes: a book with its authors (in order) and series, created on the way; orphans deleted | `store/books.rs` |
| the lookup: Open Library (`jscmd=data` + `details`), Google Books when it lacks something; cached in `book_data` | `store/book_data.rs` |
| list (filters, sort by title / surname / added), detail | `routes/books/mod.rs` |
| add, edit, delete (refused while behind an advent door) | `routes/books/form.rs` |
| `GET /books/api/isbn/:isbn`, the lookup as JSON | `routes/books/api.rs` |
| authors, series (with the missing volumes) | `routes/books/authors.rs`, `series.rs` |
| the book advent calendars (a twin of the card ones, own tables and photo folder) | `routes/books/advent/` |

The add form (`static/js/book_form.js`): "Look up" (or Enter, which USB
barcode scanners send) calls the API and fills the fields; "Scan" opens the
phone's camera through a file input and reads the barcode **in the
browser**: `BarcodeDetector` where the browser has it (HTTPS only), else the
ZXing library loaded from jsDelivr, trying the photo as taken and turned a
quarter turn. Plain HTTP on the home network can't use the live camera
(browsers keep it to HTTPS), hence the photo.

Like ygoprodeck: the lookup happens **before** any write transaction, and
tests never call it (they insert `book_data` rows).

## Login

`web/auth.rs`: with `APP_PASSWORD` set, everything in `write_router()` and the
shipment PDFs require a session cookie. Pages redirect to
`/login?next=…`, the APIs (`/api/*`, `/books/api/*`) answer 401. Reading stays public. The cookie is
an expiry date signed with a key stored in the `app_secret` table, so a
restart doesn't log you out. Without `APP_PASSWORD` there is no login, and
the server refuses to listen anywhere but `127.0.0.1`.

## Maintenance commands

`backend/src/commands/`: `scrap-ban-list` (Konami ban lists → `advanced_ban_list`:
the EU site's JSON, then the US site's pages for the dates still missing, a
fallback for when the EU JSON lags behind; the US index only links the current
list, so `--date 2026-10-19` fetches one not linked yet, and a US page whose table
is still the previous list's is skipped)
and `clean-cards` (normalise names, relink card data, swap proxies for owned
copies, delete unused proxies, rarity from `(V.x - …)` names, names in the
language of their code, rarity from the card data when the code was printed in
one rarity only). Both accept `--dry-run`: they run in a transaction
that is rolled back.

`refresh-prices` fetches today's market price of every card data
(`store/prices/ygoprodeck.rs`, batches of 100 ids) into `card_price`, then
Cardmarket's price guide (`store/prices/cardmarket.rs`, links and prices); the server
does it weekly by itself (`prices::spawn_weekly`: checked every 6 hours, runs
when the last refresh is 7 days old).

You rarely need `clean-cards` by hand: the server runs it in the background
5 minutes after it starts, then once a day (`clean_cards::spawn_daily`, one
summary line in the log), and the actions that make a cleanup necessary do
their part themselves (saving deck zones deletes unused proxies, saving new
cards links, translates and swaps proxies).

## Talking to ygoprodeck

All the calls to the [ygoprodeck API](https://ygoprodeck.com/api-guide/)
are in `backend/src/store/card_data/` (the client in `mod.rs`, then
`translations.rs`, `sets.rs`, `linking.rs`) and `store/prices/`. It allows about 20 requests per
second and blocks the IP for an hour above that: `throttle()` keeps every
call of the server 60 ms apart, and what can be remembered is.

| What | Call | Remembered in |
|---|---|---|
| a card's data, by English name or ygopro id | `cardinfo.php?name=` / `?id=` | `card_data` (with the whole JSON in `json_data`) |
| its picture | the URL in `json_data` | `media/card_images/` (`/card-image/:id`) |
| a set, for the quick add (`GET /api/sets/:code`) | `cardsets.php`, then `cardinfo.php?cardset=` in English and French for each set with that code | memory (`SET_CACHE`, until a restart); its French names also go to `card_translation` |
| a name in another language | `cardinfo.php?id=&language=fr` | `card_translation` (migration 0005): a name found is kept, "no translation yet" is asked again after 7 days |
| a set code's card names, for its Cardmarket expansion (`store/prices/cardmarket.rs`) | `cardsets.php`, then `cardinfo.php?cardset=` per set with that code | `cardmarket_expansion` (once; not found: again after 30 days) |
| market prices, weekly (`store/prices/ygoprodeck.rs`) | `cardinfo.php?id=1,2,…` (100 ids a request; unknown ids are left out, all unknown = 400) | `card_price` (migration 0008), one row per card and refresh date, never pruned |

Two rules: call ygoprodeck **before** opening a write transaction (the calls
write through the pool), and only through these functions. Tests never call
it: `TestApp` pre-inserts the card data they need (and translations, in
`card_translation`). The shapes ygoprodeck answers are reshaped in pure
functions in `domain/` (`card_set`, `rarity::Rarity::from_label` for its
rarity names), which are unit-tested on small synthetic JSON.

## The collection value

`/collection/value` (`routes/collection/mod.rs`), a tile on the dashboard,
and a "Market" line on card pages. Two price sources, both refreshed weekly
by `refresh-prices`:

- **Cardmarket's price guide** (`store/prices/cardmarket.rs`,
  `domain::cardmarket_catalog`): two public JSON files (products, prices; ~30
  MB) downloaded into memory, a price **per printing**. Only what the
  collection needs is kept (migration 0009): the products named like our
  card data, each set code's expansion, the price history of the linked
  products. The files are never saved. They have no set code and no rarity,
  so each copy is linked to its product by a guess: the set code's
  expansion is the one holding most of the set's card names (ygoprodeck),
  twins (OCG version, reprint) told apart by the date they were added; then
  the product with the card's name there, by rarity rank against price rank
  when there are several. About 80% of the copies get one; the rest, or a
  wrong guess, are set on the card's edit page ("Cardmarket product",
  `card.cardmarket_product_manual = 1`, kept by the refreshes). Its price
  (the trend, else the 30-day average) covers every language and edition.
- **ygoprodeck** (`card_price`): one price per card name, the cheapest
  printing's, an estimate usually too low for old or high rarity copies.
  Used for the copies without a product. The `card_prices` copy inside
  `card_data.json_data` is never used: it dates from when the card was
  linked.

Cardmarket's API takes no new users and scraping its site is against its
terms; the price guide files are its open download.

`domain::valuation` does the sums:
- a copy's price: its product's (`Prices::of`), else its card name's;
- a copy's value: price × condition (MT/NM/unknown 1, EX 0.8, G 0.6, P 0.4)
  × language (non-English 0.9), totals rounded to the cent;
- a copy counts on date D when its purchase lot (not cancelled) is dated on
  or before D and it has no sale lot by D; proxies never count;
- the **gain/loss uses the lot prices** (`lot.price`, shipping included):
  value of the copies owned + sale lots − purchase lots, up to D. Unite
  prices can't do it: many lots were bought as a whole, without a price per
  card;
- beside it, the copies that do have a unite price are compared card by
  card (value against that price, no shipping share);
- each purchase lot against its **full price** (`lot_gain`): the value of
  its cards still owned, plus what its sold cards brought (their price in
  the sale lot), minus `lot.price` (non-card items included); the lots
  without cards (sleeves, boxes) are left out;
- price moves: since the previous refresh and over 3 months, cards owned
  now, moves under 0.50 € left out (one line per price, the same source on
  both dates).

The lists show estimates too: each card on `/cards` and on a lot's page
(`CardRow::estimate`, from the `valuation::MARKET_PRICE` SQL expression:
the card's latest price, its product's else its name's), and each purchase
lot the value of its cards still owned (`fill_lot_estimates`).

The page loads every row (`store::valuation::owned_copies`, `lots`,
`prices_by_date`) and computes in Rust: money is never summed in SQL. The
chart is `static/js/collection_value.js`; the card page's price line is an
SVG drawn by the server (`cards::sparkline`).

## Tests

- `domain/`: plain unit tests on values.
- `backend/src/tests/` (one file per area, helpers in `mod.rs`): `TestApp` creates a migrated SQLite DB in a
  temporary directory (with two fake `card_data` rows, so nothing calls
  ygoprodeck) and sends real HTTP requests to `app()`. Each test reads like
  a scenario: POST a form, check the redirect, check the rows with
  `app.scalar("SELECT …")`.
- `domain/tests/real_shipments.rs` (ignored by default): checks the PDF
  parser against your real shipments and their lots in `data/app.db`.

`cargo test --workspace` runs everything but the last one. Tests never
touch `data/app.db`, apart from that ignored test, which opens it
read-only.

`cargo cov` (an alias in `.cargo/config.toml`, for `cargo-llvm-cov`) runs
the same tests and writes a coverage report to
`target/llvm-cov/html/index.html`: open it to see, line by line, the code
no test runs.
