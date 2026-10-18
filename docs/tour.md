# A tour of the code

This is the reading guide for the Rust version of the app. It assumes you
know the Django version (and the collection itself) well, and shows where
everything went. For step-by-step changes, see [recipes.md](recipes.md).

Suggested reading order, about an hour with the code open next to it:

1. [The big picture](#the-big-picture) (this file)
2. [Following a page request](#following-a-page-request): `backend/src/routes/sellers.rs`
3. [Following a form](#following-a-form): `backend/src/routes/seller_form.rs`, then `lot_form.rs::create`
4. [The database](#the-database): `migrations/0002_ygo_schema.sql`, `backend/src/db.rs`
5. [The domain crate](#the-domain-crate): `domain/src/lib.rs` and one module, e.g. `ydke.rs`
6. [The deck builder](#the-deck-builder): `frontend/src/App.svelte` and `routes/deck_api/`
7. [Tests](#tests): `backend/src/tests.rs`

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
 │ forms.rs, cards.rs, card_data.rs, auth.rs, db.rs …      │
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
| `models/*.py` (methods) | `domain/` for pure logic, SQL in `routes/` or `cards.rs` |
| ORM queries | raw SQL strings run by `sqlx` (`query_as`, `query_scalar`) |
| `urls.py` | `routes/mod.rs`: `router()` (reads) and `write_router()` (writes) |
| class-based views | `async fn` handlers in `routes/<domain>.rs` |
| Django templates | askama templates in `backend/templates/`, nearly identical syntax |
| `forms.py` | `forms.rs` (`FormData`) + validation in each `*_form.rs` |
| DRF API + serializers | `routes/deck_api/` + `#[derive(Serialize)]` structs |
| `signals.py` | explicit code in the handler (e.g. `delete_orphan_proxies`) |
| `manage.py <command>` | `cargo run -p backend -- <command>` (`commands/`) |
| admin | edit pages (`/cards/:id/edit`, …) |
| Django auth | `auth.rs`: one password (`APP_PASSWORD`), signed cookie |
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

`GET /sellers?page=2`, in `backend/src/routes/sellers.rs`:

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
  `#[derive(Template)] #[template(path = "sellers_list.html")]`; returning
  it renders the HTML. The template's variables are the struct's fields, and
  it can call the struct's methods.
- **Templates are compiled.** askama turns each template into Rust code at
  build time, so a typo in a variable name is a compile error, not a blank
  on the page. The price: `cargo build` after each template edit (there is
  no hot reload of templates).
- **Row structs** (`SellerRow`) are filled by name from the SQL columns
  (`#[derive(sqlx::FromRow)]`). Rows used by several pages live in
  `models.rs` (`CardRow`, `LotRow`, `DeckVersionRow`), with their `SELECT`
  as a constant next to them.
- **Errors** (`error.rs`): handlers return `AppResult<T>`. `?` turns a
  `sqlx::Error` into a 500 (logged), `.or_404()` turns a missing row
  (`Option::None`) into a 404, `bad_request("…")` gives a 400 with a JSON
  `{"error": …}` for the API.
- **Pagination** (`pagination.rs`): `Page::new(requested, count, querystring)`
  and `page.offset()`; `_pagination.html` renders the links. Filter pages
  (`/cards`, `/lots`) keep their filters in the page links with
  `querystring(…)`.
- **Template inheritance** works like Django's: `base.html` → `list_base.html`
  (blocks `items`, `filter`, `add_button`) → `sellers_list.html`. Partials
  start with `_` and are pulled in with `{% include %}`. Filters such as
  `|money` or `|yes_no` are plain functions in `filters.rs`.

## Following a form

`POST /sellers/:id/edit`, in `backend/src/routes/seller_form.rs`:

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

`lot_form.rs::create` is the same pattern at full size:

- a lot plus repeated card rows (`cards-0-name`, `cards-0-code`, …, read by
  `form.card_rows()` / `form.new_cards()`);
- an existing seller **or** a new one created on the fly;
- `cards::insert_lot_cards` creates each card `quantity` times with its
  `unite`;
- an optional shipment PDF (see below) moved into
  `media/shipment_files/` right before the commit, and moved back if the
  commit fails;
- after the commit, `spawn_link_cards` starts a **background task** that
  links the new cards to their ygoprodeck `card_data` (fetching unknown
  names from the API). The redirect doesn't wait for it.

### The shipment PDF flow

`POST /lots/add/shipment` (`lot_form::upload`): the PDF is parsed **in
memory** (`domain::cardmarket::parse_shipment_pdf`), and the form is
rendered pre-filled with the dates, total, seller and card lines. The file
waits in `uploads/pending-<token>.pdf` (deleted after 24 h), and the form
carries the token in a hidden `pending` field. On save, `create` moves it to
`media/shipment_files/shipment_<lot id>.pdf`.

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

**Writing safely** (`db.rs`): SQLite allows one writer at a time. Every
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
| `ban_list` | Konami's ban-list JSON → entries |

Each module's doc comment (`//!` at the top) says what it ports and lists
the **deliberate differences from Django** (bugs fixed, crashes turned into
errors). That is the place to look when the Rust version behaves
differently from what you remember. Unit tests sit at the bottom of each
file (`#[cfg(test)] mod tests`).

## The deck builder

A Svelte 4 app in `frontend/`, built by Vite into `static/deck-builder/`
(`bundle.js`, `bundle.css`). The page `/deck-builder/:id`
(`templates/deck_builder.html`) is just a `<div id="app">` that loads the
bundle. The app then only talks JSON to `/api/*`:

| Frontend | Backend |
|---|---|
| `src/main.js`: mounts `App` with the version id | |
| `src/App.svelte`: state of the zones, drag & drop, save | `deck_api/version.rs`: load, save, delete, copy |
| `src/api.js`: one function per endpoint | `routes/deck_api/`: one handler per endpoint, one file per group (`ApiCard` in `mod.rs`) |
| `src/deck.js`: pure helpers (sorting, prices, YDKE URL) | |
| `src/lib/*.svelte`: zone, card tile, card overview, reserve search, YDKE toolbar, version panel, simulation, tournament; each with its CSS | `reserve.rs`, `proxies.rs`, `categories.rs`, `simulation.rs`, `tournament.rs`, `ydke_import.rs` |

Moving cards between zones only changes the app's local state until
**Save** (`PATCH /api/deck-versions/:id` with the three zones). Proxies,
categories and the tournament are saved immediately. Errors come back as
`{"error": "…"}` with a 4xx status, and `api.js` turns them into exceptions
the app shows to you.

During frontend work, `npm run dev` serves the app with hot reload on
`:5173` and forwards `/api` to the backend on `:3000`.

## Login

`auth.rs`: with `APP_PASSWORD` set, everything in `write_router()` and the
shipment PDFs require a session cookie. Pages redirect to
`/login?next=…`, the API answers 401. Reading stays public. The cookie is
an expiry date signed with a key stored in the `app_secret` table, so a
restart doesn't log you out. Without `APP_PASSWORD` there is no login, and
the server refuses to listen anywhere but `127.0.0.1`.

## Maintenance commands

`backend/src/commands/`: `scrap-ban-list` (Konami ban lists → `advanced_ban_list`)
and `clean-cards` (normalise names, relink card data, swap proxies for owned
copies, delete unused proxies). Both accept `--dry-run`: they run in a
transaction that is rolled back.

## Tests

- `domain/`: plain unit tests on values.
- `backend/src/tests.rs`: `TestApp` creates a migrated SQLite DB in a
  temporary directory (with two fake `card_data` rows, so nothing calls
  ygoprodeck) and sends real HTTP requests to `app()`. Each test reads like
  a scenario: POST a form, check the redirect, check the rows with
  `app.scalar("SELECT …")`.
- `domain/tests/real_shipments.rs` (ignored by default): checks the PDF
  parser against your real shipments and their lots in `data/app.db`.

`cargo test --workspace` runs everything but the last one. Tests never
touch `data/app.db`, apart from that ignored test, which opens it
read-only.
