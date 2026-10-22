# TODO

- [ ] Structured tournament results instead of the free `tournament.notes`
      (the notes are already written the same way each time). Per
      tournament: rounds, each with the opponent's deck, die won/lost, the
      result (win / loss / double loss — no draws: when the round's time
      runs out unfinished, both players lose — bye, win by forfeit) and
      its games (won/lost, went first/second, a comment line); then the
      placement ("7th of 11"), the conclusion (stays free text), and for
      team events the team's result per round. Opposing decks tagged with
      archetype/engine labels, several per deck (`Mitsurugi` + `K9` for
      "mitsu/K9"), one label per strategy whatever the spelling ("mitsu",
      "mitsurugi"), chosen from the existing ones (new ones added on the
      way); own decks tagged the same way. Stats to follow (win rate per
      deck version, per label (matchups: "against K9", "with Orcust"), die
      and first/second). The 7
      existing tournaments' notes to move over (by hand or a one-off
      parse). **Spec: `docs/specs/tournaments.md`** (2026-10-22): rounds and
      games (draws possible, so more than 3), labels, `/tournaments` with the
      stats, old notes transcribed by Claude and loaded by a one-off command.
      The owner writes `domain::tournament::stats`, Claude the rest.
- [ ] Tidy the navigation bar (`templates/base.html`, `static/css/`): 10
      links once "Tournaments" is added, all at the same level. Group them
      (e.g. Collection: Cards, Lots, Sellers, Value, Rarities; Decks:
      Decks, Built, Tournaments; Budget; Advent), with a menu on phones.
## Done

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
- [x] Built decks (2026-10-20): a built version locks its cards (proxies
      in the other decks), deck box and sleeves, `/decks/built` with the
      shopping list. 2026-10-22: `plan_build` written; a built deck's
      proxies are swapped by hand only ("Use my copy"); saving a built deck
      takes its new cards from the other decks.
