# My Yu-Gi-Oh! card collection

A personal web app to track a Yu-Gi-Oh! collection: the cards owned, the lots
they were bought or sold in (with Cardmarket shipment import), sellers, a
monthly budget, decks with a deck builder (YDKE import/export, categories,
opening-hand simulation) and the Forbidden & Limited lists.

This branch (`rust-stack`) is the Rust rewrite of the Django app on `main`.

## Stack

- **Backend**: Rust — axum, sqlx (SQLite), askama templates
- **Deck builder**: Svelte + Vite (`frontend/`)
- **Data**: the private `data` submodule holds the Django dump, card images
  and shipment files

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

Create the database from the Django dump:

```bash
cargo run -p importer -- data/data.json
```

## Running

```bash
cargo run -p backend          # http://127.0.0.1:3000
```

The server only listens on localhost: there is no login yet. Settings come
from the environment (or `.env`): `DATABASE_URL` (`sqlite:app.db`),
`BIND_ADDR`, `MEDIA_DIR` (`data/media`), `UPLOADS_DIR` (`uploads`),
`CARDMARKET_ACCOUNT`.

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
