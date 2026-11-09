# My card and book collection

## What it's for

A personal web app with four goals, in this order:

1. **Manage the Yu-Gi-Oh! card collection** (the main one): the cards owned,
   the lots they were bought or sold in (a Cardmarket shipment PDF fills the
   lot form), sellers, a monthly budget, the market value, statistics and
   advent calendars; also a **Pokémon** collection (booster openings,
   wishlist, prices). Pages: Cards, Lots, Sellers, Budget, Value, Statistics,
   Advent, Rarities, `/pokemon`.
2. **Play competitively**: decks with a deck builder (YDKE import/export,
   categories, opening-hand simulation), built decks and what to buy for
   them, formats and the Forbidden & Limited lists, tournaments with their
   rounds, the list played and statistics. Pages: Decks, Built decks,
   Formats, Tournaments, Stats.
3. **Share a public copy**: every morning the NAS puts a read-only static
   site (the cards, statistics, market values, the public decks and the
   tournaments, the books; nothing private) on a web hosting.
4. **Keep the books**: the novels at home, added by ISBN (looked up, or
   scanned with a phone), with authors, series and their missing volumes,
   and book advent calendars. Pages: `/books`.

It runs on a NAS at home, for the home network only. Every morning the NAS
backs the data up and writes the public copy.

Written in Rust; it replaced the original Django app in October 2026 (still
in the history, tag `django-final`).

## Stack

| Part | Tech |
|---|---|
| Server | Rust: axum, sqlx (SQLite), askama templates |
| Deck builder | Svelte 4 + Vite (`frontend/`), talks JSON to `/api/*` |
| Data | SQLite. The live database is on the NAS; the private `data` submodule keeps copies of it, with the pictures and shipment files |
| Environment | Nix flake (`nix develop`, or direnv), Rust pinned in `rust-toolchain.toml` |
| Deployment | the CI builds a static binary for the NAS (`nas-latest` release); systemd, a nightly job, SFTP upload of the public copy (`deploy/`) |

Outside services: [YGOPRODeck](https://ygoprodeck.com/api-guide/) (card
data, pictures, prices), Cardmarket's price guide files, Konami's
Forbidden & Limited lists, [TCGdex](https://tcgdex.dev) and the
[Pokémon TCG API](https://pokemontcg.io) (Pokémon cards and pictures), Open
Library, Google Books and the BnF's catalogue (ISBN lookups).

## Documentation: where to read what

Read in this order; each step assumes the ones before.

1. **This README**: what the app is, how to start it on your computer.
2. **[docs/guide.md](docs/guide.md)**: the user guide, by goal. The menus,
   the pages and the usual tasks (a lot, a deck, a tournament, books, the
   phone).
3. **[docs/tour.md](docs/tour.md)**: the architecture, an hour with the
   code open. The foundations (crates, a page request, a form, the
   database), then each goal: the collection, competitive play, the public
   copy, the books and Pokémon.
4. **[docs/recipes.md](docs/recipes.md)**: how to change it (a page, a
   form, a migration, business logic, a deck builder feature, a test) and
   run it (backups, the NAS).
5. **[deploy/README.md](deploy/README.md)**: the NAS and the public copy.
   SSH, first install, the nightly job, updates, backups to git, what to
   type when something goes wrong.
6. **[CLAUDE.md](CLAUDE.md)**: the project's conventions in one dense page
   (goals, database rules, privacy rule for the shipment PDFs, who writes
   what), written for Claude but a good checklist for anyone.
7. **[TODO.md](TODO.md)** (what's next). Designs not built yet go in
   `docs/specs/` (the folder is created when one is needed; today:
   [deck-lists.md](docs/specs/deck-lists.md)).

## Running it on your computer

Everything comes from the Nix flake:

```bash
nix develop                    # or `direnv allow` once
git submodule update --init    # the private data repository (optional)
cd frontend && npm install && npm run build && cd ..   # the deck builder, once and after each change
cargo run -p backend           # http://127.0.0.1:3000
```

The live data is on the NAS: on the computer, run the server on a **copy**
(`deploy/pc/pull-backup.sh` brings the NAS's latest one into `data/`):

```bash
cp data/app.db /tmp/copy.db
DATABASE_URL=sqlite:/tmp/copy.db cargo run -p backend
```

Settings come from the environment or `.env` (gitignored):

| Variable | Default | What |
|---|---|---|
| `DATABASE_URL` | `sqlite:data/app.db` | the database (created if missing, migrations run at start) |
| `BIND_ADDR` | `127.0.0.1:3000` | where to listen; a network address needs a password |
| `APP_PASSWORD` | none | the owner's password; none = no login (loopback only) |
| `APP_PASSWORD_<NAME>` | none | more users, one password each |
| `APP_ACCESS_<NAME>` | `books` | what that user may change: `all`, or a list (`books,pokemon`); everyone reads everything |
| `PUBLIC_READS` | `1` | `0` puts every page behind the login (the NAS) |
| `SECURE_COOKIES` | `0` | `1` behind HTTPS |
| `MEDIA_DIR`, `UPLOADS_DIR` | `data/media`, `uploads` | pictures and files, pending uploads |
| `CARDMARKET_ACCOUNT` | `0rtie` | your Cardmarket name, for reading the shipment PDFs |
| `GOOGLE_BOOKS_API_KEY` | none | Google Books' API key for the ISBN lookups (free, Google Cloud console → Books API → Credentials); without one Google nearly always refuses |

## Commands

The server binary is also the maintenance tool:

```bash
cargo run -p backend -- scrap-ban-list    # new Forbidden & Limited lists
cargo run -p backend -- clean-cards       # tidy names, card data links, proxies (also daily, by itself)
cargo run -p backend -- refresh-prices    # market prices (also weekly, by itself)
cargo run -p backend -- clean-pictures    # delete pictures no card uses (also daily)
cargo run -p backend -- export-public     # the public copy, into public-export/
cargo run -p backend -- backup --to DIR   # a consistent copy while the server runs
```

Most accept `--dry-run`. All of them: `docs/tour.md`, "Maintenance
commands".

## Development

```bash
cargo test --workspace                 # temporary databases, never the real one
cargo clippy --workspace --all-targets
cargo cov                              # coverage report, target/llvm-cov/html/index.html
cd frontend && npm run dev             # deck builder hot reload, http://localhost:5173/?id=<version id>
```

Work goes on `dev`; `main` is what runs on the NAS. A pull request from
`dev` into `main` (with the CI green) gathers several changes, and the NAS
installs the new build of `main` the next morning (7:30).
