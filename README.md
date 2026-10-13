# My Yu-Gi-Oh! card collection

A personal web app to track a Yu-Gi-Oh! collection: the cards owned, the lots
they were bought or sold in (with Cardmarket shipment import), sellers, a
monthly budget, decks with a deck builder (YDKE import/export, categories,
opening-hand simulation) and the Forbidden & Limited lists.

This branch (`rust-stack`) is the Rust rewrite of the Django app on `main`.

## Stack

- **Backend**: Rust — axum, sqlx (SQLite), askama templates
- **Deck builder**: Svelte + Vite (`frontend/`)
- **Data**: the private `data` submodule holds the database (`data/app.db`),
  card images and shipment files

## Setup

Everything comes from the Nix flake:

```bash
nix develop        # or `direnv allow` once
git submodule update --init
```

Build the deck builder once (and after each change in `frontend/`):

```bash
cd frontend && npm install && npm run build
```

The database `data/app.db` is tracked in the `data` submodule. To save the
data, stop the server (Ctrl-C: it checkpoints the SQLite WAL into `app.db`)
then commit in the submodule:

```bash
git -C data add -A && git -C data commit -m "…" && git -C data push
git add data && git commit -m "update data"
```

`data/app.db` was created once from the Django dump (`importer`, which
refuses a non-empty database); `data/data.json` and `data/db.sqlite3` are the
Django data it came from.

## Running

```bash
cargo run -p backend          # http://127.0.0.1:3000
```

Settings come from the environment (or `.env`): `DATABASE_URL`
(`sqlite:data/app.db`), `BIND_ADDR` (`127.0.0.1:3000`), `APP_PASSWORD`,
`MEDIA_DIR` (`data/media`), `UPLOADS_DIR` (`uploads`), `CARDMARKET_ACCOUNT`.

With `APP_PASSWORD` set, every change (forms, deck builder saves) and the
shipment PDFs need a login (`/login`, a 30-day session cookie); reading
stays open. Without it there is no login, and the server refuses to listen
on anything but a loopback address: set it before using
`BIND_ADDR=0.0.0.0:3000` to serve the local network.

For hot reload while working on the deck builder, run `npm run dev` in
`frontend/` next to the backend and open `http://localhost:5173/?id=<deck version id>`.

## Maintenance

```bash
cargo run -p backend -- scrap-ban-list   # new Forbidden & Limited lists (--all to re-import, --dry-run)
cargo run -p backend -- clean-cards      # tidy card names, card data links and proxies (--dry-run)
```

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
```

Card images and card data come from the [YGOPRODeck API](https://ygoprodeck.com/api-guide/),
ban lists from Konami's EU site.
