# Spec: deployment, security and repository workflow

Status: **in progress** (2026-10-26). On `dev`: step 1 (CI,
dependencies), step 2 (security items 1–9, with tests), step 3
(`export-public`, `deploy/nas/upload-ovh.sh`: SFTP, enabled by the owner
and tested against the hosting), step 4 (`deploy/`: the CI's
NAS build, `backup`, `fetch-release`, the scripts and `deploy/README.md`).
Left: the protection of `main` (owner), the first install and the move
(step 5, with the owner), the scans. GitHub settings are changed only
after a go.

## What we want (owner's answers, 2026-10-26)

- The whole app for the owner and their friend, who live together:
  **from the home network only** (computers, phones on the Wi-Fi). No
  access from outside, so no VPN (Tailscale dropped).
- A **public, read-only copy** to share with others: the Yu-Gi-Oh! cards,
  the Pokémon cards and wishlist, the books, **with market values**. No
  lots, sellers, prices paid, budget, decks, tournaments, advent calendars,
  notes, and nothing that changes data.
- The machines: a **Netgear ReadyNAS** (x86_64, 2 GB of memory, ReadyNAS OS
  6.10.10 = Debian 8, kernel 4.4, `curl`, `sqlite3` and systemd, no `git`;
  volumes `/Data1To` (also `/apps`, `/home`) and `/Data500`), always on,
  not reachable from the internet; an **OVH domain name** with the free
  hosting (100 MB, FTP, free SSL, static files and PHP only: no program of
  ours can run there).

## The plan in one picture

```
 owner, friend (home Wi-Fi)                    anyone
        │ http://192.168.1.202:3000              │ https://collection.<domain>
        ▼                                        ▼
 ┌──────────────── NAS ─────────────────┐   OVH free hosting
 │ /apps/collection       the app        │   (static files only:
 │ /Data1To/collection    live data      │ ─► the cards, wishlist and books,
 │ /Data500/…-backups     nightly copies  │    small pictures, ~21 MB)
 │ nightly: backup, export + upload,     │
 │          update of the app            │
 └───────────────────────────────────────┘
        ▲ the owner's computer: pulls the latest copy and commits it to
          the private data repository (one script, weekly or at will)
```

- **The app is reachable from the home network only**: plain HTTP on the
  NAS's address, port 3000 (`BIND_ADDR=0.0.0.0:3000`). The box opens no
  port to the NAS: with no more updates from Netgear, that is its main
  protection. On the home Wi-Fi (WPA2/WPA3), HTTP is acceptable; the
  cookie can't be `Secure` then (`SECURE_COOKIES=0`). **Private mode** stays
  on (`PUBLIC_READS=0`): a guest on the Wi-Fi sees nothing without a
  password. Two users: `APP_PASSWORD` (owner) and `APP_PASSWORD_<NAME>`.
- **The public copy** is written by `export-public` and uploaded to the
  OVH hosting every night: only files there, nothing to attack. Address:
  `collection.<domain>`.

## The data on the NAS

- **Live**: `/Data1To/collection/app.db` and `media/` (the server's
  `DATABASE_URL`, `MEDIA_DIR`), `uploads/`. The owner's computer keeps a copy
  for development only, never the live one.
- **Nightly copies** on the other volume (other disks): `backend backup
  --to /Data500/collection-backups` writes `app-YYYY-MM-DD.db` (`VACUUM
  INTO`: consistent while the server runs), mirrors `media/` and keeps the
  last 14 days.
- **To GitHub** (no `git` on the NAS, and installing packages on ReadyNAS
  is risky): a script on the owner's computer copies the latest copy and
  the media into the data submodule and commits them (`docs/recipes.md`).
- **Before each update** of the app: the same backup; the migrations run at
  start. A restore is tested once, then after big changes.

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
10. **The NAS**: no port opened by the box, its admin page on the home
    network only, the passwords in the service's settings file only (never
    in git, readable by root only).
11. **Once live**: a scan of the public site (testssl.sh, OWASP ZAP
    baseline) and of the app from the home network (ZAP).

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

## Updates of the app

Automatic (the owner prefers it): when `main` changes, the CI builds the
app for the NAS (x86_64, static: musl, so Debian 8's old libraries don't
matter) with the deck builder and the stylesheets, and publishes it as a
GitHub release (`nas-latest`). Every night the NAS asks for it (`curl`, a
read-only token: the repository is private), and when it changed: backup,
replace, restart. By hand: the same script.

## Steps (each one reviewed before the next)

1. Repository: `dev`, the CI, the protection of `main` (owner). Done but
   the protection.
2. Security work 1–9 on `dev`, with tests. Done.
3. `export-public` (done), then the upload to OVH (waits for the protocol).
4. The NAS: the CI build and release, `backend backup`, the service
   (`deploy/nas/`: systemd unit, settings file, nightly and update
   scripts), the recipe in `docs/recipes.md`.
5. The move: the server stopped on the computer, the data committed, then
   copied to `/Data1To/collection/`, first start on the NAS, the phones and
   computers on `http://192.168.1.202:3000`.
6. The scans (11), then this spec moves into `docs/tour.md` / `recipes.md`
   and is deleted.
