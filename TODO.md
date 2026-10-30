# TODO

Add the next ideas here (a spec in `docs/specs/` for a big one).

## Open

- **The book scan on the friend's phone** said "out of memory" in Chrome
  (2026-10-30): the barcode is now read on the NAS, and "From a photo"
  avoids the camera app over the browser. To check once deployed.
- **Book data to fix by hand** (seen 2026-10-30): Harry Potter and the
  Goblet of Fire has volume 3 (it's 4); the series "Robert Langdon --" and
  "Red Rising, 3" were badly read from Open Library (fixed for new
  lookups): rename them in their books' forms.
- Maybe: a Google Books API key (free, 1000 lookups a day): without one its
  shared quota is often used up (429), and Open Library lacks some books
  (9781464266430).
- **Testers of the public copy:** after the 2026-10-28 changes (back link,
  full screen card, bigger tile text), is the text still too small on
  phones? If so: bigger text on phones everywhere, not only the tiles.
- Maybe: statistics for the Pokémon cards and the books too (by set,
  rarity, language; by author, series, format), like `/collection/stats`.
- Maybe: hide the add/edit buttons a user can't use (they get a "Not
  allowed" page today).
- **Formats** in the deck builder (Edison…): `docs/specs/formats.md`
  (`format::first_release` and `check` are the owner's).
- Once: a scan of the public site (testssl.sh, OWASP ZAP baseline) and of
  the app from the home network (ZAP).
- Maybe: Dependabot for the Cargo and npm updates (weekly, onto `dev`).

## Done

One line each; the details are in `docs/tour.md` and the git history.

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
