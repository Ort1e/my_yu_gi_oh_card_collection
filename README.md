# myapp

Rust backend (axum + sqlx/SQLite + askama) serving mostly server-rendered
pages, plus one JS-powered "complex page" (Svelte) for the one view that
needs richer client-side interactivity.

## Layout

```
myapp/
├── Cargo.toml
├── src/
│   ├── main.rs           # router setup
│   ├── db.rs              # sqlite pool + migrations
│   └── routes/
│       ├── pages.rs       # simple server-rendered pages (askama + htmx)
│       └── api.rs         # JSON API, only consumed by the complex page
├── templates/              # askama templates (server-rendered HTML)
├── migrations/             # sqlx migrations, run automatically on startup
├── static/                 # served at /static — this is where the built
│                            # Svelte bundle lands (static/complex/)
└── frontend/                # separate JS project, ONLY for the complex page
    ├── package.json
    ├── vite.config.js
    └── src/
        ├── main.js
        └── App.svelte
```

## Why it's split this way

- Every "simple page" (list views, forms, static-ish content) is plain
  askama templates rendered by axum, with htmx handling small bits of
  interactivity (form submit -> swap a fragment) without any JS build step.
- The one page that actually needs a real frontend framework (`/complex`)
  loads a self-contained Svelte bundle built separately by Vite. It talks
  to the backend only through `/api/*` JSON endpoints — it doesn't know or
  care that the rest of the app is server-rendered.
- This means you can grow the complex page's frontend independently
  (add state management, routing within that page, etc.) without touching
  how the rest of the site works, and without introducing a JS build step
  for pages that don't need one.

## Dev environment (Nix)

A `flake.nix` is included, providing a pinned Rust toolchain (with
`rust-analyzer`, `clippy`, `rustfmt`), `sqlite`, `sqlx-cli`, and
`nodejs_20` for the frontend — no need to install any of this globally.

```bash
nix develop
```

If you use [direnv](https://direnv.net/), an `.envrc` is included so the
shell loads automatically when you `cd` into the project (run `direnv
allow` once).

Requires Nix with flakes enabled (`experimental-features = nix-command
flakes` in your nix.conf, or run commands with `--extra-experimental-features
"nix-command flakes"`).

## Running in development

Two processes, two terminals (inside `nix develop` in both):

```bash
# terminal 1 - backend (serves everything except the live-reloading JS)
cargo run

# terminal 2 - frontend dev server for the complex page only,
# with hot reload; proxies /api calls to localhost:3000
cd frontend
npm install
npm run dev
```

While developing the complex page, open the Vite dev server URL it prints
(usually http://localhost:5173) rather than /complex on the axum server —
that's what gives you hot module reload.

## Building for production

```bash
cd frontend
npm install
npm run build      # outputs to ../static/complex/ (bundle.js, bundle.css)

cd ..
cargo build --release
./target/release/myapp
```

Now a single axum binary serves everything: the server-rendered pages,
the JSON API, and the built Svelte bundle via `/static`.

## Database

sqlx migrations in `migrations/` run automatically on startup
(`sqlx::migrate!` in `src/db.rs`). Add new `.sql` files there for schema
changes — no separate migration command needed for this minimal setup,
though for a bigger project you may want the `sqlx-cli` tool for
generating/managing migrations explicitly.

## Adding a new simple page

1. Add a handler in `src/routes/pages.rs`.
2. Add an askama template in `templates/` (extend `base.html`).
3. Register the route in `pages::router()`.

## Extending the complex page

Everything under `frontend/` is a normal Vite + Svelte project — treat it
like any other JS frontend. Add components, install npm packages, add
client-side routing if the page grows multiple views, etc. It only needs
to keep talking to `/api/*` and building into `static/complex/`.
