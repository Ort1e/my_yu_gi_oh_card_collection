# TODO

Add the next ideas here (a spec in `docs/specs/` for a big one).

## Open

### On the NAS (owner, after the next deploy)

- **The Edison format** (2026-11-03): the NAS has no format yet. Tournaments
  → Formats → Add, from the Konami list of 2010-03-01, cutoff 2010-03-01 (the
  first visit fetches the set dates from ygoprodeck, a few seconds). Check
  the cutoff against the community's definition (ygoprodeck's set dates are
  used: The Shining Darkness = 07/05/2010 there), then pick the format in the
  deck builder. Goat or others: the same way.
- **The ⚠ to-sell badge** in the deck builder (a copy flagged to sell in a
  built deck's slot): look at it once in a browser.
- **`backend find-pictures`** for the cards added before the Yugipedia
  pictures (~10 min), if the cards still show ygoprodeck's art.

### Ideas

- Statistics for the Pokémon cards and the books too (by set, rarity,
  language; by author, series, format), like `/collection/stats`.
- Hide the add/edit buttons a user can't use (they get a "Not allowed" page
  today).
- Choosing which copy fills a built deck's slot (today: best condition
  first, then the oldest, `built_deck::plan_build`; the owner's module).
- Once: a scan of the public site (testssl.sh, OWASP ZAP baseline) and of
  the app from the home network (ZAP).
- Dependabot for the Cargo and npm updates (weekly, onto `dev`).
- **Discord webhook for the live tournament notes** (owner's choice over a
  Telegram or a full bot, 2026-11-18): the NAS posts each round line and the
  day's summary to a channel. `DISCORD_WEBHOOK_URL` setting (a secret like
  the Google key: never logged, errors `without_url()`), `common::discord::post`
  (`{"content"}` ≤ 2000 chars, `allowed_mentions: {parse: []}`, one retry on
  429). One formatter, the JS one: `tournament_live.js` sends `line(round)` /
  `dayText()` as `share_line` in the round `PUT` and `share_text` in the end
  `PATCH`. Migration: `tournament.post_to_discord` (opt-in on the live start
  page, shown only with a webhook) + `tournament_round.posted_line` (a queue
  re-send posts nothing, a changed line is posted with "✏️"). Post after the
  commit in a `tokio::spawn`, a failure only a `warn!`; nothing under
  `cfg(test)`. Plumbing: Claude's.

### Tech debt

- `deck_version.tournament_id` is a dead NULL column (`REFERENCES` blocks
  `DROP COLUMN`; rebuilding `deck_version` with foreign keys on is too risky
  for it). Drop it only with a table rebuild done outside a transaction.
- sqlx 0.9 (libsqlite3-sys ≥ 0.39 clears RUSTSEC-2026-0459; `rsa` in the
  lock, never built, keeps RUSTSEC-2023-0071): then edit `.cargo/audit.toml`.

## Done

One line each; the details are in `docs/tour.md` and the git history.

- 2026-11-17: clean-up: the Django leftovers (importer, Python venv) removed,
  `card.is_proxy` dropped (0034), sqlx 0.8, the NAS checked at 0033.
- 2026-11-16: copies flagged to sell (0032), purchase lots in the public
  copy, several decks received per lot (0033), users and permissions in the
  DB with several Pokémon wishlists (0030).
- 2026-11-12: checked on the NAS: the book scan on the friend's phone, the
  book data fixed by hand, Tailscale (HTTPS address, live scan), the Google
  Books key, the public copy's text size on phones.
- 2026-11-12: booster suggestions on the Pokémon wishlist (the 3 boosters
  with the most wished cards, the 3 cheapest with one; 0028).
- 2026-11-09: the shopping list (`/decks/shopping`): built then planned
  decks sharing the collection's copies, errata, a Cardmarket wants list
  with its estimate and the budget left.
- 2026-11-12: deck versions as lists (0027): only built decks hold copies,
  the others see the collection as it is; built decks public by default.
- 2026-11-07: a deck's history (0025): a snapshot on every save, naming,
  comparing, a tournament's frozen list, "New version from this list".
- 2026-11-09: formats read and edit pages; the tournaments' form in parts.
- 2026-11-07: the menus by goal (Collection, Decks, Tournaments), the deck
  builder in tabs, the deck page as a hub.
- 2026-11-06: errata on the built decks: Yugipedia's errata cached
  (`yugipedia_errata`), the pure rule in `yugioh-domain::errata`, warnings on
  `/decks/built` and the deck builder's `…/check`.
- 2026-11-03: TypeScript: the deck builder in TypeScript, its API's types
  generated from the Rust structs (ts-rs), the page scripts checked from
  their JSDoc; `npm run check` in the CI; VS Code set up for it. Private
  books (off the public copy), one Scan button on the book form.
- 2026-11-02: formats (Edison…: `/formats`, own card pool and list,
  versions; in the deck builder: "Ban list or format", problems, tiles
  out of the pool, legal-only reserve); a card's page shows what the card
  is and says (type, attribute, level, ATK/DEF…, its text); the books'
  menu grouped (it no longer fits on medium screens otherwise); a note on
  the add page when the phone can't scan live.
- 2026-11-02: dates the French way on every page (dd/mm/yyyy); books read
  or not (switches, a ✓ in the public copy) and "What to read?"
  (`/books/random`, public too); ISBN lookups with a Google Books key
  (≤ 800 a day) and the BnF (first for French editions).

- 2026-10-30: the book lists as tables (+ the stand-alone books), a book's
  page with its cover (Open Library, not stored); deletions far from Save,
  their confirmations working again (the CSP blocked them).
- 2026-10-30: the friend's phone reaches the app (`http://` typed in full).
- 2026-10-30: a deck version's page shows its ban list, opening hands and
  tournament; the tournaments in the public copy; a card's picture of its
  own printing from Yugipedia (automatic when there's no doubt, when the
  card is added; else chosen on its edit page). Once on the NAS after the
  update: `backend find-pictures` for the cards added before (~10 min).

- 2026-10-29: a deck version's cards on a read-only page
  (`/deck-versions/:id`), public deck versions (played in a tournament, or
  shown by hand); the logos alike;
  the statistics by language fixed; the wishlist's cards seen large.

- 2026-10-28: testers' feedback: back to the list page you came from, a
  card full screen on tap, bigger tile text; the copy's pictures as WebP
  at 480 px (48 MB in all, the OVH plan has 100 MB); the morning job
  updates the app before writing the copy.
- 2026-10-28: the public copy made of the app's own pages (crawled in
  "public copy" mode), `/collection/stats`, the card seen large on hover.
- 2026-10-28: users who change the books only (`APP_ACCESS_<NAME>`, by
  default for every user but the owner).
- 2026-10-28: the nightly job at 7:30 (the NAS is off from 22:00 to 7:00,
  so it never ran at 3:30), after the network.

- 2026-10-27: deployed: the app on the NAS (CI's static build, systemd,
  nightly backup, export and update), the public copy on the OVH hosting
  (`<domain>/collection/`), `main` protected; security work (users, private
  mode, lockout, headers); lot refunds.
- 2026-10-26: one crate per collection (`yugioh`, `books`, `pokemon`, their
  domain crates, `common`, `advent`, a thin `backend`); one picture grid for
  both card collections (+ table view), card page picture, `clean-pictures`.
- 2026-10-23: Pokémon cards (`/pokemon`): booster openings, singles, graded
  copies, wishlist, TCGdex prices, rarity and variant guide.
- 2026-10-23: navigation bar in menus (Collection, Decks), phone "Menu".
- 2026-10-22: tournaments (rounds, games, labels, stats; the 7 old ones
  imported); one convention for data tables (sort, search).
- 2026-10-20: built decks (locked cards, proxies elsewhere, shopping list).
- 2026-10-19: collection value (ygoprodeck and Cardmarket prices, gain from
  lot prices); ban lists from the US pages when the EU JSON lags; rarity
  from card data (`clean-cards`).
- 2026-10-18: books (`/books`, ISBN lookup and scan, book advent calendars);
  card rarity and `/rarities` guide; quick add from a set; names in their
  code's language; advent calendar pages; automatic cleanups; logs; code
  split by area.
