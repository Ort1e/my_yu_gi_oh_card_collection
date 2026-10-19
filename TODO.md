# TODO

- [ ] Link decks together: a *built* flag on deck versions; the cards of a
      built version are locked (gone from the collection for every other
      deck; where other decks used them, they become proxies there), and
      `/decks/built` lists the built versions and their missing cards.
      Design: `docs/specs/built-decks.md` (deck box and sleeves too);
      backend (owner), then frontend.
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
      parse). Spec first, then backend (owner), then frontend.

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
