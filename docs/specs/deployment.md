# Spec: deployment, security and repository workflow

Status: **plan, answers in** (2026-10-26). Steps 1 and 2 done on `dev`
(CI, dependencies; security items 1–9 with their tests). Step 3: the
export is done (`export-public`, about 21 MB; the Pokémon wishlist is
public too, owner's request), the upload waits for the protocol the OVH
plan offers (SFTP or FTP over TLS: its FTP-SSH tab). The NAS steps wait for its exact model (`uname -m`). GitHub
settings are changed only after a go.

## What we want

- The whole app, for the owner and one friend, from anywhere (phones too).
- A **public, read-only view of the collection** to share with others:
  the Yu-Gi-Oh! cards, the Pokémon cards and the books, **with their market
  value** (owner, 2026-10-26). No lots, sellers, prices paid, budget,
  decks, tournaments, advent calendars, and nothing that changes data.
- The owner has a **NAS, always on**, and an **OVH domain name** with the
  free hosting plan: 100 MB, 1 FTP access, free SSL, static files and PHP
  only (no program of ours can run there).

## The plan in one picture

```
 owner, friend (phones, laptops)            anyone
        │ Tailscale (private network)          │ https://collection.<domain>
        ▼                                      ▼
 ┌──────────── NAS ─────────────┐        OVH free hosting
 │ the app (all of it, login)    │  FTPS  (static files only:
 │ data/app.db + media (live)    │ ─────► cards grid + card pages,
 │ nightly: backup → data repo   │ nightly  small pictures)
 │          export → OVH         │
 └───────────────────────────────┘
```

- **Nothing on the NAS is open to the internet**: no port forwarding. The
  app is reached only through Tailscale (a private VPN, free for personal
  use). The owner and the friend each install it; the owner shares the NAS
  with the friend, who can reach the app and nothing else (Tailscale ACL).
  HTTPS on the private name comes from Tailscale (`tailscale serve`).
- **The public view is a static copy** written by the app (a new command,
  `export-public`) and uploaded to the OVH hosting every night. There is no
  server to attack, only files; the copy holds only what we choose to show.
  About 650 Yu-Gi-Oh! and 60 Pokémon cards with small pictures (~25 KB
  each) and the books (no pictures) fit well under 100 MB. Address:
  `collection.<domain>` (the owner doesn't mind which).

The other option, exposing a read-only part of the app on the NAS through
the domain (port forwarding + OVH DynHost, or a Cloudflare tunnel), means a
program reachable by anyone and the NAS behind it: more risk and more
work. Not recommended.

## The data moves to the NAS

Today `data/app.db` on the owner's computer is the live database,
committed by hand. After the move:

- the NAS holds the live `app.db` and `media/`; the local copy is only for
  development (a scratch copy, never the live one);
- every night the NAS makes a safe copy (`VACUUM INTO`, consistent while
  the server runs), commits it with the new media and pushes to the private
  data repository, with a deploy key that can push to that repository only;
- before each update of the app: the same backup, then the update (the
  migrations run at start). A restore is tested once, then after big
  changes.

## Security work (code, on `dev`, each with tests)

1. **Private mode**: an env setting that puts every page behind the login,
   reads included (`PUBLIC_READS=false` on the NAS). Test: every route of
   every collection answers the login redirect or a 401.
2. **The cookie**: `Secure` when served over HTTPS (setting), still
   `HttpOnly` and `SameSite=Strict`. Test on the `Set-Cookie` header.
3. **Password guessing**: a limit per IP (e.g. 5 failures, then 15 minutes
   of refusal), logged. Test.
4. **Two people**: one password each (owner's answer), so the friend's
   can be changed alone; the log says who did what.
5. **Security headers** on every answer: Content-Security-Policy (scripts
   from the app and the two CDNs used: Chart.js, ZXing on jsDelivr),
   `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`,
   `Referrer-Policy`. Test on a page.
6. **Files**: `/media` can't leave its folder (`../`), the private folders
   need the login (exists, test kept), uploads keep their size limits; a
   huge image (decompression bomb) and a broken PDF are refused cleanly,
   without a crash. Tests.
7. **HTML injection**: review the three `|safe` in templates and the
   scripts building HTML (`card_rows.js`, `tournament_form.js`): only our
   own text, or escaped. A test with a card named `<script>…`.
8. **The public export**: a test that the exported files contain no lot,
   seller, budget, price paid, deck, tournament or calendar.
9. **Dependencies**: `cargo audit` and `npm audit` in CI (below). Done
   on `dev`: `cargo update` (rustls, webpki, tokio fixes), `lopdf` 0.42
   (stack overflow on nested PDFs; same parsing of the 144 real
   shipments), lopdf's logs capped at `error`. Accepted with reasons in
   `.cargo/audit.toml`: `rsa` (never built), `sqlx` 0.7 (Postgres/MySQL
   only). Later: sqlx 0.8; the deck builder's build tools (Vite 8,
   Svelte 5: dev-server advisories only, nothing in the bundle).
10. **The NAS**: its admin page not exposed, automatic updates, Tailscale
    ACL limited to the app's port, the passwords in its environment only
    (never in git).
11. **Once live**: a scan of the public site (testssl.sh, OWASP ZAP
    baseline) and of the app over Tailscale (ZAP).

## Repository: `dev` branch and protection

- **`dev`**: every change lands there first (Claude commits on `dev`);
  **`main`** = what runs on the NAS, updated by merging `dev` when tested.
- **CI** (GitHub Actions, `.github/workflows/ci.yml`), on every push and
  pull request: `cargo fmt --check`, `cargo clippy`, `cargo test`, the deck
  builder's `npm run build`, `cargo audit`, `npm audit`. Without the data
  submodule: the tests never need it. Free minutes are plenty for a
  private repository.
- **Protection of `main`** (a GitHub ruleset): no force push, no deletion,
  changes only through a pull request from `dev` with the CI green. The
  owner may keep a bypass for emergencies. Same "no force push, no
  deletion" on the data repository's `main`.
- **Dependabot** for Cargo and npm updates (weekly, onto `dev`).
- The GitHub settings are changed by the owner, or by Claude with `gh`
  after a go (they change who can push what).

## Steps (each one reviewed before the next)

1. Repository: push `dev`, add the CI, then the protection of `main`.
2. Security work 1–9 on `dev` (tests), merged into `main`.
3. `export-public` + the FTPS upload; tried on a scratch copy, then the
   first upload to OVH.
4. The NAS: the app as a container or a single static binary for its CPU,
   Tailscale, the nightly backup and export, the update procedure
   (`docs/recipes.md`).
5. The move: last local commit of the data, first start on the NAS, the
   phones switched to the Tailscale address. The local server is no longer
   started on `data/app.db`.
6. The scans (11), then this spec moves into `docs/tour.md` / `recipes.md`
   and is deleted.

## Updates of the app

Automatic if simple (the owner prefers it, not mandatory): the CI builds
the app for the NAS when `main` changes (a container image on GitHub's
registry, or a binary), and the NAS checks for a new one every night,
backs up, then restarts on it. Otherwise one command by hand.

## The NAS: a Netgear ReadyNAS

A line Netgear has stopped (no more security updates), running ReadyNAS
OS 6 (an older Debian with systemd); depending on the model, an ARM or an
Intel processor. So:

- the app runs as **one static binary** (Rust, musl: no library to
  install, no Docker), built by the CI for the NAS's processor, started by
  a systemd service, its data in a folder of a NAS share;
- Tailscale also runs as its static binary (Tailscale's own packages
  for ARM and x86);
- the NAS stays **unreachable from the internet** (no port forwarding, its
  admin page on the home network only): with no more updates, that is the
  main protection.

## Still open

1. The exact model, or its processor: `uname -m` over SSH (`armv7l` or
   `x86_64`), or System → Overview in its admin page.
