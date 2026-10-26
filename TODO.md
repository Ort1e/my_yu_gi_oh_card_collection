# TODO

## Done

- [x] Split by collection (2026-10-26, branch `refactor-crates`): one crate per
      collection (`yugioh`, `books`, `pokemon`) with its pure domain crate,
      `common` (state, web plumbing, DB, photos, throttle, media, schedule,
      base templates, test harness), `common-domain` (conditions), `advent`
      (one calendar engine for cards and books), a thin `backend` binary.
      Shared on the way: the Pokémon picture grid for `/cards` too (with a
      table view), the list page base, one rarity guide renderer. Every
      route, feature and test kept.
- [x] Pokémon cards (2026-10-23): `/pokemon`, a third collection (TCGdex card
      data and Cardmarket prices, French/English/Japanese): booster openings,
      single cards, graded copies, sold not deleted, wishlist with price
      alerts, rarity and variant guide with help to choose.
- [x] Navigation bar (2026-10-23): links grouped in menus (Collection:
      Cards, Lots, Sellers, Value, Rarities; Decks: Decks, Built decks,
      Tournaments), the current page marked, a "Menu" button on phones.
- [x] Clean up the interface artifacts.
- [x] Add logs.
- [x] Refactor for readability (2026-10-18): files split by area
      (`web/`, `store/`, `routes/<area>/`, `templates/<area>/`).
- [x] Card rarity: `card.rarity`, `domain/src/rarity.rs`, read from the
      shipments, badges, filter, `/rarities` guide.
- [x] Faster card entry: the "Quick add from a set" panel (set code once,
      then numbers or a pasted list; rarities per code; names in the set's
      language), backed by `GET /api/sets/:code`.
- [x] Names in the language of their code (ygoprodeck, remembered in
      `card_translation`); English name shown under the French one.
- [x] Advent calendar page (`/advent-calendar`): the 2025 handmade
      calendar, 24 doors with the photo and the card of each day; new
      calendars created and filled in the app (recipe in docs/recipes.md).
- [x] Cleanups run by themselves: proxies, card data links and names after
      new cards, unused proxies when saving zones, a daily `clean-cards`.
- [x] Rarity from the card data (`clean-cards` step 7): cards without a
      rarity get the one of their code when it was printed in one rarity
      only (631 cards on 2026-10-19; 77 left to set by hand).
- [x] Ban lists from the US site's pages when Konami's EU JSON lags behind
      (it stopped at May 2026 for a while): `scrap-ban-list` falls back on
      them for missing dates, `--date` for a list not linked yet.
- [x] Collection value (2026-10-19): weekly market prices from ygoprodeck
      (`card_price`), `/collection/value` (value, gain from the lot prices,
      copies with a unit price compared one by one, moves, by deck), price
      line on card pages, dashboard tile.
- [x] Tournaments (2026-10-22): rounds and games (draws, unfinished),
      deck labels, `/tournaments` with the stats (by version, label,
      matchup, die, first/second); the 7 old tournaments imported from
      their notes.
- [x] Tables (2026-10-22): one convention for every data table, sorting
      and search (`static/js/tables.js`); the lot page groups the same
      copies.
- [x] Built decks (2026-10-20): a built version locks its cards (proxies
      in the other decks), deck box and sleeves, `/decks/built` with the
      shopping list. 2026-10-22: `plan_build` written; a built deck's
      proxies are swapped by hand only ("Use my copy"); saving a built deck
      takes its new cards from the other decks.
