# CLAUDE.md

Personal Yu-Gi-Oh! card collection manager: cards owned, purchase/sale lots, sellers,
monthly budget, decks (versions, categories, YDKE import/export, draw simulation), ban lists.

## Branch situation — read first

- `main` = the **Django** app (reference implementation, still the one in use).
- `rust-stack` (this branch) = **rewrite in Rust**. Goal: reach feature parity with Django, then replace it.
- The Django code is **not in this branch's tree**. Read it from `main` without switching branches:
  - `git show main:my_ygo_cards/models/deck.py`
  - `git ls-tree -r --name-only main`
  - or `git archive main my_ygo_cards | tar -x -C <scratch dir>` to browse it all.
- `README.md` is still the generic scaffold README ("myapp", wrong `src/` paths). Don't trust its layout section.

## Stack

| Layer | Tech |
|---|---|
| HTTP | axum 0.7, tower-http (ServeDir, trace) |
| DB | SQLite via sqlx 0.7 (runtime queries, `sqlx::migrate!`) |
| Simple pages | askama 0.12 + askama_axum templates, htmx 1.9 for fragments |
| Rich page | Svelte 4 + Vite 5 in `frontend/`, built to `static/complex/`, talks only to `/api/*` |
| Env | Nix flake (`flake.nix`, `.envrc` → `use flake`), Rust 1.88.0 pinned in `rust-toolchain.toml` |

Cargo workspace (`Cargo.toml`), shared deps under `[workspace.dependencies]`:
- `backend/` — the web server:
  - `main.rs` (`AppState`: pool, reqwest client, media dir), `db.rs` (pool + `Money`, a `Decimal` stored as TEXT that sqlx reads/writes directly), `error.rs` (`AppError`/`AppResult`, `.or_404()`), `filters.rs` (askama filters `money`, `debt_color`, `yes_no`), `pagination.rs` (`Page`, 10 per page), `models.rs` (row types shared by pages: `CardRow`, `LotRow`, `DeckVersionRow`), `forms.rs` (`FormData`: raw form pairs → validated values + error list; repeated card rows `cards-<i>-<field>`), `card_data.rs` (ygoprodeck `get_or_fetch`, background card→card_data linking, `insert_lot_cards`), `tests.rs` (handler tests on a temp migrated DB, no network).
  - `routes/<domain>.rs`: one module per page group, each with its row structs, queries, `#[derive(Template)]` structs and handlers. Read-only routes go in `routes::router()`, **every write route in `routes::write_router()`** (future login layer). Form modules: `lot_form.rs`, `deck_form.rs`, `budget_form.rs`.
  - `templates/`: `base.html`, `list_base.html` (blocks `items`/`filter`), partials prefixed `_`. CSS copied from Django in `static/css/`.
- `domain/` — pure business logic (no DB/HTTP), unit-tested: `card` (name normalisation, statuses), `ydke`, `lot` (lot type, shipping cost), `budget` (monthly recap/totals), `monte_carlo`, `cardmarket` (shipment PDF parsing via lopdf).
- `importer/` — one-off loader from Django `dumpdata` JSON into the sqlx schema.
- `migrations/` — sqlx migrations (shared by both crates via `sqlx::migrate!("../migrations")`).
- `data/` — git **submodule** (`Ort1e/my_yu_gi_oh_card_data`, private): `data.json` (Django dump), `db.sqlite3` (Django DB), `media/card_images/`, `media/shipment_files/`.

## Commands

All inside the Nix shell (`nix develop`, or automatic with direnv). Run from the **repo root**: `ServeDir::new("static")` and the default `DATABASE_URL` are relative paths.

```bash
cargo check --workspace
cargo test --workspace                    # unit tests
cargo clippy --workspace --all-targets
# parser vs the real shipment PDFs of the data submodule (prints only counts/lot ids)
cargo test -p domain --test real_shipments -- --ignored --nocapture
cargo run -p backend                      # http://127.0.0.1:3000 (BIND_ADDR to change), runs migrations on startup
# rebuild the DB from the Django dump (importer is NOT idempotent: start from an empty DB)
rm -f app.db && cargo run -p importer -- data/data.json
cd frontend && npm install && npm run dev   # Vite dev server, proxies /api → :3000
cd frontend && npm run build                # → static/complex/bundle.{js,css}
sqlite3 app.db                              # inspect
```

`.env` sets `DATABASE_URL=sqlite:app.db` (gitignored, as is `*.db`). Both binaries create the DB file if missing.
Backend env vars (defaults): `BIND_ADDR` (`127.0.0.1:3000`), `MEDIA_DIR` (`data/media`), `UPLOADS_DIR` (`uploads`, gitignored), `CARDMARKET_ACCOUNT` (`0rtie`).

## Database conventions (`migrations/0002_ygo_schema.sql`)

- Primary keys are the Django pks, imported verbatim.
- Money (`price`, `no_card_price`, `amount`) is **TEXT**, never REAL (Django `DecimalField`). Read/write it as `db::Money` / `rust_decimal::Decimal`; never `SUM()` it in SQL (that goes through REAL).
- Dates are ISO `YYYY-MM-DD` TEXT. Booleans are INTEGER 0/1.
- Django M2M → join tables `deck_version_{main,extra,side}_deck`, `card_category_condition`.
- Django MTI `CardConditionalCategory(CardCategory)` → **flattened** into `card_category` with nullable `condition_description`, `categorie_true_id`, `categorie_false_id`. "Is conditional" = `condition_description IS NOT NULL`.
- Domain names are kept from Django (some French): `unite` (one card in a lot), `apport` (one-off budget contribution), `categorie_true/false`.
- `0002_ygo_schema.sql` mirrors Django's constraints (UNIQUE, CHECK on choice fields, nullability, ON DELETE; PROTECT → RESTRICT). Empty Django FileFields (`""`) are imported as NULL.
- `sqlx::migrate!` embeds migrations at compile time and checksums applied ones. **Until cutover** `0002` is the baseline and may be edited: then rebuild with `rm -f app.db && cargo run -p importer -- data/data.json`. **After cutover** only add new numbered files.
- SQLite + FKs: never drop/rebuild a parent table in a migration while `foreign_keys` is on — `DROP TABLE` fires ON DELETE CASCADE in children.

## Migration status

Done: Rust workspace, Nix env, full schema with Django's constraints + importer (row counts, FK and integrity checks verified against `data.json`).
Read-only pages ✅ (scaffold demo removed). Budget figures checked month by month against a re-implementation of Django's logic on `data.json`.

Decided: serve from `/` (drop Django's `/my_ygo_cards/` prefix, e.g. `/my_ygo_cards/cards/` → `/cards/`); the deck builder becomes the Svelte app in `frontend/` (don't port the vanilla JS).
Auth: none for now, server binds to 127.0.0.1 by default. Keep every write route in its own router so a login layer (single password from env + session cookie) can be added in one place when the app is served on the LAN; `/media/shipment_files` must go behind it too.
Port order: schema ✅ → shared domain logic + tests ✅ → read-only pages ✅ → lots/budget writes ✅ → deck builder.

Rust routes so far: `/` dashboard, `/budget`, `/cards` (+ filters), `/cards/:id`, `/sellers`, `/sellers/:id`, `/lots` (+ filters), `/lots/:id`, `/decks`, `/decks/:id`, `/card-image/:card_data_id` (serves `media/card_images/<name>.jpg`, downloading it once from the URL in `card_data.json_data` if missing — Django downloaded during page render), `/static`, `/media`.
Write routes: `/lots/add` (form + `POST /lots/add/shipment` PDF upload → pre-filled form, PDF kept as `uploads/pending-<token>.pdf` (deleted after 24 h) and moved to `media/shipment_files/shipment_<lot id>.pdf` on save), `POST /lots/:id/cards`, `/decks/add`, `/budget/manage` (+ `/budget/manage/budget` upsert, `/budget/manage/apports`, `/budget/manage/apports/:id/delete`).
Deliberate differences: card rows added/removed with JS instead of `?cards=N` reloads; identical PDF cards are one row with a quantity; a new seller keeps is_person/source (Django's `get_or_create(name=)` dropped them); new cards are linked to card_data in a background task right after saving (Django fetched lazily on page views); budgets/apports have a page (Django used its admin).
Missing vs Django: auth, editing/deleting cards, lots, sellers and decks (Django only had that in its admin), fetching card_data for the 3 imported cards that have none, deck builder + API, management commands.

Django reference for everything (✅ = ported):

### Pages (Django views in `my_ygo_cards/views/`, templates in `my_ygo_cards/templates/`)
| Django URL | View | Notes |
|---|---|---|
| ✅ `/` | `index.index` | Dashboard: Chart.js charts (card vs shipping spend per month, cards bought per month, card-price histogram), last 5 months recap, totals, `available_debt = last budget × 3` |
| ✅ `/budget/` | `budget.budget_overview` | `RecapEntry` / `BudgetTotals` logic: monthly budget carried forward + apports + sale-lot revenue; spending from non-cancelled purchase lots |
| ✅ `/cards/`, `/cards/card/<pk>/` | `cards.py` | Filters: status, proxy, name (name OR en_name icontains), card_type, code, sold. Paginate 10, order `en_name, id`. Nav link defaults to `?proxy=false&sold=false` |
| ✅ `/sellers/`, `/sellers/seller/<pk>` | `sellers.py` | Seller detail lists lots + total card count |
| ✅ `/lots/`, `/lots/<pk>/`, `/lots/add/` | `lots/lots.py`, `lots/lot_form.py` | Lot detail POST adds N unites (quantity → N new cards). Add-lot: lot form + seller select/create + unite formset + **Cardmarket PDF upload that pre-fills the form** |
| ✅ `/decks/`, `/decks/deck/<pk>`, `/decks/add/` | `deck.py` | Add deck also creates its first version |
| `/deck_builder/<deck_version_id>/` | `rest_api/deck_builder_api.DeckBuilderView` | GET page + POST tournament form. Rewrite as the Svelte app; the old vanilla JS in `my_ygo_cards/static/js/deck_builder/` is only a behaviour reference |
| `/accounts/login/` etc. | Django auth | Login required for add lot, add deck, deck builder. No Rust equivalent yet |

### JSON API (`rest_api/deck_builder_api.py`, `rest_api/serializers.py`)
Under `api/deck_versions/<id>/`: `""` GET/PATCH/DELETE · `clone/` POST · `proxy/` POST · `proxy/<card_id>/` DELETE · `card_lists/` POST (filtered cards not in deck, forced proxy=false sold=false) · `categories/` GET/POST · `assign_categories/<card_id>/` GET/POST · `categories/delete/<category_id>/` DELETE · `monte_carlos/` POST.
Plus `api/decks/<deck_id>/import_ydke/` POST. The Svelte app is the only consumer, so shapes may be cleaned up, but keep the information the DRF serializers expose.

### Domain logic
Ported to `domain/` ✅ (pure parts): `normalize_card_name` / `clean_card_name`, YDKE encode/parse, `shipping_cost`, budget recap + totals + available debt, Monte Carlo, Cardmarket parser (validated on all 135 real PDFs). Deliberate differences are documented in each module's doc comment.

Still to port (needs DB or HTTP, goes in the backend):
- Apply `normalize_card_name` to `en_name` on every card insert/update (Django did it in `Card.save`).
- `CardData.get_or_fetch` + `ImageDownloader`: ygoprodeck API `https://db.ygoprodeck.com/api/v7/cardinfo.php?name=|id=`; images cached as `data/media/card_images/<name_with_underscores>.jpg`, served under `media/`.
- `Card.is_sold`: has a unite in a non-cancelled `sale` lot.
- YDKE import (`DeckVersion.from_ydke`): reuses unused non-proxy owned cards with the same `en_name`, else creates proxy cards (fetching card data by ygopro id).
- Deck price: Σ over cards of the min unite price.
- Proxy cleanup: deleting a deck version / removing a card deletes proxy cards no longer in any deck.
- Clone deck version: duplicates proxy cards, shares real cards.
- Shipment upload: Django saves uploads as `uploads/shipment_<n>.pdf`, then moves them to `media/shipment_files/shipment_<lot id>.pdf` on lot creation. Cardmarket account name is `0rtie`.
- Management commands: `scrap_ban_list` (Konami EU JSON at `img.yugioh-card.com/eu/_data/fllists/`), `clean_cards` (renormalize names, relink card_data, prune proxies), `clean_card_categories` (drop assignments for cards no longer in the category's deck version). Could become importer subcommands or a separate CLI crate.

Also on `main`: `deck_builder_frontend/` is an abandoned React SPA stub; ignore it.

## Privacy rule: shipment PDFs

Cardmarket shipment PDFs (uploads, `data/media/shipment_files/`, and the test PDFs in `my_ygo_cards/tests/doc/` on `main`) contain **personal data** (names, addresses).
- Never store their extracted text anywhere: not in the DB, logs/tracing, error messages, test fixtures, snapshots, docs, commits, or Claude memory. Only the parsed fields the app needs (dates, total, seller name, card lines) may be persisted.
- Parse the text in memory and drop it. Tests must read the PDF at runtime and assert on parsed fields; never paste PDF text into source files. Unit tests use synthetic text.
- Use `lopdf` for extraction: `pdf-extract` `println!`s text fragments to stdout. Check any new PDF crate for prints/logs of content.
- To debug the layout, print a masked shape (letters → `a`, digits → `9`), never the text.
- Don't print or quote PDF contents in chat/terminal output, and don't copy the PDFs to new locations without asking.

## Working conventions

- Match existing style: handlers + askama templates per page in `backend/src/routes/pages.rs`, JSON in `routes/api.rs`, state is `db::Pool` (`SqlitePool`). Split into modules per domain as it grows.
- Handlers return `AppResult<T>`; use `?` and `.or_404()`, never `.unwrap()` on DB results.
- Askama quirks: method arguments in templates are passed inconsistently by value/reference — prefer argument-less helper methods (`page.next_href()`); `{% let x = field %}` moves, so expose `&` getters for non-Copy fields.
- Format on save is on (rust-analyzer from PATH). Keep `cargo check --workspace` clean.
- When porting a feature, read the Django model + view + template + serializer from `main` first and keep its behaviour (filters, ordering, pagination size). Idiomatic Rust is preferred over strict parity: enums, `Result` instead of crashes, fixing obvious Django bugs are fine — note each deliberate difference in a doc comment.
