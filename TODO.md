# TODO

Add the next ideas here (a spec in `docs/specs/` for a big one).

## Open

- **The nightly job didn't run on 2026-10-28** (no upload of the public
  copy). Likely cause: the NAS is **on from 7:00 to 22:00 only**, and the
  timer is set for 3:30. Its `Persistent=true` only catches up at boot
  once the timer has run at least once (systemd keeps a stamp of the last
  run), and the network may not be up that early in the boot. To check:
  `ssh nas journalctl -u collection-nightly -b`,
  `ssh nas systemctl list-timers collection-nightly`. Fix: move the timer
  to an hour when the NAS is on (e.g. 21:30, or at lunchtime), maybe
  `After=network-online.target` on the service.
- **The app doesn't load on the friend's phone** (works everywhere else).
  To try, in order: `http://` typed in full (Chrome's "Always use secure
  connections", Firefox/Brave HTTPS-only, Samsung Internet send it to
  `https://`, which the NAS doesn't serve); the phone on the main Wi-Fi,
  not a guest network or mobile data; a VPN, ad blocker or "private DNS"
  app on the phone (it may send every page through the internet); another
  browser. If it still fails: does `http://192.168.1.202:3000/login` show
  anything, and what error does the browser give.
- **A better public copy**: the app's own layout (header, colours of each
  collection) for the public pages, and the Yu-Gi-Oh! collection's
  statistics (value, counts, by rarity / set…; to be chosen). A spec first.
- **Two kinds of users**: the main password (`APP_PASSWORD`) can do
  everything; a secondary one sees everything but can only change the
  books. A role per user in `common::web::auth` and a check on each
  collection's `write` routes.
- **Public or private repository**: the owner is deciding. If private:
  `GITHUB_TOKEN` in the NAS's `nightly.env` (deploy/README.md, "Updates
  without a token"), the protection of `main` needs a paid plan, and the
  CI minutes become limited (2,000 a month).
- **Formats** in the deck builder (Edison…): `docs/specs/formats.md`
  (`format::first_release` and `check` are the owner's).
- Once: a scan of the public site (testssl.sh, OWASP ZAP baseline) and of
  the app from the home network (ZAP).
- Maybe: Dependabot for the Cargo and npm updates (weekly, onto `dev`).

## Done

One line each; the details are in `docs/tour.md` and the git history.

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
